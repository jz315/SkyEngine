use super::{resolve_column_ptr, EntityId, PreparedCache, World};
use rayon::prelude::*;
use std::slice;

const TARGET_STRIPE_ENTITIES: usize = 4_096;
const MIN_PARALLEL_STRIPES_PER_THREAD: usize = 3;

#[derive(Clone, Copy)]
struct RawColumnPtr(*mut u8);

unsafe impl Send for RawColumnPtr {}
unsafe impl Sync for RawColumnPtr {}

#[derive(Clone, Copy)]
struct RawChunkDataPtr(*mut u8);

unsafe impl Send for RawChunkDataPtr {}
unsafe impl Sync for RawChunkDataPtr {}

#[derive(Clone, Copy)]
struct RawEntityPtr(*const EntityId);

unsafe impl Send for RawEntityPtr {}
unsafe impl Sync for RawEntityPtr {}

#[derive(Clone, Copy)]
pub struct ParallelChunkJob {
    start: usize,
    len: usize,
    entities: RawEntityPtr,
    component_ptrs: [RawColumnPtr; super::INLINE_QUERY_COMPONENTS],
}

impl ParallelChunkJob {
    #[inline(always)]
    fn component_ptr(&self, index: usize) -> *mut u8 {
        self.component_ptrs[index].0
    }

    #[inline(always)]
    unsafe fn entities<'w>(&self) -> &'w [EntityId] {
        slice::from_raw_parts(self.entities.0.add(self.start), self.len)
    }
}

#[derive(Clone, Copy)]
struct ParallelChunkSignature {
    data_index: usize,
    entity_count: usize,
    chunk_data: RawChunkDataPtr,
    entities: RawEntityPtr,
}

#[derive(Default)]
pub(crate) struct ParallelJobCache {
    cached_epoch: Option<usize>,
    jobs: Vec<ParallelChunkJob>,
    chunk_signatures: Vec<ParallelChunkSignature>,
}

pub trait ParallelQueryParam {
    type Slice<'w>;

    unsafe fn slice_from_raw<'w>(ptr: *mut u8, start: usize, len: usize) -> Self::Slice<'w>;
}

impl<T: Sync + 'static> ParallelQueryParam for &T {
    type Slice<'w> = &'w [T];

    #[inline(always)]
    unsafe fn slice_from_raw<'w>(ptr: *mut u8, start: usize, len: usize) -> Self::Slice<'w> {
        slice::from_raw_parts((ptr as *const T).add(start), len)
    }
}

impl<T: Send + 'static> ParallelQueryParam for &mut T {
    type Slice<'w> = &'w mut [T];

    #[inline(always)]
    unsafe fn slice_from_raw<'w>(ptr: *mut u8, start: usize, len: usize) -> Self::Slice<'w> {
        slice::from_raw_parts_mut((ptr as *mut T).add(start), len)
    }
}

impl<T: Sync + 'static> ParallelQueryParam for Option<&T> {
    type Slice<'w> = Option<&'w [T]>;

    #[inline(always)]
    unsafe fn slice_from_raw<'w>(ptr: *mut u8, start: usize, len: usize) -> Self::Slice<'w> {
        if ptr.is_null() {
            None
        } else {
            Some(slice::from_raw_parts((ptr as *const T).add(start), len))
        }
    }
}

impl<T: Send + 'static> ParallelQueryParam for Option<&mut T> {
    type Slice<'w> = Option<&'w mut [T]>;

    #[inline(always)]
    unsafe fn slice_from_raw<'w>(ptr: *mut u8, start: usize, len: usize) -> Self::Slice<'w> {
        if ptr.is_null() {
            None
        } else {
            Some(slice::from_raw_parts_mut((ptr as *mut T).add(start), len))
        }
    }
}

pub trait ParallelQuerySpec {
    type Chunk<'w>;

    unsafe fn chunk_from_raw<'w>(
        chunk: &'w super::Chunk,
        component_indices: &[u8],
    ) -> Self::Chunk<'w>;
    unsafe fn chunk_from_job<'w>(job: &'w ParallelChunkJob) -> Self::Chunk<'w>;
}

impl<P: ParallelQueryParam> ParallelQuerySpec for P {
    type Chunk<'w> = P::Slice<'w>;

    #[inline(always)]
    unsafe fn chunk_from_raw<'w>(
        chunk: &'w super::Chunk,
        component_indices: &[u8],
    ) -> Self::Chunk<'w> {
        P::slice_from_raw(
            resolve_column_ptr(chunk, component_indices[0]),
            0,
            chunk.entity_count,
        )
    }

    #[inline(always)]
    unsafe fn chunk_from_job<'w>(job: &'w ParallelChunkJob) -> Self::Chunk<'w> {
        P::slice_from_raw(job.component_ptr(0), job.start, job.len)
    }
}

macro_rules! impl_parallel_query_spec_tuple {
    ($(($Param:ident, $index:tt)),+ $(,)?) => {
        impl<$($Param: ParallelQueryParam),+> ParallelQuerySpec for ($($Param,)+) {
            type Chunk<'w> = ($($Param::Slice<'w>,)+);

            #[inline(always)]
            unsafe fn chunk_from_raw<'w>(
                chunk: &'w super::Chunk,
                component_indices: &[u8],
            ) -> Self::Chunk<'w> {
                (
                    $(
                        $Param::slice_from_raw(
                            resolve_column_ptr(chunk, component_indices[$index]),
                            0,
                            chunk.entity_count,
                        ),
                    )+
                )
            }

            #[inline(always)]
            unsafe fn chunk_from_job<'w>(job: &'w ParallelChunkJob) -> Self::Chunk<'w> {
                (
                    $(
                        $Param::slice_from_raw(job.component_ptr($index), job.start, job.len),
                    )+
                )
            }
        }
    };
}

impl_parallel_query_spec_tuple!((A, 0), (B, 1));
impl_parallel_query_spec_tuple!((A, 0), (B, 1), (C, 2));
impl_parallel_query_spec_tuple!((A, 0), (B, 1), (C, 2), (D, 3));
impl_parallel_query_spec_tuple!((A, 0), (B, 1), (C, 2), (D, 3), (E, 4));
impl_parallel_query_spec_tuple!((A, 0), (B, 1), (C, 2), (D, 3), (E, 4), (F, 5));
impl_parallel_query_spec_tuple!((A, 0), (B, 1), (C, 2), (D, 3), (E, 4), (F, 5), (G, 6));
impl_parallel_query_spec_tuple!(
    (A, 0),
    (B, 1),
    (C, 2),
    (D, 3),
    (E, 4),
    (F, 5),
    (G, 6),
    (H, 7)
);

fn collect_chunk_jobs(
    cache: &mut ParallelJobCache,
    prepared: &PreparedCache,
    world: &World,
) -> usize {
    cache.cached_epoch = Some(world.archetype_epoch());
    cache.jobs.clear();
    cache.chunk_signatures.clear();
    let mut total_entities = 0usize;
    prepared.visit_chunks(world, |cached, chunk| {
        if chunk.entity_count == 0 {
            return;
        }
        total_entities += chunk.entity_count;

        cache.chunk_signatures.push(ParallelChunkSignature {
            data_index: cached.data_index,
            entity_count: chunk.entity_count,
            chunk_data: RawChunkDataPtr(chunk.data_ptr()),
            entities: RawEntityPtr(chunk.entities().as_ptr()),
        });

        let mut component_ptrs =
            [RawColumnPtr(std::ptr::null_mut()); super::INLINE_QUERY_COMPONENTS];
        for (slot, &index) in cached.component_indices.iter().enumerate() {
            component_ptrs[slot] = RawColumnPtr(resolve_column_ptr(chunk, index));
        }

        let entities = chunk.entities();
        let mut start = 0usize;
        while start < chunk.entity_count {
            let len = (chunk.entity_count - start).min(TARGET_STRIPE_ENTITIES);
            cache.jobs.push(ParallelChunkJob {
                start,
                len,
                entities: RawEntityPtr(entities.as_ptr()),
                component_ptrs,
            });
            start += len;
        }
    });
    total_entities
}

fn cached_total_entities(
    cache: &ParallelJobCache,
    prepared: &PreparedCache,
    world: &World,
) -> Option<usize> {
    if cache.cached_epoch != Some(world.archetype_epoch()) {
        return None;
    }

    if cache.chunk_signatures.is_empty() && !cache.jobs.is_empty() {
        return None;
    }

    let mut signature_index = 0usize;
    let mut total_entities = 0usize;
    let mut valid = true;

    prepared.visit_chunks(world, |cached, chunk| {
        if !valid {
            return;
        }

        let Some(signature) = cache.chunk_signatures.get(signature_index) else {
            valid = false;
            return;
        };

        if signature.data_index != cached.data_index
            || signature.entity_count != chunk.entity_count
            || signature.chunk_data.0 != chunk.data_ptr()
            || signature.entities.0 != chunk.entities().as_ptr()
        {
            valid = false;
            return;
        }

        total_entities += chunk.entity_count;
        signature_index += 1;
    });

    (valid && signature_index == cache.chunk_signatures.len()).then_some(total_entities)
}

#[inline(always)]
fn ensure_chunk_jobs(
    cache: &mut ParallelJobCache,
    prepared: &PreparedCache,
    world: &World,
) -> usize {
    cached_total_entities(cache, prepared, world)
        .unwrap_or_else(|| collect_chunk_jobs(cache, prepared, world))
}

fn should_parallel(total_entities: usize, job_count: usize) -> bool {
    let thread_count = rayon::current_num_threads();
    if job_count <= 1 || job_count < thread_count * 2 {
        return false;
    }

    total_entities >= thread_count * TARGET_STRIPE_ENTITIES * MIN_PARALLEL_STRIPES_PER_THREAD
}

fn seq_for_each_chunk<Q, F>(prepared: &PreparedCache, world: &World, f: &F)
where
    Q: ParallelQuerySpec,
    F: for<'w> Fn(<Q as ParallelQuerySpec>::Chunk<'w>),
{
    prepared.visit_chunks(world, |cached, chunk| unsafe {
        f(Q::chunk_from_raw(chunk, &cached.component_indices));
    });
}

fn seq_for_each_chunk_with_entities<Q, F>(prepared: &PreparedCache, world: &World, f: &F)
where
    Q: ParallelQuerySpec,
    F: for<'w> Fn(&'w [EntityId], <Q as ParallelQuerySpec>::Chunk<'w>),
{
    prepared.visit_chunks(world, |cached, chunk| unsafe {
        f(
            chunk.entities(),
            Q::chunk_from_raw(chunk, &cached.component_indices),
        );
    });
}
pub(crate) fn par_for_each_chunk<Q, F>(
    cache: &mut ParallelJobCache,
    prepared: &PreparedCache,
    world: &World,
    f: F,
) where
    Q: ParallelQuerySpec,
    F: for<'w> Fn(<Q as ParallelQuerySpec>::Chunk<'w>) + Send + Sync,
{
    let total_entities = ensure_chunk_jobs(cache, prepared, world);
    if should_parallel(total_entities, cache.jobs.len()) {
        cache.jobs.par_iter().for_each(|job| unsafe {
            f(Q::chunk_from_job(job));
        });
    } else {
        seq_for_each_chunk::<Q, _>(prepared, world, &f);
    }
}

pub(crate) fn par_for_each_chunk_with_entities<Q, F>(
    cache: &mut ParallelJobCache,
    prepared: &PreparedCache,
    world: &World,
    f: F,
) where
    Q: ParallelQuerySpec,
    F: for<'w> Fn(&'w [EntityId], <Q as ParallelQuerySpec>::Chunk<'w>) + Send + Sync,
{
    let total_entities = ensure_chunk_jobs(cache, prepared, world);
    if should_parallel(total_entities, cache.jobs.len()) {
        cache.jobs.par_iter().for_each(|job| unsafe {
            f(job.entities(), Q::chunk_from_job(job));
        });
    } else {
        seq_for_each_chunk_with_entities::<Q, _>(prepared, world, &f);
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::{create_archetype, World};
    use super::super::PreparedQuery;
    use std::collections::HashSet;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Mutex, MutexGuard};

    #[derive(Clone, Copy, Default)]
    struct Position {
        x: f32,
        y: f32,
    }

    #[derive(Clone, Copy, Default)]
    struct Velocity {
        x: f32,
        y: f32,
    }

    #[derive(Clone, Copy, Default)]
    struct Extra {
        _value: f32,
    }

    #[derive(Clone, Copy)]
    struct LargePad {
        _bytes: [u8; 16 * 1024],
    }

    impl Default for LargePad {
        fn default() -> Self {
            Self {
                _bytes: [0; 16 * 1024],
            }
        }
    }

    fn spawn(world: &mut World, archetype: super::super::super::Archetype, count: usize) {
        for _ in 0..count {
            unsafe {
                world.add_entity(archetype);
            }
        }
    }

    fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
        mutex.lock().expect("test mutex poisoned")
    }

    #[test]
    fn par_for_each_chunk_spans_multiple_chunks_for_same_archetype() {
        let entity_count = 96usize;
        let mut world = World::new();
        for _ in 0..entity_count {
            world.spawn((
                Position::default(),
                Velocity { x: 1.0, y: 2.0 },
                LargePad::default(),
            ));
        }

        let chunk_visits = AtomicUsize::new(0);
        let mut query = world.query::<(&mut Position, &Velocity)>();
        query.par_for_each_chunk(&mut world, |(positions, velocities)| {
            chunk_visits.fetch_add(1, Ordering::Relaxed);
            for index in 0..positions.len() {
                positions[index].x += velocities[index].x;
                positions[index].y += velocities[index].y;
            }
        });

        assert!(
            chunk_visits.load(Ordering::Relaxed) > 1,
            "expected more than one chunk visit for large archetype"
        );

        let mut matched = 0usize;
        let mut check = world.query::<&Position>();
        check.for_each(&mut world, |position| {
            assert_eq!(position.x, 1.0);
            assert_eq!(position.y, 2.0);
            matched += 1;
        });
        assert_eq!(matched, entity_count);
    }

    #[test]
    fn par_for_each_chunk_matches_sequential_updates() {
        let archetype = create_archetype()
            .add_rust_component::<Position>()
            .add_rust_component::<Velocity>()
            .build();
        let mut world = World::new();
        spawn(&mut world, archetype, 128);

        let mut init = world.query::<&mut Velocity>();
        init.for_each(&mut world, |velocity| {
            velocity.x = 1.5;
            velocity.y = 0.5;
        });

        let mut query = world.query::<(&mut Position, &Velocity)>();
        query.par_for_each_chunk(&mut world, |(positions, velocities)| {
            for index in 0..positions.len() {
                positions[index].x += velocities[index].x * 2.0;
                positions[index].y += velocities[index].y * 4.0;
            }
        });

        let mut check = world.query::<&Position>();
        check.for_each(&mut world, |position| {
            assert_eq!(position.x, 3.0);
            assert_eq!(position.y, 2.0);
        });
    }

    #[test]
    fn par_for_each_chunk_runs_across_multiple_matching_archetypes() {
        let base = create_archetype()
            .add_rust_component::<Position>()
            .add_rust_component::<Velocity>()
            .build();
        let extended = create_archetype()
            .add_rust_component::<Position>()
            .add_rust_component::<Velocity>()
            .add_rust_component::<Extra>()
            .build();

        let mut world = World::new();
        spawn(&mut world, base, 96);
        spawn(&mut world, extended, 96);

        let mut init = world.query::<&mut Velocity>();
        init.for_each(&mut world, |velocity| {
            velocity.x = 2.0;
            velocity.y = 1.0;
        });

        let mut query = world.query::<(&mut Position, &Velocity)>();
        query.par_for_each_chunk(&mut world, |(positions, velocities)| {
            for index in 0..positions.len() {
                positions[index].x += velocities[index].x;
                positions[index].y += velocities[index].y;
            }
        });

        let mut count = 0usize;
        let mut check = world.query::<&Position>();
        check.for_each(&mut world, |position| {
            assert_eq!(position.x, 2.0);
            assert_eq!(position.y, 1.0);
            count += 1;
        });

        assert_eq!(count, 192);
    }

    #[test]
    fn par_optional_query_handles_present_and_absent_components() {
        let mut world = World::new();
        world.spawn((Position { x: 0.0, y: 0.0 }, Velocity { x: 2.0, y: 3.0 }));
        world.spawn((Position { x: 5.0, y: 0.0 },));

        let with_velocity = AtomicUsize::new(0);
        let without_velocity = AtomicUsize::new(0);
        let mut query = world.query::<(&mut Position, Option<&Velocity>)>();
        query.par_for_each_chunk(&mut world, |(positions, velocities)| {
            for index in 0..positions.len() {
                if let Some(velocities) = velocities {
                    positions[index].x += velocities[index].x;
                    positions[index].y += velocities[index].y;
                    with_velocity.fetch_add(1, Ordering::Relaxed);
                } else {
                    positions[index].x = -1.0;
                    without_velocity.fetch_add(1, Ordering::Relaxed);
                }
            }
        });

        assert_eq!(with_velocity.load(Ordering::Relaxed), 1);
        assert_eq!(without_velocity.load(Ordering::Relaxed), 1);

        let mut results = Vec::new();
        let mut check = world.query::<&Position>();
        check.for_each(&mut world, |position| {
            results.push((position.x, position.y))
        });
        results.sort_by(|lhs, rhs| lhs.0.partial_cmp(&rhs.0).unwrap());

        assert_eq!(results, vec![(-1.0, 0.0), (2.0, 3.0)]);
    }

    #[test]
    fn par_for_each_chunk_with_entities_provides_matching_entity_slices() {
        let archetype = create_archetype()
            .add_rust_component::<Position>()
            .add_rust_component::<Velocity>()
            .build();
        let mut world = World::new();
        let ids: Vec<_> = (0..64)
            .map(|_| unsafe { world.add_entity(archetype) })
            .collect();

        let seen = Mutex::new(Vec::new());
        let chunk_lengths = AtomicUsize::new(0);
        let mut query = world.query::<(&Position, &Velocity)>();
        query.par_for_each_chunk_with_entities(&mut world, |entities, (positions, velocities)| {
            assert_eq!(entities.len(), positions.len());
            assert_eq!(entities.len(), velocities.len());
            chunk_lengths.fetch_add(entities.len(), Ordering::Relaxed);
            lock(&seen).extend_from_slice(entities);
        });

        assert_eq!(chunk_lengths.load(Ordering::Relaxed), ids.len());
        let seen = lock(&seen);
        let actual: HashSet<_> = seen.iter().copied().collect();
        let expected: HashSet<_> = ids.iter().copied().collect();
        assert_eq!(actual, expected);
    }

    #[test]
    fn par_for_each_chunk_with_entities_preserves_entity_alignment_across_multiple_chunks() {
        let entity_count = 96usize;
        let mut world = World::new();
        let ids: Vec<_> = (0..entity_count)
            .map(|_| {
                world.spawn((
                    Position::default(),
                    Velocity { x: 0.0, y: 0.0 },
                    LargePad::default(),
                ))
            })
            .collect();

        let mut init = world.query::<&mut Position>();
        let mut next = 0u32;
        init.for_each(&mut world, |position| {
            position.x = next as f32;
            next += 1;
        });

        let chunk_visits = AtomicUsize::new(0);
        let mut query = world.query::<(&Position, &Velocity)>();
        query.par_for_each_chunk_with_entities(&mut world, |entities, (positions, velocities)| {
            chunk_visits.fetch_add(1, Ordering::Relaxed);
            assert_eq!(entities.len(), positions.len());
            assert_eq!(entities.len(), velocities.len());
            for index in 0..entities.len() {
                assert_eq!(positions[index].x as u32, entities[index].index());
            }
        });

        assert!(
            chunk_visits.load(Ordering::Relaxed) > 1,
            "expected more than one chunk visit for large archetype"
        );

        let seen = Mutex::new(Vec::new());
        let mut verify = world.query::<&Position>();
        verify.for_each_with_entity(&mut world, |entity, position| {
            assert_eq!(position.x as u32, entity.index());
            lock(&seen).push(entity);
        });
        let seen = lock(&seen);
        let actual: HashSet<_> = seen.iter().copied().collect();
        let expected: HashSet<_> = ids.iter().copied().collect();
        assert_eq!(actual, expected);
    }

    #[test]
    fn par_optional_mut_query_handles_multiple_chunks_and_archetypes() {
        let with_velocity_count = 48usize;
        let without_velocity_count = 48usize;
        let mut world = World::new();

        for index in 0..with_velocity_count {
            world.spawn((
                Position {
                    x: index as f32,
                    y: 0.0,
                },
                Velocity { x: 1.0, y: 2.0 },
                LargePad::default(),
            ));
        }
        for index in 0..without_velocity_count {
            world.spawn((
                Position {
                    x: 1000.0 + index as f32,
                    y: 0.0,
                },
                LargePad::default(),
            ));
        }

        let some_chunks = AtomicUsize::new(0);
        let none_chunks = AtomicUsize::new(0);
        let some_entities = AtomicUsize::new(0);
        let none_entities = AtomicUsize::new(0);

        let mut query = world.query::<(&mut Position, Option<&mut Velocity>)>();
        query.par_for_each_chunk(&mut world, |(positions, velocities)| {
            if let Some(velocities) = velocities {
                some_chunks.fetch_add(1, Ordering::Relaxed);
                for index in 0..positions.len() {
                    positions[index].x += velocities[index].x;
                    velocities[index].y += 10.0;
                    some_entities.fetch_add(1, Ordering::Relaxed);
                }
            } else {
                none_chunks.fetch_add(1, Ordering::Relaxed);
                for position in positions.iter_mut() {
                    position.y = -1.0;
                    none_entities.fetch_add(1, Ordering::Relaxed);
                }
            }
        });

        assert!(some_chunks.load(Ordering::Relaxed) > 1);
        assert!(none_chunks.load(Ordering::Relaxed) > 1);
        assert_eq!(some_entities.load(Ordering::Relaxed), with_velocity_count);
        assert_eq!(
            none_entities.load(Ordering::Relaxed),
            without_velocity_count
        );

        let mut with_velocity_seen = 0usize;
        let mut without_velocity_seen = 0usize;
        let mut check = world.query::<(&Position, Option<&Velocity>)>();
        check.for_each(&mut world, |(position, velocity)| {
            if let Some(velocity) = velocity {
                assert_eq!(velocity.y, 12.0);
                assert!(position.x >= 1.0 && position.x <= with_velocity_count as f32);
                with_velocity_seen += 1;
            } else {
                assert_eq!(position.y, -1.0);
                without_velocity_seen += 1;
            }
        });

        assert_eq!(with_velocity_seen, with_velocity_count);
        assert_eq!(without_velocity_seen, without_velocity_count);
    }

    #[test]
    fn par_for_each_chunk_does_not_invoke_closure_when_nothing_matches() {
        let mut world = World::new();
        world.spawn((Position { x: 1.0, y: 2.0 }, LargePad::default()));
        world.spawn((Position { x: 3.0, y: 4.0 }, LargePad::default()));

        let invocations = AtomicUsize::new(0);
        let mut query = world.query::<(&Position, &Velocity)>();
        query.par_for_each_chunk(&mut world, |_| {
            invocations.fetch_add(1, Ordering::Relaxed);
        });

        assert_eq!(invocations.load(Ordering::Relaxed), 0);
        assert_eq!(query.cached_archetype_count(), 0);
    }

    #[test]
    fn par_prepared_query_refreshes_when_new_matching_archetype_appears() {
        let mut world = World::new();
        world.spawn((
            Position::default(),
            Velocity { x: 1.0, y: 0.0 },
            LargePad::default(),
        ));

        let mut prepared = PreparedQuery::<(&mut Position, &Velocity)>::new();
        prepared.par_for_each_chunk(&mut world, |(positions, velocities)| {
            for index in 0..positions.len() {
                positions[index].x += velocities[index].x;
            }
        });
        assert_eq!(prepared.cached_archetype_count(), 1);

        world.spawn((
            Position::default(),
            Velocity { x: 2.0, y: 0.0 },
            Extra::default(),
            LargePad::default(),
        ));

        prepared.par_for_each_chunk(&mut world, |(positions, velocities)| {
            for index in 0..positions.len() {
                positions[index].x += velocities[index].x;
            }
        });
        assert_eq!(prepared.cached_archetype_count(), 2);

        let mut total = 0.0f32;
        let mut check = world.query::<&Position>();
        check.for_each(&mut world, |position| {
            total += position.x;
        });
        assert_eq!(total, 4.0);
    }

    #[test]
    fn par_prepared_query_rebuilds_when_matching_entity_count_changes() {
        let mut world = World::new();
        world.spawn((Position::default(), Velocity { x: 1.0, y: 0.0 }));
        world.spawn((Position::default(), Velocity { x: 1.0, y: 0.0 }));

        let mut prepared = PreparedQuery::<(&mut Position, &Velocity)>::new();
        prepared.par_for_each_chunk(&mut world, |(positions, velocities)| {
            for index in 0..positions.len() {
                positions[index].x += velocities[index].x;
            }
        });

        world.spawn((Position::default(), Velocity { x: 1.0, y: 0.0 }));

        prepared.par_for_each_chunk(&mut world, |(positions, velocities)| {
            for index in 0..positions.len() {
                positions[index].x += velocities[index].x;
            }
        });

        let mut total = 0.0f32;
        let mut count = 0usize;
        let mut check = world.query::<&Position>();
        check.for_each(&mut world, |position| {
            total += position.x;
            count += 1;
        });

        assert_eq!(count, 3);
        assert_eq!(total, 5.0);
        assert_eq!(prepared.cached_archetype_count(), 1);
    }

    #[test]
    fn par_prepared_query_rebuilds_jobs_after_world_clear() {
        let mut world = World::new();
        for _ in 0..64 {
            world.spawn((Position::default(), Velocity { x: 1.0, y: 0.0 }));
        }

        let mut prepared = PreparedQuery::<(&mut Position, &Velocity)>::new();
        prepared.par_for_each_chunk(&mut world, |(positions, velocities)| {
            for index in 0..positions.len() {
                positions[index].x += velocities[index].x;
            }
        });

        world.clear();
        for _ in 0..64 {
            world.spawn((
                Position::default(),
                Velocity { x: 2.0, y: 3.0 },
                Extra::default(),
            ));
        }

        prepared.par_for_each_chunk(&mut world, |(positions, velocities)| {
            for index in 0..positions.len() {
                positions[index].x += velocities[index].x;
                positions[index].y += velocities[index].y;
            }
        });

        let mut count = 0usize;
        let mut check = world.query::<&Position>();
        check.for_each(&mut world, |position| {
            assert_eq!(position.x, 2.0);
            assert_eq!(position.y, 3.0);
            count += 1;
        });
        assert_eq!(count, 64);
    }

    #[test]
    fn par_for_each_chunk_with_entities_sees_replaced_entities_without_rebuild() {
        let mut world = World::new();
        let removed = world.spawn((Position::default(), Velocity { x: 1.0, y: 0.0 }));
        world.spawn((Position::default(), Velocity { x: 1.0, y: 0.0 }));
        world.spawn((Position::default(), Velocity { x: 1.0, y: 0.0 }));

        let mut prepared = PreparedQuery::<(&Position, &Velocity)>::new();
        prepared.par_for_each_chunk_with_entities(&mut world, |_entities, _chunk| {});

        assert!(world.despawn(removed));
        let replacement = world.spawn((Position::default(), Velocity { x: 1.0, y: 0.0 }));

        let seen = Mutex::new(Vec::new());
        prepared.par_for_each_chunk_with_entities(&mut world, |entities, _chunk| {
            lock(&seen).extend_from_slice(entities);
        });

        let seen = lock(&seen);
        let actual: HashSet<_> = seen.iter().copied().collect();
        assert_eq!(actual.len(), 3);
        assert!(!actual.contains(&removed));
        assert!(actual.contains(&replacement));
    }
}
