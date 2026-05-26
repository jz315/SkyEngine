use crate::ecs::EntityId;

use super::PersistId;

/// Human-readable entity name used by persistence tools and debugging UI.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Name(pub String);

impl Name {
    #[inline]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    #[inline]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Runtime component storing the source document ID for an entity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PersistEntity {
    pub id: PersistId,
}

impl PersistEntity {
    #[inline]
    pub fn new(id: PersistId) -> Self {
        Self { id }
    }
}

/// Marker component inserted on loaded world or prefab root entities.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PersistRoot;

/// Parent link for persisted hierarchy.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Parent(pub EntityId);

impl Parent {
    #[inline]
    pub const fn new(entity: EntityId) -> Self {
        Self(entity)
    }

    #[inline]
    pub const fn entity(self) -> EntityId {
        self.0
    }
}

/// Child list for persisted hierarchy.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Children {
    pub entities: Vec<EntityId>,
}

impl Children {
    #[inline]
    pub fn new(entities: Vec<EntityId>) -> Self {
        Self { entities }
    }

    #[inline]
    pub fn as_slice(&self) -> &[EntityId] {
        &self.entities
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.entities.len()
    }
}
