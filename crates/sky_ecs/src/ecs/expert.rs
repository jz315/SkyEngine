//! Unsafe ECS escape hatches for benchmarks and engine-level tooling.
//!
//! Normal gameplay and tools should prefer the typed ECS API or
//! [`crate::ecs::dynamic`]. Everything in this module exposes storage-level
//! contracts directly to the caller.

use super::{world::World, EntityId};

pub use super::archetype::{create_archetype, Archetype, ArchetypeBuilder};
pub use super::chunk::Chunk;
pub use super::query::PreparedQuery;
pub use sky_type::{
    register as register_component_type, type_of as component_type, Type as ComponentType,
    TypeInfo as ComponentTypeInfo,
};

pub trait WorldExpertExt {
    /// Adds an entity with uninitialized component columns.
    ///
    /// # Safety
    ///
    /// The caller must initialize every component in `archetype` for the
    /// returned entity before the entity is queried, structurally moved,
    /// removed, or dropped.
    unsafe fn spawn_uninit(&mut self, archetype: Archetype) -> EntityId;
}

impl WorldExpertExt for World {
    unsafe fn spawn_uninit(&mut self, archetype: Archetype) -> EntityId {
        unsafe { World::add_entity(self, archetype) }
    }
}
