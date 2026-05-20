use super::{Archetype, EntityId, MAX_COMPONENTS};
use smallvec::SmallVec;
use std::alloc::{alloc_zeroed, dealloc, handle_alloc_error, Layout};
use std::cell::RefCell;
use std::cmp::Ordering;
use std::mem;
use std::ptr::{self, NonNull};

pub(crate) const CHUNK_SIZE: usize = 512 * 1024;
const BACKING_POOL_BUDGET_BYTES: usize = 4 * 1024 * 1024;

thread_local! {
    static CHUNK_BLOCK_POOL: RefCell<ChunkBlockPool> =
        RefCell::new(ChunkBlockPool::default());
}

fn align_up(value: usize, align: usize) -> usize {
    debug_assert!(align.is_power_of_two());
    (value + align - 1) & !(align - 1)
}

struct ChunkBlock {
    data: NonNull<u8>,
    alloc_layout: Layout,
}

impl ChunkBlock {
    fn fresh(alignment: usize) -> Self {
        let alloc_layout = Layout::from_size_align(CHUNK_SIZE, alignment.max(1)).unwrap();
        let data = unsafe {
            let raw = alloc_zeroed(alloc_layout);
            NonNull::new(raw).unwrap_or_else(|| handle_alloc_error(alloc_layout))
        };

        Self { data, alloc_layout }
    }

    #[inline(always)]
    fn retained_size(&self) -> usize {
        CHUNK_SIZE
    }

    #[inline(always)]
    fn alignment(&self) -> usize {
        self.alloc_layout.align()
    }

    fn into_raw_parts(self) -> (NonNull<u8>, Layout) {
        let data = self.data;
        let alloc_layout = self.alloc_layout;
        mem::forget(self);
        (data, alloc_layout)
    }
}

impl Drop for ChunkBlock {
    fn drop(&mut self) {
        unsafe {
            dealloc(self.data.as_ptr(), self.alloc_layout);
        }
    }
}

struct PooledBlock {
    block: ChunkBlock,
    last_used: u64,
}

#[derive(Default)]
struct ChunkBlockPool {
    entries: Vec<PooledBlock>,
    retained_bytes: usize,
    clock: u64,
}

#[derive(Clone)]
pub(crate) struct ChunkLayout {
    max_entity_count: usize,
    column_offsets: SmallVec<[usize; MAX_COMPONENTS]>,
}

impl ChunkLayout {
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

    fn for_archetype(archetype: Archetype) -> Self {
        let component_bytes: usize = archetype
            .components
            .iter()
            .map(|component| component.size)
            .sum();

        if component_bytes == 0 {
            return Self {
                max_entity_count: 0,
                column_offsets: SmallVec::new(),
            };
        }

        let mut entity_capacity = (CHUNK_SIZE / component_bytes).max(1);

        while entity_capacity > 0 {
            if let Some(column_offsets) = Self::compute_column_offsets(archetype, entity_capacity) {
                return Self {
                    max_entity_count: entity_capacity,
                    column_offsets,
                };
            }
            entity_capacity -= 1;
        }

        panic!("archetype is too large to fit in a chunk");
    }

    #[inline(always)]
    pub(crate) fn column_offset(&self, component_index: usize) -> usize {
        self.column_offsets[component_index]
    }
}

impl ChunkBlockPool {
    #[inline(always)]
    fn next_tick(&mut self) -> u64 {
        let tick = self.clock;
        self.clock = self.clock.wrapping_add(1);
        tick
    }

    fn acquire(&mut self, alignment: usize) -> Option<ChunkBlock> {
        let mut best: Option<(usize, usize)> = None;

        for (index, entry) in self.entries.iter().enumerate() {
            let entry_alignment = entry.block.alignment();
            if entry_alignment < alignment {
                continue;
            }

            let replace = best.is_none_or(|(_, best_alignment)| entry_alignment < best_alignment);
            if replace {
                best = Some((index, entry_alignment));
            }
        }

        let index = best.map(|(index, _)| index)?;
        let entry = self.entries.swap_remove(index);
        self.retained_bytes -= entry.block.retained_size();
        Some(entry.block)
    }

    fn evict_oldest(&mut self) -> bool {
        let Some((oldest_index, _)) = self
            .entries
            .iter()
            .enumerate()
            .min_by_key(|(_, entry)| entry.last_used)
        else {
            return false;
        };

        let entry = self.entries.swap_remove(oldest_index);
        self.retained_bytes -= entry.block.retained_size();
        true
    }

    fn release(&mut self, block: ChunkBlock) {
        let retained_size = block.retained_size();
        if retained_size > BACKING_POOL_BUDGET_BYTES {
            return;
        }

        while self.retained_bytes + retained_size > BACKING_POOL_BUDGET_BYTES {
            if !self.evict_oldest() {
                return;
            }
        }

        let last_used = self.next_tick();
        self.retained_bytes += retained_size;
        self.entries.push(PooledBlock { block, last_used });
    }

    #[cfg(test)]
    fn clear(&mut self) {
        self.entries.clear();
        self.retained_bytes = 0;
        self.clock = 0;
    }
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
    fn take_block(alignment: usize) -> ChunkBlock {
        CHUNK_BLOCK_POOL
            .with(|pool| pool.borrow_mut().acquire(alignment))
            .unwrap_or_else(|| ChunkBlock::fresh(alignment))
    }

    pub fn new(archetype: Archetype) -> Self {
        let layout = ChunkLayout::for_archetype(archetype);
        Self::new_with_layout(archetype, &layout)
    }

    pub(crate) fn new_with_layout(archetype: Archetype, layout: &ChunkLayout) -> Self {
        let block = Self::take_block(archetype.alignment.max(1));
        let (data, alloc_layout) = block.into_raw_parts();

        Self {
            entity_count: 0,
            max_entity_count: layout.max_entity_count,
            archetype,
            data,
            alloc_layout,
            column_offsets: layout.column_offsets.clone(),
            entities: Vec::with_capacity(layout.max_entity_count),
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

    #[inline(always)]
    pub fn column_ptr(&self, component_index: usize) -> *mut u8 {
        unsafe { self.data.as_ptr().add(self.column_offsets[component_index]) }
    }

    pub fn data_ptr(&self) -> *mut u8 {
        self.data.as_ptr()
    }

    #[inline(always)]
    unsafe fn component_ptr_unchecked(
        &self,
        component_index: usize,
        entity_index: usize,
    ) -> *mut u8 {
        let component = &self.archetype.components[component_index];
        self.column_ptr(component_index)
            .add(entity_index * component.size)
    }

    #[inline(always)]
    pub fn component_ptr(&self, component_index: usize, entity_index: usize) -> *mut u8 {
        if entity_index >= self.entity_count {
            return ptr::null_mut();
        }

        unsafe { self.component_ptr_unchecked(component_index, entity_index) }
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

    /// Bitwise-copy entity data from `src_index` to `dst_index` within
    /// this chunk.  The destination slot must be logically uninitialised
    /// (or already dropped) — this function does NOT drop the old data
    /// at `dst_index`.
    #[inline(always)]
    pub(crate) fn copy_entity_within(&mut self, src_index: usize, dst_index: usize) {
        if src_index == dst_index {
            return;
        }

        for (component_index, component) in self.archetype.components.iter().enumerate() {
            unsafe {
                ptr::copy_nonoverlapping(
                    self.component_ptr_unchecked(component_index, src_index),
                    self.component_ptr_unchecked(component_index, dst_index),
                    component.size,
                );
            }
        }

        self.entities[dst_index] = self.entities[src_index];
    }

    /// Bitwise-copy entity data from `src` chunk at `src_index` into
    /// `self` at `dst_index`.  The destination slot must be logically
    /// uninitialised (or already dropped).
    #[inline(always)]
    pub(crate) fn copy_entity_from(&mut self, src: &Chunk, src_index: usize, dst_index: usize) {
        debug_assert_eq!(self.archetype.id(), src.archetype.id());

        for (component_index, component) in self.archetype.components.iter().enumerate() {
            unsafe {
                ptr::copy_nonoverlapping(
                    src.component_ptr_unchecked(component_index, src_index),
                    self.component_ptr_unchecked(component_index, dst_index),
                    component.size,
                );
            }
        }

        self.entities[dst_index] = src.entities[src_index];
    }

    #[inline(always)]
    pub(crate) fn remove_last_entity(&mut self) -> Option<EntityId> {
        if self.entity_count == 0 {
            return None;
        }

        self.entity_count -= 1;
        self.entities.pop()
    }

    // -----------------------------------------------------------------------
    // Drop helpers
    // -----------------------------------------------------------------------

    /// Drop all components of a single entity at `entity_index`.
    ///
    /// # Safety
    ///
    /// `entity_index` must be a valid, occupied slot in this chunk.
    /// After this call the component data at that slot is logically
    /// uninitialised and must not be read or dropped again.
    pub(crate) unsafe fn drop_entity_components(&self, entity_index: usize) {
        for &component_index in &self.archetype.drop_component_indices {
            let component_index = component_index as usize;
            let component = &self.archetype.components[component_index];
            let drop_fn = component
                .drop_fn()
                .expect("drop component index must point to a droppable component");
            let ptr = self.component_ptr_unchecked(component_index, entity_index);
            drop_fn(ptr);
        }
    }

    /// Drop a single component column for one entity.
    ///
    /// # Safety
    ///
    /// `entity_index` must be a valid, occupied slot.  `component_index`
    /// must be a valid column.  After this call that component value is
    /// logically uninitialised.
    pub(crate) unsafe fn drop_single_component(&self, component_index: usize, entity_index: usize) {
        let component = &self.archetype.components[component_index];
        if let Some(drop_fn) = component.drop_fn() {
            let ptr = self.component_ptr_unchecked(component_index, entity_index);
            drop_fn(ptr);
        }
    }

    /// Drop components for every live entity in this chunk.
    ///
    /// # Safety
    ///
    /// Must only be called once.  After this call, all component data in
    /// the chunk is logically uninitialised.
    pub(crate) unsafe fn drop_all_entities(&self) {
        if self.archetype.drop_component_indices.is_empty() {
            return;
        }

        for entity_index in 0..self.entity_count {
            self.drop_entity_components(entity_index);
        }
    }
}

impl Drop for Chunk {
    fn drop(&mut self) {
        // Safety: we are the sole owner and this chunk is being destroyed.
        unsafe {
            self.drop_all_entities();
        }

        let block = ChunkBlock {
            data: self.data,
            alloc_layout: self.alloc_layout,
        };
        CHUNK_BLOCK_POOL.with(|pool| {
            pool.borrow_mut().release(block);
        });
    }
}

pub struct Data {
    pub archetype: Archetype,
    pub(crate) layout: ChunkLayout,
    pub chunks: Vec<Chunk>,
    /// Cached empty chunk to avoid pool round-trips on spawn/despawn churn.
    spare_chunk: Option<Chunk>,
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
            layout: ChunkLayout::for_archetype(archetype),
            chunks: Vec::new(),
            spare_chunk: None,
        }
    }

    fn add_chunk(&mut self) {
        let chunk = self
            .spare_chunk
            .take()
            .unwrap_or_else(|| Chunk::new_with_layout(self.archetype, &self.layout));
        self.chunks.push(chunk);
    }

    #[allow(dead_code)]
    #[inline(always)]
    fn dense_cmp(left: ChunkEntityLocation, right: ChunkEntityLocation) -> Ordering {
        left.chunk_index
            .cmp(&right.chunk_index)
            .then_with(|| left.entity_index.cmp(&right.entity_index))
    }

    #[allow(dead_code)]
    fn last_dense_location(&self) -> Option<ChunkEntityLocation> {
        let chunk_index = self.chunks.len().checked_sub(1)?;
        let entity_index = self.chunks[chunk_index].entity_count.checked_sub(1)?;
        Some(ChunkEntityLocation {
            chunk_index,
            entity_index,
        })
    }

    #[allow(dead_code)]
    fn prev_dense_location(&self, location: ChunkEntityLocation) -> Option<ChunkEntityLocation> {
        if location.entity_index > 0 {
            return Some(ChunkEntityLocation {
                chunk_index: location.chunk_index,
                entity_index: location.entity_index - 1,
            });
        }

        let mut chunk_index = location.chunk_index;
        while chunk_index > 0 {
            chunk_index -= 1;
            let entity_count = self.chunks[chunk_index].entity_count;
            if entity_count > 0 {
                return Some(ChunkEntityLocation {
                    chunk_index,
                    entity_index: entity_count - 1,
                });
            }
        }

        None
    }

    #[inline(always)]
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

    #[allow(dead_code)]
    pub(crate) fn add_entities_batch(&mut self, entities: &[EntityId]) -> Vec<ChunkEntityLocation> {
        let mut locations = Vec::with_capacity(entities.len());
        let mut next_entity = 0usize;

        while next_entity < entities.len() {
            if self.chunks.last().is_none_or(Chunk::is_full) {
                self.add_chunk();
            }

            let chunk_index = self.chunks.len() - 1;
            let chunk = &mut self.chunks[chunk_index];
            while next_entity < entities.len() && !chunk.is_full() {
                let entity_index = chunk
                    .add_entity(entities[next_entity])
                    .expect("chunk was checked for capacity");
                locations.push(ChunkEntityLocation {
                    chunk_index,
                    entity_index,
                });
                next_entity += 1;
            }
        }

        locations
    }

    #[inline(always)]
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
            let empty = self.chunks.pop().unwrap();
            // Cache one spare chunk; drop extras back to the pool.
            if self.spare_chunk.is_none() {
                self.spare_chunk = Some(empty);
            }
        }

        moved_entity
    }

    #[allow(dead_code)]
    pub(crate) fn remove_entities_batch_desc(
        &mut self,
        removals: &[ChunkEntityLocation],
    ) -> Vec<(EntityId, ChunkEntityLocation)> {
        if removals.is_empty() || self.chunks.is_empty() {
            return Vec::new();
        }

        debug_assert!(removals
            .windows(2)
            .all(|window| { Self::dense_cmp(window[0], window[1]) != Ordering::Less }));

        let mut moved = Vec::with_capacity(removals.len());
        let mut logical_last = self.last_dense_location();

        for &hole in removals {
            let Some(last) = logical_last else {
                break;
            };

            debug_assert!(Self::dense_cmp(hole, last) != Ordering::Greater);

            if hole.chunk_index != last.chunk_index || hole.entity_index != last.entity_index {
                let moved_entity = self.chunks[last.chunk_index]
                    .entity_id(last.entity_index)
                    .expect("logical tail must be occupied");

                // Drop the removed entity's components at `hole` before
                // overwriting with the swap-move source.
                // Safety: hole is a valid, occupied slot that is being removed.
                unsafe {
                    self.chunks[hole.chunk_index].drop_entity_components(hole.entity_index);
                }

                if hole.chunk_index == last.chunk_index {
                    self.chunks[hole.chunk_index]
                        .copy_entity_within(last.entity_index, hole.entity_index);
                } else {
                    let (head, tail) = self.chunks.split_at_mut(last.chunk_index);
                    let dst_chunk = &mut head[hole.chunk_index];
                    let src_chunk = &tail[0];
                    dst_chunk.copy_entity_from(src_chunk, last.entity_index, hole.entity_index);
                }

                moved.push((moved_entity, hole));
            } else {
                // The hole IS the logical last entity — just drop it,
                // no swap needed.
                unsafe {
                    self.chunks[hole.chunk_index].drop_entity_components(hole.entity_index);
                }
            }

            logical_last = self.prev_dense_location(last);
        }

        let mut remaining = removals.len();
        while remaining > 0 {
            let last_chunk = self
                .chunks
                .last_mut()
                .expect("batch removal count must not exceed live entities");
            if remaining >= last_chunk.entity_count {
                remaining -= last_chunk.entity_count;
                // Set entity_count to 0 before popping so Chunk::Drop
                // doesn't double-drop the entities we already dropped above.
                last_chunk.entity_count = 0;
                last_chunk.entities.clear();
                self.chunks.pop();
            } else {
                last_chunk.entity_count -= remaining;
                last_chunk.entities.truncate(last_chunk.entity_count);
                remaining = 0;
            }
        }

        moved
    }
}

#[cfg(test)]
mod tests {
    use super::{Chunk, BACKING_POOL_BUDGET_BYTES, CHUNK_BLOCK_POOL};
    use crate::ecs::{create_archetype, register_component_type};

    #[repr(align(16))]
    #[allow(dead_code)]
    struct Aligned16([u8; 16]);

    #[repr(align(8))]
    #[allow(dead_code)]
    struct Aligned8([u8; 8]);

    #[repr(align(32))]
    #[allow(dead_code)]
    struct Aligned32([u8; 32]);

    #[allow(dead_code)]
    #[derive(Clone, Copy)]
    struct Tiny(u32);

    #[allow(dead_code)]
    #[derive(Clone, Copy)]
    struct Large([u8; 1024]);

    #[repr(align(16))]
    #[allow(dead_code)]
    #[derive(Clone, Copy)]
    struct AlignedTiny(u32);

    #[repr(align(16))]
    #[allow(dead_code)]
    #[derive(Clone, Copy)]
    struct AlignedLarge([u8; 1024]);

    #[allow(dead_code)]
    #[derive(Clone, Copy)]
    struct Huge([u8; 16 * 1024]);

    struct PoolGuard;

    impl PoolGuard {
        fn new() -> Self {
            CHUNK_BLOCK_POOL.with(|pool| pool.borrow_mut().clear());
            Self
        }
    }

    impl Drop for PoolGuard {
        fn drop(&mut self) {
            CHUNK_BLOCK_POOL.with(|pool| pool.borrow_mut().clear());
        }
    }

    fn pool_stats() -> (usize, usize) {
        CHUNK_BLOCK_POOL.with(|pool| {
            let pool = pool.borrow();
            (pool.entries.len(), pool.retained_bytes)
        })
    }

    fn pooled_ptrs() -> Vec<usize> {
        CHUNK_BLOCK_POOL.with(|pool| {
            pool.borrow()
                .entries
                .iter()
                .map(|entry| entry.block.data.as_ptr() as usize)
                .collect()
        })
    }

    fn test_entity(index: usize) -> crate::ecs::EntityId {
        crate::ecs::EntityId::new(index as u32, 0)
    }

    #[test]
    fn chunk_columns_respect_component_alignment() {
        let _guard = PoolGuard::new();

        let ty_a = register_component_type(
            "chunk_test_aligned16",
            core::mem::size_of::<Aligned16>(),
            core::mem::align_of::<Aligned16>(),
        );
        let ty_b = register_component_type(
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

    #[test]
    fn dropped_block_is_reused_on_same_thread() {
        let _guard = PoolGuard::new();

        let archetype = create_archetype().add_rust_component::<Tiny>().build();
        let first_ptr = {
            let chunk = Chunk::new(archetype);
            chunk.data_ptr() as usize
        };

        let (entry_count, _) = pool_stats();
        assert_eq!(entry_count, 1);

        let second_chunk = Chunk::new(archetype);
        assert_eq!(second_chunk.data_ptr() as usize, first_ptr);
    }

    #[test]
    fn add_entities_batch_spans_chunks_and_preserves_order() {
        let _guard = PoolGuard::new();

        let archetype = create_archetype().add_rust_component::<Huge>().build();
        let mut data = super::Data::new(archetype);
        assert_eq!(data.layout.max_entity_count, 32);

        let entities: Vec<_> = (0..40).map(test_entity).collect();
        let locations = data.add_entities_batch(&entities);

        assert_eq!(locations.len(), entities.len());
        assert_eq!(locations[0].chunk_index, 0);
        assert_eq!(locations[31].chunk_index, 0);
        assert_eq!(locations[31].entity_index, 31);
        assert_eq!(locations[32].chunk_index, 1);
        assert_eq!(locations[32].entity_index, 0);
        assert_eq!(data.chunks[0].entity_count, 32);
        assert_eq!(data.chunks[1].entity_count, 8);
        assert_eq!(data.chunks[0].entity_id(0), Some(entities[0]));
        assert_eq!(data.chunks[1].entity_id(7), Some(entities[39]));
    }

    #[test]
    fn remove_entities_batch_desc_compacts_across_chunks() {
        let _guard = PoolGuard::new();

        let archetype = create_archetype().add_rust_component::<Huge>().build();
        let mut data = super::Data::new(archetype);
        let entities: Vec<_> = (0..40).map(test_entity).collect();
        let locations = data.add_entities_batch(&entities);

        let moved = data.remove_entities_batch_desc(&[locations[38], locations[5]]);

        assert_eq!(data.chunks.len(), 2);
        assert_eq!(data.chunks[0].entity_count, 32);
        assert_eq!(data.chunks[1].entity_count, 6);
        assert_eq!(data.chunks[0].entity_id(5), Some(entities[39]));
        assert!(moved.iter().any(|(entity, location)| {
            *entity == entities[39] && location.chunk_index == 0 && location.entity_index == 5
        }));
    }

    #[test]
    fn higher_alignment_block_satisfies_lower_alignment_request() {
        let _guard = PoolGuard::new();

        let high = create_archetype().add_rust_component::<Aligned32>().build();
        let low = create_archetype().add_rust_component::<Aligned8>().build();
        let high_ptr = {
            let chunk = Chunk::new(high);
            chunk.data_ptr() as usize
        };

        let low_chunk = Chunk::new(low);
        assert_eq!(low_chunk.data_ptr() as usize, high_ptr);
    }

    #[test]
    fn different_archetypes_can_reuse_same_raw_block() {
        let _guard = PoolGuard::new();

        let large = create_archetype()
            .add_rust_component::<AlignedLarge>()
            .build();
        let small = create_archetype()
            .add_rust_component::<AlignedTiny>()
            .build();
        let large_ptr = {
            let chunk = Chunk::new(large);
            chunk.data_ptr() as usize
        };

        let small_chunk = Chunk::new(small);
        assert_eq!(small_chunk.data_ptr() as usize, large_ptr);
    }

    #[test]
    fn retained_bytes_stay_within_budget() {
        let _guard = PoolGuard::new();

        let archetype = create_archetype().add_rust_component::<Large>().build();
        let mut chunks = Vec::new();
        for _ in 0..8 {
            chunks.push(Chunk::new(archetype));
        }
        drop(chunks);

        let (_, retained_bytes) = pool_stats();
        assert!(retained_bytes <= BACKING_POOL_BUDGET_BYTES);
    }

    #[test]
    fn lru_eviction_removes_oldest_block_when_budget_overflows() {
        let _guard = PoolGuard::new();

        let archetype = create_archetype().add_rust_component::<Large>().build();
        let chunks: Vec<_> = (0..9).map(|_| Chunk::new(archetype)).collect();
        let ptrs: Vec<_> = chunks
            .iter()
            .map(|chunk| chunk.data_ptr() as usize)
            .collect();
        for chunk in chunks {
            drop(chunk);
        }

        let pooled = pooled_ptrs();
        assert!(!pooled.contains(&ptrs[0]));
        assert_eq!(pooled.len(), 8);
    }

    #[test]
    fn pool_reset_helper_clears_thread_local_state() {
        let _guard = PoolGuard::new();

        let archetype = create_archetype().add_rust_component::<Tiny>().build();
        drop(Chunk::new(archetype));
        let (entry_count, _) = pool_stats();
        assert_eq!(entry_count, 1);

        CHUNK_BLOCK_POOL.with(|pool| pool.borrow_mut().clear());
        let (entry_count, retained_bytes) = pool_stats();
        assert_eq!(entry_count, 0);
        assert_eq!(retained_bytes, 0);
    }
}
