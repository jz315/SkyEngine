use super::{Archetype, MAX_COMPONENTS};
use smallvec::SmallVec;
use std::alloc::{alloc_zeroed, dealloc, handle_alloc_error, Layout};
use std::ptr::{self, NonNull};

const CHUNK_SIZE: usize = 48 * 1024;

fn align_up(value: usize, align: usize) -> usize {
    debug_assert!(align.is_power_of_two());
    (value + align - 1) & !(align - 1)
}

pub struct Chunk {
    pub entity_count: usize,
    pub max_entity_count: usize,
    pub archetype: Archetype,
    data: NonNull<u8>,
    alloc_layout: Layout,
    column_offsets: SmallVec<[usize; MAX_COMPONENTS]>,
}

impl Chunk {
    fn compute_column_offsets(
        archetype: Archetype,
        entity_capacity: usize,
    ) -> Option<SmallVec<[usize; MAX_COMPONENTS]>> {
        let mut offsets = SmallVec::with_capacity(archetype.components.len());
        let mut cursor = 0usize;

        for component in &archetype.components {
            cursor = align_up(cursor, component.align);

            let bytes = component.size.checked_mul(entity_capacity)?;
            let end = cursor.checked_add(bytes)?;
            if end > CHUNK_SIZE {
                return None;
            }

            offsets.push(cursor);
            cursor = end;
        }

        Some(offsets)
    }

    fn compute_chunk_layout(archetype: Archetype) -> (usize, SmallVec<[usize; MAX_COMPONENTS]>) {
        let component_bytes: usize = archetype
            .components
            .iter()
            .map(|component| component.size)
            .sum();

        if component_bytes == 0 {
            return (0, SmallVec::new());
        }

        let mut entity_capacity = (CHUNK_SIZE / component_bytes).max(1);

        while entity_capacity > 0 {
            if let Some(column_offsets) = Self::compute_column_offsets(archetype, entity_capacity) {
                return (entity_capacity, column_offsets);
            }
            entity_capacity -= 1;
        }

        panic!("archetype is too large to fit in a chunk");
    }

    pub fn new(archetype: Archetype) -> Self {
        let (max_entity_count, column_offsets) = Self::compute_chunk_layout(archetype);
        let alloc_layout = Layout::from_size_align(CHUNK_SIZE, archetype.alignment.max(1)).unwrap();

        let data = unsafe {
            let raw = alloc_zeroed(alloc_layout);
            NonNull::new(raw).unwrap_or_else(|| handle_alloc_error(alloc_layout))
        };

        Self {
            entity_count: 0,
            max_entity_count,
            archetype,
            data,
            alloc_layout,
            column_offsets,
        }
    }

    pub fn is_full(&self) -> bool {
        self.entity_count == self.max_entity_count
    }

    pub fn is_empty(&self) -> bool {
        self.entity_count == 0
    }

    pub fn add_entity(&mut self) -> bool {
        if self.entity_count == self.max_entity_count {
            return false;
        }

        self.entity_count += 1;
        true
    }

    pub fn column_ptr(&self, component_index: usize) -> *mut u8 {
        unsafe { self.data.as_ptr().add(self.column_offsets[component_index]) }
    }

    pub fn component_ptr(&self, component_index: usize, entity_index: usize) -> *mut u8 {
        if entity_index >= self.entity_count {
            return ptr::null_mut();
        }

        let component = &self.archetype.components[component_index];
        unsafe {
            self.column_ptr(component_index)
                .add(entity_index * component.size)
        }
    }

    pub fn get_entity() {
        todo!()
    }

    pub fn get_entity_as_ptr(&self, index: usize) -> *const u8 {
        if self.archetype.components.is_empty() {
            return ptr::null();
        }

        self.component_ptr(0, index) as *const u8
    }
}

impl Drop for Chunk {
    fn drop(&mut self) {
        unsafe {
            dealloc(self.data.as_ptr(), self.alloc_layout);
        }
    }
}

pub struct Data {
    pub archetype: Archetype,
    pub chunks: Vec<Chunk>,
}

impl Data {
    pub fn new(archetype: Archetype) -> Self {
        Self {
            archetype,
            chunks: Vec::new(),
        }
    }

    fn add_chunk(&mut self) {
        self.chunks.push(Chunk::new(self.archetype));
    }

    pub fn add_entity(&mut self) {
        if let Some(chunk) = self.chunks.last_mut() {
            if chunk.add_entity() {
                return;
            }
        }

        self.add_chunk();
        let added = self.chunks.last_mut().unwrap().add_entity();
        debug_assert!(added);
    }
}

pub struct EntityIter<'a> {
    data: &'a Data,
    current_chunk_index: usize,
    current_entity_index: usize,
}

impl<'a> EntityIter<'a> {
    pub fn new(data: &'a Data) -> Self {
        Self {
            data,
            current_chunk_index: 0,
            current_entity_index: 0,
        }
    }
}

impl<'a> Iterator for EntityIter<'a> {
    type Item = *const u8;

    fn next(&mut self) -> Option<Self::Item> {
        while self.current_chunk_index < self.data.chunks.len() {
            let chunk = &self.data.chunks[self.current_chunk_index];

            if self.current_entity_index < chunk.entity_count {
                let entity_index = self.current_entity_index;
                self.current_entity_index += 1;
                return Some(chunk.get_entity_as_ptr(entity_index));
            }

            self.current_chunk_index += 1;
            self.current_entity_index = 0;
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::Chunk;
    use crate::{ecs::create_archetype, reflect};

    #[repr(align(16))]
    struct Aligned16([u8; 16]);

    #[repr(align(8))]
    struct Aligned8([u8; 8]);

    #[test]
    fn chunk_columns_respect_component_alignment() {
        let ty_a = reflect::register(
            "chunk_test_aligned16",
            core::mem::size_of::<Aligned16>(),
            core::mem::align_of::<Aligned16>(),
        );
        let ty_b = reflect::register(
            "chunk_test_aligned8",
            core::mem::size_of::<Aligned8>(),
            core::mem::align_of::<Aligned8>(),
        );

        let archetype = create_archetype()
            .add_component(ty_a)
            .add_component(ty_b)
            .build();
        let mut chunk = Chunk::new(archetype);

        assert!(chunk.add_entity());
        assert!(chunk.add_entity());

        assert!(chunk.max_entity_count > 0);
        assert_eq!(
            chunk.column_ptr(0) as usize % core::mem::align_of::<Aligned16>(),
            0
        );
        assert_eq!(
            chunk.column_ptr(1) as usize % core::mem::align_of::<Aligned8>(),
            0
        );
        assert_eq!(
            unsafe {
                chunk
                    .component_ptr(0, 1)
                    .offset_from(chunk.component_ptr(0, 0)) as usize
            },
            core::mem::size_of::<Aligned16>()
        );
    }
}
