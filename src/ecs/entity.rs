#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EntityId {
    index: u32,
    generation: u32,
}

impl EntityId {
    pub(crate) fn new(index: u32, generation: u32) -> Self {
        Self { index, generation }
    }

    #[inline(always)]
    pub fn index(self) -> u32 {
        self.index
    }

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
