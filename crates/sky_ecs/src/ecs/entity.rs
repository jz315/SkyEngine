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
    /// this way are not guaranteed to refer to a live entity in any
    /// [`World`](super::World).
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
    // Keep the random-access indirection table at four records per 64-byte
    // cache line. `u32::MAX` in data_index represents a vacant entity slot.
    pub generation: u32,
    data_index: u32,
    chunk_index: u32,
    entity_index: u32,
}

impl EntityRecord {
    const VACANT_DATA_INDEX: u32 = u32::MAX;

    #[inline(always)]
    pub(crate) fn vacant(generation: u32) -> Self {
        Self {
            generation,
            data_index: Self::VACANT_DATA_INDEX,
            chunk_index: 0,
            entity_index: 0,
        }
    }

    #[inline(always)]
    pub(crate) fn location(self) -> Option<EntityLocation> {
        if self.data_index == Self::VACANT_DATA_INDEX {
            return None;
        }

        Some(EntityLocation {
            data_index: self.data_index as usize,
            chunk_index: self.chunk_index as usize,
            entity_index: self.entity_index as usize,
        })
    }

    #[inline(always)]
    pub(crate) fn set_location(&mut self, location: EntityLocation) {
        debug_assert!(location.data_index < Self::VACANT_DATA_INDEX as usize);
        debug_assert!(location.chunk_index <= u32::MAX as usize);
        debug_assert!(location.entity_index <= u32::MAX as usize);

        self.data_index = location.data_index as u32;
        self.chunk_index = location.chunk_index as u32;
        self.entity_index = location.entity_index as u32;
    }

    #[inline(always)]
    pub(crate) fn clear_location(&mut self) {
        self.data_index = Self::VACANT_DATA_INDEX;
    }

    #[inline(always)]
    pub(crate) fn is_alive(self) -> bool {
        self.data_index != Self::VACANT_DATA_INDEX
    }
}

#[cfg(test)]
mod tests {
    use super::{EntityLocation, EntityRecord};

    #[test]
    fn entity_record_stays_cache_dense() {
        assert_eq!(std::mem::size_of::<EntityRecord>(), 16);

        let mut record = EntityRecord::vacant(7);
        assert!(!record.is_alive());
        assert!(record.location().is_none());

        let location = EntityLocation {
            data_index: 3,
            chunk_index: 5,
            entity_index: 8,
        };
        record.set_location(location);
        assert!(record.is_alive());
        let actual = record.location().unwrap();
        assert_eq!(actual.data_index, location.data_index);
        assert_eq!(actual.chunk_index, location.chunk_index);
        assert_eq!(actual.entity_index, location.entity_index);

        record.clear_location();
        assert!(!record.is_alive());
        assert!(record.location().is_none());
    }
}
