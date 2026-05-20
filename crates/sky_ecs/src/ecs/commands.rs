use super::{Bundle, EntityId, World};
use crate::ecs::{component_type, ComponentType};
use rustc_hash::FxHashMap;
use smallvec::SmallVec;
use std::any::Any;
use std::mem::{self, MaybeUninit};
use std::ptr;

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

pub(crate) enum InsertValue {
    Inline {
        len: usize,
        bytes: [MaybeUninit<u8>; INLINE_INSERT_BYTES],
        drop_fn: Option<unsafe fn(*mut u8)>,
    },
    Heap {
        data: Box<[u8]>,
        drop_fn: Option<unsafe fn(*mut u8)>,
    },
    /// Sentinel: the value has been consumed (written to a chunk).
    Consumed,
}

impl InsertValue {
    fn from_value<T>(value: T) -> Self
    where
        T: 'static,
    {
        let len = mem::size_of::<T>();
        let drop_fn: Option<unsafe fn(*mut u8)> = if mem::needs_drop::<T>() {
            Some(sky_type::drop_in_place_erased::<T>)
        } else {
            None
        };

        // Wrap in ManuallyDrop so the original value isn't dropped after
        // we memcpy its bytes into our buffer.
        let value = mem::ManuallyDrop::new(value);

        if len <= INLINE_INSERT_BYTES {
            let mut bytes = [MaybeUninit::<u8>::uninit(); INLINE_INSERT_BYTES];
            unsafe {
                ptr::copy_nonoverlapping(
                    (&*value as *const T).cast::<u8>(),
                    bytes.as_mut_ptr().cast::<u8>(),
                    len,
                );
            }
            Self::Inline {
                len,
                bytes,
                drop_fn,
            }
        } else {
            let mut data = vec![0u8; len].into_boxed_slice();
            unsafe {
                ptr::copy_nonoverlapping(
                    (&*value as *const T).cast::<u8>(),
                    data.as_mut_ptr(),
                    len,
                );
            }
            Self::Heap { data, drop_fn }
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
                    ptr::copy_nonoverlapping(bytes.as_ptr().cast::<u8>(), dst, *len);
                    // Clear drop_fn BEFORE we replace self, so the implicit
                    // Drop triggered by `*self = Consumed` does NOT call
                    // the destructor on bytes that were already moved out.
                    *drop_fn = None;
                }
                Self::Heap { data, drop_fn } => {
                    ptr::copy_nonoverlapping(data.as_ptr(), dst, data.len());
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
                    drop_fn(bytes.as_mut_ptr().cast::<u8>());
                }
            }
            Self::Heap {
                data,
                drop_fn: Some(drop_fn),
            } => unsafe {
                drop_fn(data.as_mut_ptr());
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
// Public Commands API
// ---------------------------------------------------------------------------

/// A deferred command buffer for batching structural ECS changes.
///
/// Commands are recorded and then applied atomically via [`apply`](Self::apply).
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
/// # use sky_ecs::{World, Commands};
/// # #[derive(Clone, Copy)] struct Health(f32);
/// # let mut world = World::new();
/// let entity = world.spawn((Health(100.0),));
///
/// let mut cmds = Commands::new();
/// cmds.insert(entity, Health(50.0));
/// cmds.apply(&mut world);
///
/// assert_eq!(world.get::<Health>(entity).unwrap().0, 50.0);
/// ```
#[derive(Default)]
pub struct Commands {
    queue: Vec<Command>,
    queued_count: usize,
}

impl Commands {
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
    pub fn apply(&mut self, world: &mut World) {
        for command in self.queue.drain(..) {
            match command {
                Command::EntityBatch(batch) => batch.flush(world),
                Command::SpawnBatch(batch) => batch.apply(world),
                Command::Deferred(command) => command.apply(world),
            }
        }

        self.queued_count = 0;
    }
}
