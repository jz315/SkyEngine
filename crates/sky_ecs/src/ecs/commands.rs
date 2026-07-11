use super::{Bundle, EntityId, World};
use crate::ecs::{component_type, ComponentType};
use rustc_hash::FxHashMap;
use smallvec::SmallVec;
use std::alloc::{alloc, dealloc, handle_alloc_error, Layout};
use std::any::Any;
use std::mem::{self, MaybeUninit};
use std::ptr;
use std::ptr::NonNull;

// ---------------------------------------------------------------------------
// Deferred command trait – closures applied to &mut World
// ---------------------------------------------------------------------------

trait DeferredWorldCommand {
    fn apply(self: Box<Self>, world: &mut World);
}

struct FnDeferredCommand<F>(F);

impl<F> DeferredWorldCommand for FnDeferredCommand<F>
where
    F: FnOnce(&mut World) + 'static,
{
    fn apply(self: Box<Self>, world: &mut World) {
        (self.0)(world);
    }
}

// ---------------------------------------------------------------------------
// Spawn-batch coalescing
// ---------------------------------------------------------------------------

trait SpawnBatchCommand {
    fn apply(self: Box<Self>, world: &mut World);
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

struct TypedSpawnBatch<B> {
    bundles: Vec<B>,
}

impl<B> SpawnBatchCommand for TypedSpawnBatch<B>
where
    B: Bundle,
{
    fn apply(self: Box<Self>, world: &mut World) {
        world.spawn_batch(self.bundles);
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

// ---------------------------------------------------------------------------
// InsertValue – inline-or-heap component payload
// ---------------------------------------------------------------------------

const INLINE_INSERT_BYTES: usize = 16;

#[repr(align(16))]
pub(crate) struct InlineInsertBytes([MaybeUninit<u8>; INLINE_INSERT_BYTES]);

pub(crate) enum InsertValue {
    Inline {
        len: usize,
        bytes: InlineInsertBytes,
        drop_fn: Option<unsafe fn(*mut u8)>,
    },
    Heap {
        data: NonNull<u8>,
        len: usize,
        layout: Layout,
        drop_fn: Option<unsafe fn(*mut u8)>,
    },
    /// Sentinel: the value has been consumed (written to a chunk).
    Consumed,
}

impl InsertValue {
    pub(crate) fn from_value<T>(value: T) -> Self
    where
        T: 'static,
    {
        let len = mem::size_of::<T>();
        let align = mem::align_of::<T>();
        let drop_fn: Option<unsafe fn(*mut u8)> = if mem::needs_drop::<T>() {
            Some(sky_type::drop_in_place_erased::<T>)
        } else {
            None
        };

        // Wrap in ManuallyDrop so the original value isn't dropped after
        // we memcpy its bytes into our buffer.
        let value = mem::ManuallyDrop::new(value);

        if len <= INLINE_INSERT_BYTES && align <= mem::align_of::<InlineInsertBytes>() {
            let mut bytes = InlineInsertBytes([MaybeUninit::<u8>::uninit(); INLINE_INSERT_BYTES]);
            unsafe {
                ptr::copy_nonoverlapping(
                    (&*value as *const T).cast::<u8>(),
                    bytes.0.as_mut_ptr().cast::<u8>(),
                    len,
                );
            }
            Self::Inline {
                len,
                bytes,
                drop_fn,
            }
        } else {
            let layout = Layout::from_size_align(len.max(1), align).unwrap();
            let data = unsafe {
                let raw = alloc(layout);
                NonNull::new(raw).unwrap_or_else(|| handle_alloc_error(layout))
            };
            unsafe {
                ptr::copy_nonoverlapping((&*value as *const T).cast::<u8>(), data.as_ptr(), len);
            }
            Self::Heap {
                data,
                len,
                layout,
                drop_fn,
            }
        }
    }

    /// Write the stored bytes to `dst` and mark this value as consumed.
    ///
    /// After this call, ownership of the component value has transferred
    /// to the destination buffer; this `InsertValue` will NOT run the
    /// type-erased drop.
    #[inline(always)]
    pub(crate) fn write(&mut self, dst: *mut u8) {
        // Safety: we copy the stored bytes to the destination.
        // The destination is expected to be a properly-aligned,
        // uninitialised (or already-dropped) slot.
        unsafe {
            match self {
                Self::Inline {
                    len,
                    bytes,
                    drop_fn,
                } => {
                    ptr::copy_nonoverlapping(bytes.0.as_ptr().cast::<u8>(), dst, *len);
                    // Clear drop_fn BEFORE we replace self, so the implicit
                    // Drop triggered by `*self = Consumed` does NOT call
                    // the destructor on bytes that were already moved out.
                    *drop_fn = None;
                }
                Self::Heap {
                    data, len, drop_fn, ..
                } => {
                    ptr::copy_nonoverlapping(data.as_ptr(), dst, *len);
                    *drop_fn = None;
                }
                Self::Consumed => {
                    debug_assert!(false, "InsertValue::write called on a consumed value");
                    return;
                }
            }
        }

        // Mark as consumed (no-op from drop perspective since drop_fn is
        // already None, but semantically clearer).
        *self = Self::Consumed;
    }
}

impl Drop for InsertValue {
    fn drop(&mut self) {
        match self {
            Self::Inline {
                bytes,
                drop_fn: Some(drop_fn),
                ..
            } => {
                // Safety: the value was never consumed and the bytes
                // represent a valid, initialised value of the original type.
                unsafe {
                    drop_fn(bytes.0.as_mut_ptr().cast::<u8>());
                }
            }
            Self::Heap {
                data,
                layout,
                drop_fn: Some(drop_fn),
                ..
            } => unsafe {
                drop_fn(data.as_ptr());
                dealloc(data.as_ptr(), *layout);
            },
            Self::Heap { data, layout, .. } => unsafe {
                dealloc(data.as_ptr(), *layout);
            },
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Command queue
// ---------------------------------------------------------------------------

enum Command {
    EntityBatch(PendingEntityBuffer),
    SpawnBatch(Box<dyn SpawnBatchCommand>),
    Deferred(Box<dyn DeferredWorldCommand>),
}

enum EntityCommand {
    Despawn(EntityId),
    Insert {
        entity: EntityId,
        component: ComponentType,
        value: InsertValue,
    },
    Remove {
        entity: EntityId,
        component: ComponentType,
    },
}

// ---------------------------------------------------------------------------
// Per-entity coalesced command state
// ---------------------------------------------------------------------------

enum PendingComponentCommand {
    Insert(InsertValue),
    Remove,
}

struct PendingComponentEntry {
    component: ComponentType,
    command: PendingComponentCommand,
}

#[derive(Default)]
struct PendingEntityCommands {
    despawn: bool,
    components: SmallVec<[PendingComponentEntry; 4]>,
}

impl PendingEntityCommands {
    fn queue_insert(&mut self, component: ComponentType, value: InsertValue) {
        if self.despawn {
            return;
        }

        if let Some(pending) = self
            .components
            .iter_mut()
            .find(|p| p.component.id() == component.id())
        {
            pending.command = PendingComponentCommand::Insert(value);
            return;
        }

        self.components.push(PendingComponentEntry {
            component,
            command: PendingComponentCommand::Insert(value),
        });
    }

    fn queue_remove(&mut self, component: ComponentType) {
        if self.despawn {
            return;
        }

        if let Some(pending) = self
            .components
            .iter_mut()
            .find(|p| p.component.id() == component.id())
        {
            pending.command = PendingComponentCommand::Remove;
            return;
        }

        self.components.push(PendingComponentEntry {
            component,
            command: PendingComponentCommand::Remove,
        });
    }

    fn queue_despawn(&mut self) {
        self.despawn = true;
        self.components.clear();
    }
}

// ---------------------------------------------------------------------------
// PendingEntityBuffer – Vec-backed coalesced entity command buffer
//
// Primary storage is `entries`, preserving first-seen entity order.
// `index` is a dedup-only map from EntityId to position in `entries`.
// ---------------------------------------------------------------------------

#[derive(Default)]
struct PendingEntityBuffer {
    entries: Vec<(EntityId, PendingEntityCommands)>,
    index: FxHashMap<EntityId, u32>,
}

impl PendingEntityBuffer {
    fn push(&mut self, command: EntityCommand) {
        let entity = match &command {
            EntityCommand::Despawn(entity) => *entity,
            EntityCommand::Insert { entity, .. } => *entity,
            EntityCommand::Remove { entity, .. } => *entity,
        };

        let pending = if let Some(&pos) = self.index.get(&entity) {
            &mut self.entries[pos as usize].1
        } else {
            let pos = self.entries.len();
            debug_assert!(
                pos <= u32::MAX as usize,
                "PendingEntityBuffer: entry count exceeds u32 index capacity"
            );
            self.index.insert(entity, pos as u32);
            self.entries
                .push((entity, PendingEntityCommands::default()));
            &mut self.entries.last_mut().unwrap().1
        };

        match command {
            EntityCommand::Despawn(_) => pending.queue_despawn(),
            EntityCommand::Insert {
                component, value, ..
            } => pending.queue_insert(component, value),
            EntityCommand::Remove { component, .. } => pending.queue_remove(component),
        }
    }

    fn flush(self, world: &mut World) {
        for (entity, pending) in self.entries {
            if pending.despawn {
                world.despawn(entity);
                continue;
            }

            for entry in pending.components {
                match entry.command {
                    PendingComponentCommand::Insert(mut value) => {
                        world.insert_dynamic(entity, entry.component, &mut value);
                    }
                    PendingComponentCommand::Remove => {
                        world.remove_dynamic(entity, entry.component);
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Owned command buffer
// ---------------------------------------------------------------------------

/// A deferred command buffer for batching structural ECS changes.
///
/// Commands are recorded and then applied in one explicit pass via
/// [`apply`](Self::apply).
/// This is useful when you need to make structural changes (spawn, despawn,
/// insert, remove) from within a query loop or a system, where direct
/// mutation of the [`World`] is not possible.
///
/// Commands targeting the same entity are **coalesced** — only the final
/// state per component is applied, reducing archetype migrations.
///
/// # Examples
///
/// ```
/// # use sky_ecs::{CommandBuffer, World};
/// # #[derive(Clone, Copy)] struct Health(f32);
/// # let mut world = World::new();
/// let entity = world.spawn((Health(100.0),));
///
/// let mut cmds = CommandBuffer::new();
/// cmds.insert(entity, Health(50.0));
/// cmds.apply(&mut world);
///
/// assert_eq!(world.get::<Health>(entity).unwrap().0, 50.0);
/// ```
#[derive(Default)]
pub struct CommandBuffer {
    queue: Vec<Command>,
    queued_count: usize,
}

struct CommandApplyGuard {
    world: *mut World,
    completed: bool,
}

impl CommandApplyGuard {
    fn new(world: &mut World) -> Self {
        world.assert_command_apply_allowed();
        Self {
            world: std::ptr::from_mut(world),
            completed: false,
        }
    }

    fn complete(mut self) {
        self.completed = true;
    }
}

impl Drop for CommandApplyGuard {
    fn drop(&mut self) {
        if !self.completed && std::thread::panicking() {
            // Safety: the guard never outlives CommandBuffer::apply's borrowed
            // World, and apply cannot return while this guard is alive.
            unsafe { &mut *self.world }.poison_after_command_panic();
        }
    }
}

impl CommandBuffer {
    /// Creates a new, empty command buffer.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns `true` if no commands have been recorded.
    pub fn is_empty(&self) -> bool {
        self.queued_count == 0
    }

    /// Returns the number of commands recorded so far.
    pub fn len(&self) -> usize {
        self.queued_count
    }

    fn push_deferred<F>(&mut self, f: F)
    where
        F: FnOnce(&mut World) + 'static,
    {
        self.queued_count += 1;
        self.queue
            .push(Command::Deferred(Box::new(FnDeferredCommand(f))));
    }

    fn push_entity(&mut self, command: EntityCommand) {
        self.queued_count += 1;

        if let Some(Command::EntityBatch(batch)) = self.queue.last_mut() {
            batch.push(command);
            return;
        }

        let mut batch = PendingEntityBuffer::default();
        batch.push(command);
        self.queue.push(Command::EntityBatch(batch));
    }

    /// Records a deferred spawn.  Consecutive spawns of the same bundle
    /// type are coalesced into a single batch for efficiency.
    pub fn spawn<B>(&mut self, bundle: B)
    where
        B: Bundle,
    {
        self.queued_count += 1;

        if let Some(Command::SpawnBatch(batch)) = self.queue.last_mut() {
            if let Some(batch) = batch.as_any_mut().downcast_mut::<TypedSpawnBatch<B>>() {
                batch.bundles.push(bundle);
                return;
            }
        }

        self.queue
            .push(Command::SpawnBatch(Box::new(TypedSpawnBatch {
                bundles: vec![bundle],
            })));
    }

    /// Records a deferred entity despawn.
    pub fn despawn(&mut self, entity: EntityId) {
        self.push_entity(EntityCommand::Despawn(entity));
    }

    /// Records a deferred component insertion (or overwrite).
    pub fn insert<T>(&mut self, entity: EntityId, component: T)
    where
        T: 'static,
    {
        self.push_entity(EntityCommand::Insert {
            entity,
            component: component_type::<T>(),
            value: InsertValue::from_value(component),
        });
    }

    /// Records a deferred component removal.
    pub fn remove<T>(&mut self, entity: EntityId)
    where
        T: 'static,
    {
        self.push_entity(EntityCommand::Remove {
            entity,
            component: component_type::<T>(),
        });
    }

    /// Records a deferred resource insertion.
    pub fn insert_resource<R>(&mut self, resource: R)
    where
        R: 'static,
    {
        self.push_deferred(move |world| {
            world.insert_resource(resource);
        });
    }

    /// Records a deferred resource removal.
    pub fn remove_resource<R>(&mut self)
    where
        R: 'static,
    {
        self.push_deferred(move |world| {
            world.remove_resource::<R>();
        });
    }

    /// Applies all recorded commands to the world and clears the buffer.
    ///
    /// # Panics and poisoning
    ///
    /// Commands may contain arbitrary user values whose code or destructors
    /// can panic. General rollback is therefore impossible. If a panic escapes
    /// this apply pass, the World is marked poisoned and rejects later command
    /// application and schedule ticks rather than running with a partial
    /// commit. The World remains inspectable and may be shut down.
    pub fn apply(&mut self, world: &mut World) {
        let apply_guard = CommandApplyGuard::new(world);
        // Restore the observable invariant before user-owned drops/deferred
        // work can unwind. On success the emptied Vec (and its capacity) is
        // returned; on panic `self` already contains a valid empty queue.
        self.queued_count = 0;
        let mut queue = std::mem::take(&mut self.queue);
        for command in queue.drain(..) {
            match command {
                Command::EntityBatch(batch) => batch.flush(world),
                Command::SpawnBatch(batch) => batch.apply(world),
                Command::Deferred(command) => command.apply(world),
            }
        }
        self.queue = queue;
        apply_guard.complete();
    }

    /// Discards every pending command while preserving allocated capacity.
    pub fn clear(&mut self) {
        self.queued_count = 0;
        let mut queue = std::mem::take(&mut self.queue);
        queue.clear();
        self.queue = queue;
    }
}

// ---------------------------------------------------------------------------
// Borrowed system command writer
// ---------------------------------------------------------------------------

/// A system-local deferred structural writer.
///
/// Each scheduled system receives an isolated writer. The scheduler applies
/// the underlying buffers in deterministic registration order at the next
/// documented flush boundary.
pub struct Commands<'w> {
    buffer: &'w mut CommandBuffer,
}

impl<'w> Commands<'w> {
    pub(crate) unsafe fn from_ptr(buffer: *mut CommandBuffer) -> Self {
        Self {
            buffer: unsafe { &mut *buffer },
        }
    }

    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    pub fn spawn<B>(&mut self, bundle: B)
    where
        B: Bundle + Send,
    {
        self.buffer.spawn(bundle);
    }

    pub fn despawn(&mut self, entity: EntityId) {
        self.buffer.despawn(entity);
    }

    pub fn insert<T>(&mut self, entity: EntityId, component: T)
    where
        T: Send + 'static,
    {
        self.buffer.insert(entity, component);
    }

    pub fn remove<T>(&mut self, entity: EntityId)
    where
        T: 'static,
    {
        self.buffer.remove::<T>(entity);
    }

    pub fn insert_resource<R>(&mut self, resource: R)
    where
        R: Send + 'static,
    {
        self.buffer.insert_resource(resource);
    }

    pub fn remove_resource<R>(&mut self)
    where
        R: 'static,
    {
        self.buffer.remove_resource::<R>();
    }
}
