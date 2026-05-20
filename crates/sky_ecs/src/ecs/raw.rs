use super::{world::World, EntityId};

pub use super::archetype::{create_archetype, Archetype, ArchetypeBuilder};
pub use super::chunk::Chunk;
pub use super::query::{PreparedQuery, Query, QueryIter};
pub use sky_type::{
    register as register_component_type, type_of as component_type, Type as ComponentType,
    TypeInfo as ComponentTypeInfo,
};

pub trait WorldRawExt {
    fn add_entity(&mut self, archetype: Archetype) -> EntityId;
}

impl WorldRawExt for World {
    fn add_entity(&mut self, archetype: Archetype) -> EntityId {
        World::add_entity(self, archetype)
    }
}
