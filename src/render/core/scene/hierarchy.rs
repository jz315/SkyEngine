use crate::ecs::EntityId;

/// Scene hierarchy link used by the unified renderer.
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
