use super::{Archetype, EntityId, MAX_COMPONENTS};
use smallvec::SmallVec;
use std::alloc::{alloc_zeroed, dealloc, handle_alloc_error, Layout};
use std::ptr::{self, NonNull};

const CHUNK_SIZE: usize = 512 * 1024;

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
    entities: Vec<EntityId>,
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
            entities: Vec::with_capacity(max_entity_count),
        }
    }

    pub fn is_full(&self) -> bool {
        self.entity_count == self.max_entity_count
    }

    pub fn is_empty(&self) -> bool {
        self.entity_count == 0
    }

    pub fn add_entity(&mut self, entity: EntityId) -> Option<usize> {
        if self.entity_count == self.max_entity_count {
            return None;
        }

        let entity_index = self.entity_count;
        self.entity_count += 1;
        self.entities.push(entity);
        Some(entity_index)
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

    pub(crate) fn entities(&self) -> &[EntityId] {
        &self.entities[..self.entity_count]
    }

    pub fn get_entity_as_ptr(&self, index: usize) -> *const u8 {
        if self.archetype.components.is_empty() {
            return ptr::null();
        }

        self.component_ptr(0, index) as *const u8
    }

    pub fn entity_id(&self, entity_index: usize) -> Option<EntityId> {
        self.entities.get(entity_index).copied()
    }

    pub fn copy_entity_within(&mut self, src_index: usize, dst_index: usize) {
        if src_index == dst_index {
            return;
        }

        for (component_index, component) in self.archetype.components.iter().enumerate() {
            unsafe {
                ptr::copy_nonoverlapping(
                    self.component_ptr(component_index, src_index),
                    self.component_ptr(component_index, dst_index),
                    component.size,
                );
            }
        }

        self.entities[dst_index] = self.entities[src_index];
    }

    pub fn copy_entity_from(&mut self, src: &Chunk, src_index: usize, dst_index: usize) {
        debug_assert_eq!(self.archetype.id(), src.archetype.id());

        for (component_index, component) in self.archetype.components.iter().enumerate() {
            unsafe {
                ptr::copy_nonoverlapping(
                    src.component_ptr(component_index, src_index),
                    self.component_ptr(component_index, dst_index),
                    component.size,
                );
            }
        }

        self.entities[dst_index] = src.entities[src_index];
    }

    pub fn remove_last_entity(&mut self) -> Option<EntityId> {
        if self.entity_count == 0 {
            return None;
        }

        self.entity_count -= 1;
        self.entities.pop()
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

#[derive(Clone, Copy, Debug)]
pub struct ChunkEntityLocation {
    pub chunk_index: usize,
    pub entity_index: usize,
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

    pub fn add_entity(&mut self, entity: EntityId) -> ChunkEntityLocation {
        if let Some(chunk) = self.chunks.last_mut() {
            if let Some(entity_index) = chunk.add_entity(entity) {
                return ChunkEntityLocation {
                    chunk_index: self.chunks.len() - 1,
                    entity_index,
                };
            }
        }

        self.add_chunk();
        let chunk = self.chunks.last_mut().unwrap();
        let entity_index = chunk.add_entity(entity).unwrap();
        ChunkEntityLocation {
            chunk_index: self.chunks.len() - 1,
            entity_index,
        }
    }

    pub fn remove_entity(
        &mut self,
        location: ChunkEntityLocation,
    ) -> Option<(EntityId, ChunkEntityLocation)> {
        if self.chunks.is_empty() {
            return None;
        }

        let last_chunk_index = self.chunks.len() - 1;
        let last_entity_index = self.chunks[last_chunk_index].entity_count.checked_sub(1)?;

        let removed_is_last =
            location.chunk_index == last_chunk_index && location.entity_index == last_entity_index;

        let moved_entity = if removed_is_last {
            None
        } else {
            let moved_entity = self.chunks[last_chunk_index]
                .entity_id(last_entity_index)
                .unwrap();

            if location.chunk_index == last_chunk_index {
                self.chunks[last_chunk_index]
                    .copy_entity_within(last_entity_index, location.entity_index);
            } else {
                let (head, tail) = self.chunks.split_at_mut(last_chunk_index);
                let dst_chunk = &mut head[location.chunk_index];
                let src_chunk = &tail[0];
                dst_chunk.copy_entity_from(src_chunk, last_entity_index, location.entity_index);
            }

            Some((
                moved_entity,
                ChunkEntityLocation {
                    chunk_index: location.chunk_index,
                    entity_index: location.entity_index,
                },
            ))
        };

        self.chunks[last_chunk_index].remove_last_entity();
        if self.chunks.last().is_some_and(Chunk::is_empty) {
            self.chunks.pop();
        }

        moved_entity
    }
}

#[cfg(test)]
mod tests {
    use super::Chunk;
    use crate::{ecs::create_archetype, reflect};

    #[repr(align(16))]
    #[allow(dead_code)]
    struct Aligned16([u8; 16]);

    #[repr(align(8))]
    #[allow(dead_code)]
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
        let index_a = archetype.query_component_index(&ty_a).unwrap();
        let index_b = archetype.query_component_index(&ty_b).unwrap();

        let entity_a = crate::ecs::EntityId::new(0, 0);
        let entity_b = crate::ecs::EntityId::new(1, 0);

        assert!(chunk.add_entity(entity_a).is_some());
        assert!(chunk.add_entity(entity_b).is_some());

        assert!(chunk.max_entity_count > 0);
        assert_eq!(
            chunk.column_ptr(index_a) as usize % core::mem::align_of::<Aligned16>(),
            0
        );
        assert_eq!(
            chunk.column_ptr(index_b) as usize % core::mem::align_of::<Aligned8>(),
            0
        );
        assert_eq!(
            unsafe {
                chunk
                    .component_ptr(index_a, 1)
                    .offset_from(chunk.component_ptr(index_a, 0)) as usize
            },
            core::mem::size_of::<Aligned16>()
        );
    }
}
