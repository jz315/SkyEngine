/// A lightweight, generational handle to an entity in a [`World`](super::World).
///
/// Entity IDs are composed of a **slot index** (which may be reused) and a
/// **generation counter** that increments each time the slot is recycled.
/// This means stale IDs are automatically detected — calling
/// [`World::get`](super::World::get) with a despawned entity's ID returns
/// `None` rather than accessing a different entity that reused the slot.
///
/// `EntityId` is `Copy`, `Eq`, and `Hash`, so it can be used freely as a
/// key in collections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EntityId {
    index: u32,
    generation: u32,
}

impl EntityId {
    /// Creates an entity id from its raw slot index and generation.
    ///
    /// This is mainly for low-level tests and renderer sort keys. IDs created
    /// this way are not guaranteed to refer to a live entity in any [`World`].
    pub const fn new(index: u32, generation: u32) -> Self {
        Self { index, generation }
    }

    /// Returns the raw slot index.
    ///
    /// This is an internal detail and should not be relied upon for identity
    /// comparisons — use `PartialEq` instead.
    #[inline(always)]
    pub fn index(self) -> u32 {
        self.index
    }

    /// Returns the generation counter for this entity.
    ///
    /// The generation increments each time the slot is reused after a
    /// despawn, allowing stale handles to be detected.
    #[inline(always)]
    pub fn generation(self) -> u32 {
        self.generation
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct EntityLocation {
    pub data_index: usize,
    pub chunk_index: usize,
    pub entity_index: usize,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct EntityRecord {
    pub generation: u32,
    pub location: Option<EntityLocation>,
}
