use super::{Bundle, EntityId, World};
use crate::reflect::{register_rust_type, Type};
use smallvec::SmallVec;
use std::any::Any;
use rustc_hash::FxHashMap;
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
    },
    Heap(Box<[u8]>),
}

impl InsertValue {
    fn from_value<T>(value: T) -> Self
    where
        T: Copy + 'static,
    {
        let len = mem::size_of::<T>();
        if len <= INLINE_INSERT_BYTES {
            let mut bytes = [MaybeUninit::<u8>::uninit(); INLINE_INSERT_BYTES];
            unsafe {
                ptr::copy_nonoverlapping(
                    (&value as *const T).cast::<u8>(),
                    bytes.as_mut_ptr().cast::<u8>(),
                    len,
                );
            }
            Self::Inline { len, bytes }
        } else {
            let mut bytes = vec![0u8; len].into_boxed_slice();
            unsafe {
                ptr::copy_nonoverlapping(
                    (&value as *const T).cast::<u8>(),
                    bytes.as_mut_ptr(),
                    len,
                );
            }
            Self::Heap(bytes)
        }
    }

    #[inline(always)]
    pub(crate) fn write(&self, dst: *mut u8) {
        unsafe {
            match self {
                Self::Inline { len, bytes } => {
                    ptr::copy_nonoverlapping(bytes.as_ptr().cast::<u8>(), dst, *len);
                }
                Self::Heap(bytes) => {
                    ptr::copy_nonoverlapping(bytes.as_ptr(), dst, bytes.len());
                }
            }
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
        component: Type,
        value: InsertValue,
    },
    Remove {
        entity: EntityId,
        component: Type,
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
    component: Type,
    command: PendingComponentCommand,
}

#[derive(Default)]
struct PendingEntityCommands {
    despawn: bool,
    components: SmallVec<[PendingComponentEntry; 4]>,
}

impl PendingEntityCommands {
    fn queue_insert(&mut self, component: Type, value: InsertValue) {
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

    fn queue_remove(&mut self, component: Type) {
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
            self.entries.push((entity, PendingEntityCommands::default()));
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
                    PendingComponentCommand::Insert(value) => {
                        world.insert_dynamic(entity, entry.component, &value);
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

#[derive(Default)]
pub struct Commands {
    queue: Vec<Command>,
    queued_count: usize,
}

impl Commands {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.queued_count == 0
    }

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

    pub fn despawn(&mut self, entity: EntityId) {
        self.push_entity(EntityCommand::Despawn(entity));
    }

    pub fn insert<T>(&mut self, entity: EntityId, component: T)
    where
        T: Copy + 'static,
    {
        self.push_entity(EntityCommand::Insert {
            entity,
            component: register_rust_type::<T>(),
            value: InsertValue::from_value(component),
        });
    }

    pub fn remove<T>(&mut self, entity: EntityId)
    where
        T: 'static,
    {
        self.push_entity(EntityCommand::Remove {
            entity,
            component: register_rust_type::<T>(),
        });
    }

    pub fn insert_resource<R>(&mut self, resource: R)
    where
        R: 'static,
    {
        self.push_deferred(move |world| {
            world.insert_resource(resource);
        });
    }

    pub fn remove_resource<R>(&mut self)
    where
        R: 'static,
    {
        self.push_deferred(move |world| {
            world.remove_resource::<R>();
        });
    }

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
