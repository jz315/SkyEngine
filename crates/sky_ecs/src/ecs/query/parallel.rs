use super::{resolve_column_ptr, CachedArchetype, EntityId, QuerySpec, World};
use rayon::prelude::*;
use std::slice;
use std::sync::Arc;

const TARGET_STRIPE_ENTITIES: usize = 4_096;
const MIN_PARALLEL_STRIPES_PER_THREAD: usize = 3;

#[derive(Clone, Copy)]
struct RawEntityPtr(*const EntityId);

// Safety: pointers are only dereferenced while the originating World remains
// borrowed by a joined Rayon operation, and each job covers an in-bounds range.
unsafe impl Send for RawEntityPtr {}
unsafe impl Sync for RawEntityPtr {}

#[derive(Clone, Copy)]
struct ParallelChunkJob {
    start: usize,
    len: usize,
    entities: RawEntityPtr,
    component_ptrs: [*mut u8; super::MAX_QUERY_COMPONENTS],
}

// Safety: component pointers are created from live chunks and mutable columns
// are partitioned into non-overlapping stripe ranges before jobs are shared.
unsafe impl Send for ParallelChunkJob {}
unsafe impl Sync for ParallelChunkJob {}

impl ParallelChunkJob {
    #[inline(always)]
    unsafe fn entities<'w>(&self) -> &'w [EntityId] {
        slice::from_raw_parts(self.entities.0.add(self.start), self.len)
    }
}

#[derive(Default)]
pub(crate) struct ParallelJobCache {
    cached_world: Option<Arc<()>>,
    cached_storage_epoch: Option<u64>,
    jobs: Arc<Vec<ParallelChunkJob>>,
    total_entities: usize,
    #[cfg(test)]
    rebuild_count: usize,
}

pub(crate) struct ParallelJobSnapshot {
    jobs: Arc<Vec<ParallelChunkJob>>,
    total_entities: usize,
}

fn visit_chunks<'w, F>(archetypes: &[CachedArchetype], world: &'w World, mut f: F)
where
    F: FnMut(&CachedArchetype, &'w super::Chunk),
{
    for cached in archetypes {
        let data = &world.data[cached.data_index];
        for chunk in &data.chunks {
            debug_assert!(chunk.entity_count != 0);
            f(cached, chunk);
        }
    }
}

fn collect_chunk_jobs(
    cache: &mut ParallelJobCache,
    prepared: &[CachedArchetype],
    world: &World,
) -> usize {
    #[cfg(test)]
    {
        cache.rebuild_count += 1;
    }
    cache.cached_world = Some(Arc::clone(world.cache_token()));
    cache.cached_storage_epoch = Some(world.storage_epoch());
    let mut jobs = Vec::new();
    let mut total_entities = 0usize;
    visit_chunks(prepared, world, |cached, chunk| {
        if chunk.entity_count == 0 {
            return;
        }
        total_entities += chunk.entity_count;

        let mut component_ptrs = [std::ptr::null_mut(); super::MAX_QUERY_COMPONENTS];
        for (slot, &index) in cached.component_indices.iter().enumerate() {
            component_ptrs[slot] = resolve_column_ptr(chunk, index);
        }

        let entities = chunk.entities();
        let mut start = 0usize;
        while start < chunk.entity_count {
            let len = (chunk.entity_count - start).min(TARGET_STRIPE_ENTITIES);
            jobs.push(ParallelChunkJob {
                start,
                len,
                entities: RawEntityPtr(entities.as_ptr()),
                component_ptrs,
            });
            start += len;
        }
    });
    cache.jobs = Arc::new(jobs);
    cache.total_entities = total_entities;
    total_entities
}

#[inline(always)]
fn cached_total_entities(cache: &ParallelJobCache, world: &World) -> Option<usize> {
    if !cache
        .cached_world
        .as_ref()
        .is_some_and(|cached| Arc::ptr_eq(cached, world.cache_token()))
        || cache.cached_storage_epoch != Some(world.storage_epoch())
    {
        return None;
    }
    Some(cache.total_entities)
}

#[inline(always)]
fn ensure_chunk_jobs(
    cache: &mut ParallelJobCache,
    prepared: &[CachedArchetype],
    world: &World,
) -> usize {
    cached_total_entities(cache, world)
        .unwrap_or_else(|| collect_chunk_jobs(cache, prepared, world))
}

pub(crate) fn prepare_job_snapshot(
    cache: &mut ParallelJobCache,
    prepared: &[CachedArchetype],
    world: &World,
) -> ParallelJobSnapshot {
    let total_entities = ensure_chunk_jobs(cache, prepared, world);
    ParallelJobSnapshot {
        jobs: Arc::clone(&cache.jobs),
        total_entities,
    }
}

/// Refreshes a scheduler cache without cloning a run snapshot.
pub(crate) fn prepare_job_cache(
    cache: &mut ParallelJobCache,
    prepared: &[CachedArchetype],
    world: &World,
) {
    let _ = ensure_chunk_jobs(cache, prepared, world);
}

/// Clones a run snapshot from a cache that was refreshed during serial system
/// preparation. Scheduler-issued ParViews use this so worker execution never
/// performs cache discovery or stripe construction.
pub(crate) fn prepared_job_snapshot(cache: &ParallelJobCache) -> ParallelJobSnapshot {
    debug_assert!(
        cache.cached_storage_epoch.is_some(),
        "parallel job cache must be prepared before system execution"
    );
    ParallelJobSnapshot {
        jobs: Arc::clone(&cache.jobs),
        total_entities: cache.total_entities,
    }
}

#[cfg(test)]
pub(crate) fn rebuild_count(cache: &ParallelJobCache) -> usize {
    cache.rebuild_count
}

fn should_parallel(total_entities: usize, job_count: usize) -> bool {
    let thread_count = rayon::current_num_threads();
    if job_count <= 1 || job_count < thread_count * 2 {
        return false;
    }

    total_entities >= thread_count * TARGET_STRIPE_ENTITIES * MIN_PARALLEL_STRIPES_PER_THREAD
}

fn seq_for_each_chunk<Q, F>(prepared: &[CachedArchetype], world: &World, f: &F)
where
    Q: QuerySpec,
    F: for<'w> Fn(Q::Chunk<'w>),
{
    visit_chunks(prepared, world, |cached, chunk| unsafe {
        f(Q::chunk_from_raw(chunk, &cached.component_indices));
    });
}

fn seq_for_each<Q, F>(prepared: &[CachedArchetype], world: &World, f: &F)
where
    Q: QuerySpec,
    F: for<'w> Fn(Q::Item<'w>),
{
    visit_chunks(prepared, world, |cached, chunk| unsafe {
        Q::for_each_entity(chunk, &cached.component_indices, &mut |item| f(item));
    });
}

fn seq_for_each_chunk_with_entities<Q, F>(prepared: &[CachedArchetype], world: &World, f: &F)
where
    Q: QuerySpec,
    F: for<'w> Fn(&'w [EntityId], Q::Chunk<'w>),
{
    visit_chunks(prepared, world, |cached, chunk| unsafe {
        f(
            chunk.entities(),
            Q::chunk_from_raw(chunk, &cached.component_indices),
        );
    });
}

fn seq_for_each_with_entities<Q, F>(prepared: &[CachedArchetype], world: &World, f: &F)
where
    Q: QuerySpec,
    F: for<'w> Fn(EntityId, Q::Item<'w>),
{
    visit_chunks(prepared, world, |cached, chunk| unsafe {
        let entities = chunk.entities();
        let mut entity_index = 0usize;
        Q::for_each_entity(chunk, &cached.component_indices, &mut |item| {
            f(entities[entity_index], item);
            entity_index += 1;
        });
    });
}

pub(crate) fn par_for_each<Q, F>(
    prepared: &[CachedArchetype],
    world: &World,
    jobs: ParallelJobSnapshot,
    f: F,
) where
    Q: QuerySpec,
    for<'w> Q::Item<'w>: Send,
    F: for<'w> Fn(Q::Item<'w>) + Send + Sync,
{
    if should_parallel(jobs.total_entities, jobs.jobs.len()) {
        jobs.jobs.par_iter().for_each(|job| unsafe {
            Q::for_each_entity_raw_parts(&job.component_ptrs, job.start, job.len, &mut |item| {
                f(item)
            });
        });
    } else {
        seq_for_each::<Q, _>(prepared, world, &f);
    }
}

pub(crate) fn par_for_each_with_entity<Q, F>(
    prepared: &[CachedArchetype],
    world: &World,
    jobs: ParallelJobSnapshot,
    f: F,
) where
    Q: QuerySpec,
    for<'w> Q::Item<'w>: Send,
    F: for<'w> Fn(EntityId, Q::Item<'w>) + Send + Sync,
{
    if should_parallel(jobs.total_entities, jobs.jobs.len()) {
        jobs.jobs.par_iter().for_each(|job| unsafe {
            let entities = job.entities();
            let mut entity_index = 0usize;
            Q::for_each_entity_raw_parts(&job.component_ptrs, job.start, job.len, &mut |item| {
                f(entities[entity_index], item);
                entity_index += 1;
            });
        });
    } else {
        seq_for_each_with_entities::<Q, _>(prepared, world, &f);
    }
}

pub(crate) fn par_for_each_chunk<Q, F>(
    prepared: &[CachedArchetype],
    world: &World,
    jobs: ParallelJobSnapshot,
    f: F,
) where
    Q: QuerySpec,
    for<'w> Q::Chunk<'w>: Send,
    F: for<'w> Fn(Q::Chunk<'w>) + Send + Sync,
{
    if should_parallel(jobs.total_entities, jobs.jobs.len()) {
        jobs.jobs.par_iter().for_each(|job| unsafe {
            f(Q::chunk_from_raw_parts(
                &job.component_ptrs,
                job.start,
                job.len,
            ));
        });
    } else {
        seq_for_each_chunk::<Q, _>(prepared, world, &f);
    }
}

pub(crate) fn par_for_each_chunk_with_entities<Q, F>(
    prepared: &[CachedArchetype],
    world: &World,
    jobs: ParallelJobSnapshot,
    f: F,
) where
    Q: QuerySpec,
    for<'w> Q::Chunk<'w>: Send,
    F: for<'w> Fn(&'w [EntityId], Q::Chunk<'w>) + Send + Sync,
{
    if should_parallel(jobs.total_entities, jobs.jobs.len()) {
        jobs.jobs.par_iter().for_each(|job| unsafe {
            f(
                job.entities(),
                Q::chunk_from_raw_parts(&job.component_ptrs, job.start, job.len),
            );
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
        let mut query = PreparedQuery::<(&mut Position, &Velocity)>::new();
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
        let mut check = PreparedQuery::<&Position>::new();
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

        let mut init = PreparedQuery::<&mut Velocity>::new();
        init.for_each(&mut world, |velocity| {
            velocity.x = 1.5;
            velocity.y = 0.5;
        });

        let mut query = PreparedQuery::<(&mut Position, &Velocity)>::new();
        query.par_for_each_chunk(&mut world, |(positions, velocities)| {
            for index in 0..positions.len() {
                positions[index].x += velocities[index].x * 2.0;
                positions[index].y += velocities[index].y * 4.0;
            }
        });

        let mut check = PreparedQuery::<&Position>::new();
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

        let mut init = PreparedQuery::<&mut Velocity>::new();
        init.for_each(&mut world, |velocity| {
            velocity.x = 2.0;
            velocity.y = 1.0;
        });

        let mut query = PreparedQuery::<(&mut Position, &Velocity)>::new();
        query.par_for_each_chunk(&mut world, |(positions, velocities)| {
            for index in 0..positions.len() {
                positions[index].x += velocities[index].x;
                positions[index].y += velocities[index].y;
            }
        });

        let mut count = 0usize;
        let mut check = PreparedQuery::<&Position>::new();
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
        let mut query = PreparedQuery::<(&mut Position, Option<&Velocity>)>::new();
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
        let mut check = PreparedQuery::<&Position>::new();
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
        let mut query = PreparedQuery::<(&Position, &Velocity)>::new();
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

        let mut init = PreparedQuery::<&mut Position>::new();
        let mut next = 0u32;
        init.for_each(&mut world, |position| {
            position.x = next as f32;
            next += 1;
        });

        let chunk_visits = AtomicUsize::new(0);
        let mut query = PreparedQuery::<(&Position, &Velocity)>::new();
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
        let mut verify = PreparedQuery::<&Position>::new();
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

        let mut query = PreparedQuery::<(&mut Position, Option<&mut Velocity>)>::new();
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
        let mut check = PreparedQuery::<(&Position, Option<&Velocity>)>::new();
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
        let mut query = PreparedQuery::<(&Position, &Velocity)>::new();
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
        let mut check = PreparedQuery::<&Position>::new();
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
        let mut check = PreparedQuery::<&Position>::new();
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
        let mut check = PreparedQuery::<&Position>::new();
        check.for_each(&mut world, |position| {
            assert_eq!(position.x, 2.0);
            assert_eq!(position.y, 3.0);
            count += 1;
        });
        assert_eq!(count, 64);
    }

    #[test]
    fn par_prepared_query_rebuilds_when_switching_worlds_at_the_same_epoch() {
        let mut positions = World::new();
        positions.spawn((Position::default(),));

        let mut extras = World::new();
        extras.spawn((Extra::default(),));

        let calls = AtomicUsize::new(0);
        let mut prepared = PreparedQuery::<&mut Position>::new();
        prepared.par_for_each_chunk(&mut positions, |_| {
            calls.fetch_add(1, Ordering::Relaxed);
        });
        assert_eq!(calls.load(Ordering::Relaxed), 1);

        calls.store(0, Ordering::Relaxed);
        prepared.par_for_each_chunk(&mut extras, |_| {
            calls.fetch_add(1, Ordering::Relaxed);
        });
        assert_eq!(calls.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn par_for_each_chunk_with_entities_sees_replaced_entities_after_layout_refresh() {
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
