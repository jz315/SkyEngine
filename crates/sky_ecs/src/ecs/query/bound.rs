use super::parallel;
use super::{CachedArchetype, EntityId, QueryFilter, QuerySpec, ReadOnlyQuerySpec, World};
use core::marker::PhantomData;
use std::cell::OnceCell;
use std::sync::Arc;

/// A read-only query view bound to one [`World`].
///
/// Create one with [`World::query`]. Filters are type-level and can be
/// attached with [`filter`](Self::filter).
#[must_use = "queries do nothing until they are iterated"]
pub struct Query<'w, Q, Flt = ()> {
    world: &'w World,
    archetypes: OnceCell<Arc<Vec<CachedArchetype>>>,
    marker: PhantomData<fn() -> (Q, Flt)>,
}

/// A query view with exclusive access to one [`World`].
///
/// Create one with [`World::query_mut`]. The exclusive world borrow permits
/// mutable query parameters while keeping the bound view safe.
#[must_use = "queries do nothing until they are iterated"]
pub struct QueryMut<'w, Q, Flt = ()> {
    world: &'w mut World,
    archetypes: OnceCell<Arc<Vec<CachedArchetype>>>,
    marker: PhantomData<fn() -> (Q, Flt)>,
}

#[inline(always)]
fn cached_archetypes<'a, Q, Flt>(
    world: &World,
    snapshot: &'a OnceCell<Arc<Vec<CachedArchetype>>>,
) -> &'a [CachedArchetype]
where
    Q: QuerySpec + 'static,
    Flt: QueryFilter + 'static,
{
    snapshot
        .get_or_init(|| world.query_snapshot::<Q, Flt>())
        .as_slice()
}

#[inline(always)]
pub(crate) fn run_for_each_chunk<Q, F>(world: &World, archetypes: &[CachedArchetype], mut f: F)
where
    Q: QuerySpec,
    F: for<'a> FnMut(Q::Chunk<'a>),
{
    for cached in archetypes {
        let data = &world.data[cached.data_index];
        for chunk in &data.chunks {
            debug_assert!(chunk.entity_count != 0);
            unsafe {
                f(Q::chunk_from_raw(chunk, &cached.component_indices));
            }
        }
    }
}

#[inline(always)]
pub(crate) fn run_for_each<Q, F>(world: &World, archetypes: &[CachedArchetype], mut f: F)
where
    Q: QuerySpec,
    F: for<'a> FnMut(Q::Item<'a>),
{
    for cached in archetypes {
        let data = &world.data[cached.data_index];
        for chunk in &data.chunks {
            debug_assert!(chunk.entity_count != 0);
            unsafe {
                Q::for_each_entity(chunk, &cached.component_indices, &mut f);
            }
        }
    }
}

#[inline(always)]
pub(crate) fn run_for_each_with_entity<Q, F>(
    world: &World,
    archetypes: &[CachedArchetype],
    mut f: F,
) where
    Q: QuerySpec,
    F: for<'a> FnMut(EntityId, Q::Item<'a>),
{
    for cached in archetypes {
        let data = &world.data[cached.data_index];
        for chunk in &data.chunks {
            let entities = chunk.entities();
            let mut entity_index = 0usize;
            unsafe {
                Q::for_each_entity(chunk, &cached.component_indices, &mut |item| {
                    f(entities[entity_index], item);
                    entity_index += 1;
                });
            }
        }
    }
}

#[inline(always)]
pub(crate) fn run_for_each_chunk_with_entities<Q, F>(
    world: &World,
    archetypes: &[CachedArchetype],
    mut f: F,
) where
    Q: QuerySpec,
    F: for<'a> FnMut(&'a [EntityId], Q::Chunk<'a>),
{
    for cached in archetypes {
        let data = &world.data[cached.data_index];
        for chunk in &data.chunks {
            debug_assert!(chunk.entity_count != 0);
            unsafe {
                f(
                    chunk.entities(),
                    Q::chunk_from_raw(chunk, &cached.component_indices),
                );
            }
        }
    }
}

#[inline]
pub(crate) fn count_matches(world: &World, archetypes: &[CachedArchetype]) -> usize {
    archetypes
        .iter()
        .map(|cached| {
            world.data[cached.data_index]
                .chunks
                .iter()
                .map(|chunk| chunk.entity_count)
                .sum::<usize>()
        })
        .sum()
}

#[inline]
pub(crate) fn matches_nothing(world: &World, archetypes: &[CachedArchetype]) -> bool {
    !archetypes.iter().any(|cached| {
        world.data[cached.data_index]
            .chunks
            .iter()
            .any(|chunk| chunk.entity_count != 0)
    })
}

impl<'w, Q, Flt> Query<'w, Q, Flt>
where
    Q: ReadOnlyQuerySpec + 'static,
    Flt: QueryFilter + 'static,
{
    pub(crate) fn new(world: &'w World) -> Self {
        Self {
            world,
            archetypes: OnceCell::new(),
            marker: PhantomData,
        }
    }

    /// Invokes `f` once per matching chunk with typed component slices.
    #[inline(always)]
    pub fn for_each_chunk<F>(&self, f: F)
    where
        F: for<'a> FnMut(Q::Chunk<'a>),
    {
        run_for_each_chunk::<Q, _>(
            self.world,
            cached_archetypes::<Q, Flt>(self.world, &self.archetypes),
            f,
        );
    }

    /// Processes matching chunk slices in parallel using Rayon.
    /// Chunk execution order is unspecified.
    #[inline]
    pub fn par_for_each_chunk<F>(&self, f: F)
    where
        for<'a> Q::Chunk<'a>: Send,
        F: for<'a> Fn(Q::Chunk<'a>) + Send + Sync,
    {
        let archetypes = cached_archetypes::<Q, Flt>(self.world, &self.archetypes);
        let jobs = self.world.query_parallel_snapshot::<Q, Flt>(archetypes);
        parallel::par_for_each_chunk::<Q, _>(archetypes, self.world, jobs, f);
    }

    /// Invokes `f` once for every matching entity.
    #[inline(always)]
    pub fn for_each<F>(&self, f: F)
    where
        F: for<'a> FnMut(Q::Item<'a>),
    {
        run_for_each::<Q, _>(
            self.world,
            cached_archetypes::<Q, Flt>(self.world, &self.archetypes),
            f,
        );
    }

    /// Processes matching entities in parallel using stripe-sized jobs.
    /// Entity execution order is unspecified.
    #[inline]
    pub fn par_for_each<F>(&self, f: F)
    where
        for<'a> Q::Item<'a>: Send,
        F: for<'a> Fn(Q::Item<'a>) + Send + Sync,
    {
        let archetypes = cached_archetypes::<Q, Flt>(self.world, &self.archetypes);
        let jobs = self.world.query_parallel_snapshot::<Q, Flt>(archetypes);
        parallel::par_for_each::<Q, _>(archetypes, self.world, jobs, f);
    }

    /// Invokes `f` with each matching entity id and its query item.
    #[inline(always)]
    pub fn for_each_with_entity<F>(&self, f: F)
    where
        F: for<'a> FnMut(EntityId, Q::Item<'a>),
    {
        run_for_each_with_entity::<Q, _>(
            self.world,
            cached_archetypes::<Q, Flt>(self.world, &self.archetypes),
            f,
        );
    }

    /// Parallel counterpart to [`for_each_with_entity`](Self::for_each_with_entity).
    #[inline]
    pub fn par_for_each_with_entity<F>(&self, f: F)
    where
        for<'a> Q::Item<'a>: Send,
        F: for<'a> Fn(EntityId, Q::Item<'a>) + Send + Sync,
    {
        let archetypes = cached_archetypes::<Q, Flt>(self.world, &self.archetypes);
        let jobs = self.world.query_parallel_snapshot::<Q, Flt>(archetypes);
        parallel::par_for_each_with_entity::<Q, _>(archetypes, self.world, jobs, f);
    }

    /// Invokes `f` once per chunk with aligned entity ids and component slices.
    #[inline(always)]
    pub fn for_each_chunk_with_entities<F>(&self, f: F)
    where
        F: for<'a> FnMut(&'a [EntityId], Q::Chunk<'a>),
    {
        run_for_each_chunk_with_entities::<Q, _>(
            self.world,
            cached_archetypes::<Q, Flt>(self.world, &self.archetypes),
            f,
        );
    }

    /// Parallel counterpart to [`for_each_chunk_with_entities`](Self::for_each_chunk_with_entities).
    #[inline]
    pub fn par_for_each_chunk_with_entities<F>(&self, f: F)
    where
        for<'a> Q::Chunk<'a>: Send,
        F: for<'a> Fn(&'a [EntityId], Q::Chunk<'a>) + Send + Sync,
    {
        let archetypes = cached_archetypes::<Q, Flt>(self.world, &self.archetypes);
        let jobs = self.world.query_parallel_snapshot::<Q, Flt>(archetypes);
        parallel::par_for_each_chunk_with_entities::<Q, _>(archetypes, self.world, jobs, f);
    }

    /// Returns the number of matching entities.
    #[inline]
    pub fn count(&self) -> usize {
        count_matches(
            self.world,
            cached_archetypes::<Q, Flt>(self.world, &self.archetypes),
        )
    }

    /// Returns the number of archetypes in this query's cached match set.
    #[inline]
    pub fn cached_archetype_count(&self) -> usize {
        cached_archetypes::<Q, Flt>(self.world, &self.archetypes).len()
    }

    /// Returns `true` when no live entity matches this query.
    #[inline]
    pub fn is_empty(&self) -> bool {
        matches_nothing(
            self.world,
            cached_archetypes::<Q, Flt>(self.world, &self.archetypes),
        )
    }
}

impl<'w, Q> Query<'w, Q>
where
    Q: ReadOnlyQuerySpec + 'static,
{
    /// Applies a complete type-level archetype filter to this query.
    ///
    /// Tuples combine filters with AND semantics. Supply the complete filter
    /// in one call; an already filtered query cannot be filtered again.
    pub fn filter<Flt>(self) -> Query<'w, Q, Flt>
    where
        Flt: QueryFilter + 'static,
    {
        // Query preparation is lazy, so consuming the unfiltered builder here
        // does not populate or scan an unused `(Q, ())` cache entry.
        Query::new(self.world)
    }
}

impl<'w, Q, Flt> QueryMut<'w, Q, Flt>
where
    Q: QuerySpec + 'static,
    Flt: QueryFilter + 'static,
{
    pub(crate) fn new(world: &'w mut World) -> Self {
        Self {
            world,
            archetypes: OnceCell::new(),
            marker: PhantomData,
        }
    }

    /// Mutable-query counterpart to [`Query::for_each_chunk`].
    #[inline(always)]
    pub fn for_each_chunk<F>(&mut self, f: F)
    where
        F: for<'a> FnMut(Q::Chunk<'a>),
    {
        let archetypes = cached_archetypes::<Q, Flt>(self.world, &self.archetypes);
        run_for_each_chunk::<Q, _>(self.world, archetypes, f);
    }

    /// Mutable-query counterpart to [`Query::par_for_each_chunk`].
    #[inline]
    pub fn par_for_each_chunk<F>(&mut self, f: F)
    where
        for<'a> Q::Chunk<'a>: Send,
        F: for<'a> Fn(Q::Chunk<'a>) + Send + Sync,
    {
        let archetypes = cached_archetypes::<Q, Flt>(self.world, &self.archetypes);
        let jobs = self.world.query_parallel_snapshot::<Q, Flt>(archetypes);
        parallel::par_for_each_chunk::<Q, _>(archetypes, self.world, jobs, f);
    }

    /// Mutable-query counterpart to [`Query::for_each`].
    #[inline(always)]
    pub fn for_each<F>(&mut self, f: F)
    where
        F: for<'a> FnMut(Q::Item<'a>),
    {
        let archetypes = cached_archetypes::<Q, Flt>(self.world, &self.archetypes);
        run_for_each::<Q, _>(self.world, archetypes, f);
    }

    /// Mutable-query counterpart to [`Query::par_for_each`].
    #[inline]
    pub fn par_for_each<F>(&mut self, f: F)
    where
        for<'a> Q::Item<'a>: Send,
        F: for<'a> Fn(Q::Item<'a>) + Send + Sync,
    {
        let archetypes = cached_archetypes::<Q, Flt>(self.world, &self.archetypes);
        let jobs = self.world.query_parallel_snapshot::<Q, Flt>(archetypes);
        parallel::par_for_each::<Q, _>(archetypes, self.world, jobs, f);
    }

    /// Mutable-query counterpart to [`Query::for_each_with_entity`].
    #[inline(always)]
    pub fn for_each_with_entity<F>(&mut self, f: F)
    where
        F: for<'a> FnMut(EntityId, Q::Item<'a>),
    {
        let archetypes = cached_archetypes::<Q, Flt>(self.world, &self.archetypes);
        run_for_each_with_entity::<Q, _>(self.world, archetypes, f);
    }

    /// Mutable-query counterpart to [`Query::par_for_each_with_entity`].
    #[inline]
    pub fn par_for_each_with_entity<F>(&mut self, f: F)
    where
        for<'a> Q::Item<'a>: Send,
        F: for<'a> Fn(EntityId, Q::Item<'a>) + Send + Sync,
    {
        let archetypes = cached_archetypes::<Q, Flt>(self.world, &self.archetypes);
        let jobs = self.world.query_parallel_snapshot::<Q, Flt>(archetypes);
        parallel::par_for_each_with_entity::<Q, _>(archetypes, self.world, jobs, f);
    }

    /// Mutable-query counterpart to [`Query::for_each_chunk_with_entities`].
    #[inline(always)]
    pub fn for_each_chunk_with_entities<F>(&mut self, f: F)
    where
        F: for<'a> FnMut(&'a [EntityId], Q::Chunk<'a>),
    {
        let archetypes = cached_archetypes::<Q, Flt>(self.world, &self.archetypes);
        run_for_each_chunk_with_entities::<Q, _>(self.world, archetypes, f);
    }

    /// Mutable-query counterpart to [`Query::par_for_each_chunk_with_entities`].
    #[inline]
    pub fn par_for_each_chunk_with_entities<F>(&mut self, f: F)
    where
        for<'a> Q::Chunk<'a>: Send,
        F: for<'a> Fn(&'a [EntityId], Q::Chunk<'a>) + Send + Sync,
    {
        let archetypes = cached_archetypes::<Q, Flt>(self.world, &self.archetypes);
        let jobs = self.world.query_parallel_snapshot::<Q, Flt>(archetypes);
        parallel::par_for_each_chunk_with_entities::<Q, _>(archetypes, self.world, jobs, f);
    }

    /// Returns the number of matching entities.
    #[inline]
    pub fn count(&self) -> usize {
        count_matches(
            self.world,
            cached_archetypes::<Q, Flt>(self.world, &self.archetypes),
        )
    }

    /// Returns the number of archetypes in this query's cached match set.
    #[inline]
    pub fn cached_archetype_count(&self) -> usize {
        cached_archetypes::<Q, Flt>(self.world, &self.archetypes).len()
    }

    /// Returns `true` when no live entity matches this query.
    #[inline]
    pub fn is_empty(&self) -> bool {
        matches_nothing(
            self.world,
            cached_archetypes::<Q, Flt>(self.world, &self.archetypes),
        )
    }
}

impl<'w, Q> QueryMut<'w, Q>
where
    Q: QuerySpec + 'static,
{
    /// Applies a complete type-level archetype filter to this query.
    ///
    /// Tuples combine filters with AND semantics. Supply the complete filter
    /// in one call; an already filtered query cannot be filtered again.
    pub fn filter<Flt>(self) -> QueryMut<'w, Q, Flt>
    where
        Flt: QueryFilter + 'static,
    {
        QueryMut::new(self.world)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{With, Without};
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Clone, Copy, Debug, PartialEq)]
    struct Position(f32);

    #[derive(Clone, Copy)]
    struct Velocity(f32);

    #[derive(Clone, Copy)]
    struct Enemy;

    #[derive(Clone, Copy)]
    struct Dead;

    #[derive(Clone, Copy)]
    struct Flying;

    #[test]
    fn bound_read_query_and_type_filter_compose_naturally() {
        let mut world = World::new();
        let expected = world.spawn((Position(1.0), Velocity(2.0), Enemy));
        world.spawn((Position(3.0), Enemy, Dead));
        world.spawn((Position(4.0),));

        let query = world
            .query::<(&Position, Option<&Velocity>)>()
            .filter::<(With<Enemy>, Without<Dead>)>();

        assert_eq!(query.count(), 1);
        query.for_each_with_entity(|entity, (position, velocity)| {
            assert_eq!(entity, expected);
            assert_eq!(*position, Position(1.0));
            assert_eq!(velocity.map(|value| value.0), Some(2.0));
        });
    }

    #[test]
    fn bound_mut_query_updates_components_without_world_argument() {
        let mut world = World::new();
        world.spawn((Position(1.0), Velocity(2.0)));
        world.spawn((Position(10.0), Velocity(-3.0)));

        world
            .query_mut::<(&mut Position, &Velocity)>()
            .for_each(|(position, velocity)| position.0 += velocity.0);

        let mut values = Vec::new();
        world
            .query::<&Position>()
            .for_each(|position| values.push(position.0));
        values.sort_by(f32::total_cmp);
        assert_eq!(values, vec![3.0, 7.0]);
    }

    static FILTER_CALLS: AtomicUsize = AtomicUsize::new(0);

    struct CountingFilter;

    impl super::super::QueryFilterSealed for CountingFilter {}

    impl QueryFilter for CountingFilter {
        fn matches_archetype(_: &super::super::super::InternalArchetype) -> bool {
            FILTER_CALLS.fetch_add(1, Ordering::Relaxed);
            true
        }
    }

    #[test]
    fn world_cache_reuses_plans_and_scans_only_new_archetypes() {
        let mut world = World::new();
        world.spawn((Position(1.0),));
        world.spawn((Position(2.0), Velocity(0.0)));

        FILTER_CALLS.store(0, Ordering::Relaxed);
        assert_eq!(
            world
                .query::<&Position>()
                .filter::<CountingFilter>()
                .count(),
            2
        );
        assert_eq!(FILTER_CALLS.load(Ordering::Relaxed), 2);

        assert_eq!(
            world
                .query::<&Position>()
                .filter::<CountingFilter>()
                .count(),
            2
        );
        assert_eq!(FILTER_CALLS.load(Ordering::Relaxed), 2);

        world.spawn((Position(3.0), Flying));
        assert_eq!(
            world
                .query::<&Position>()
                .filter::<CountingFilter>()
                .count(),
            3
        );
        assert_eq!(FILTER_CALLS.load(Ordering::Relaxed), 3);

        world.clear();
        world.spawn((Position(4.0), Velocity(0.0)));
        assert_eq!(
            world
                .query::<&Position>()
                .filter::<CountingFilter>()
                .count(),
            1
        );
        assert_eq!(FILTER_CALLS.load(Ordering::Relaxed), 4);
    }

    #[test]
    fn recreated_bound_queries_reuse_world_parallel_jobs() {
        let mut world = World::new();
        for value in 0..32 {
            world.spawn((Position(value as f32),));
        }

        world
            .query::<&Position>()
            .par_for_each_chunk(|positions| assert!(!positions.is_empty()));
        assert_eq!(world.query_parallel_rebuild_count::<&Position, ()>(), 1);

        world
            .query::<&Position>()
            .par_for_each_chunk(|positions| assert!(!positions.is_empty()));
        assert_eq!(world.query_parallel_rebuild_count::<&Position, ()>(), 1);

        world.spawn((Position(99.0), Flying));
        world
            .query::<&Position>()
            .par_for_each_chunk(|positions| assert!(!positions.is_empty()));
        assert_eq!(world.query_parallel_rebuild_count::<&Position, ()>(), 2);
    }

    #[test]
    fn component_value_updates_do_not_invalidate_parallel_jobs() {
        let mut world = World::new();
        let entity = world.spawn((Position(1.0),));

        world.query::<&Position>().par_for_each(|_| {});
        assert_eq!(world.query_parallel_rebuild_count::<&Position, ()>(), 1);

        assert!(world.insert(entity, Position(2.0)));
        world.query::<&Position>().par_for_each(|position| {
            assert_eq!(*position, Position(2.0));
        });
        assert_eq!(world.query_parallel_rebuild_count::<&Position, ()>(), 1);

        assert!(world.despawn(entity));
        world.query::<&Position>().par_for_each(|_| {
            panic!("despawned entities must not be visited");
        });
        assert_eq!(world.query_parallel_rebuild_count::<&Position, ()>(), 2);
    }

    #[test]
    fn filter_chain_only_prepares_the_final_query_type() {
        let mut world = World::new();
        world.spawn((Position(1.0), Enemy));

        let unfiltered = world.query::<&Position>();
        assert_eq!(world.query_cache_len(), 0);

        let filtered = unfiltered.filter::<With<Enemy>>();
        assert_eq!(world.query_cache_len(), 0);
        assert_eq!(filtered.count(), 1);
        assert_eq!(world.query_cache_len(), 1);
    }

    #[test]
    fn bound_parallel_query_uses_the_shared_chunk_runner() {
        let mut world = World::new();
        for value in 0..1_024 {
            world.spawn((Position(value as f32),));
        }

        world
            .query_mut::<&mut Position>()
            .par_for_each_chunk(|positions| {
                for position in positions {
                    position.0 += 1.0;
                }
            });

        let sum = std::sync::atomic::AtomicUsize::new(0);
        world.query::<&Position>().par_for_each_chunk(|positions| {
            let local = positions.iter().map(|position| position.0 as usize).sum();
            sum.fetch_add(local, Ordering::Relaxed);
        });
        assert_eq!(sum.load(Ordering::Relaxed), (1..=1_024).sum());
    }

    #[test]
    fn bound_parallel_entity_query_updates_rows_and_preserves_entity_ids() {
        let mut world = World::new();
        let expected: std::collections::HashSet<_> = (0..1_024)
            .map(|value| world.spawn((Position(value as f32), Velocity(1.0))))
            .collect();

        world
            .query_mut::<(&mut Position, &Velocity)>()
            .par_for_each(|(position, velocity)| position.0 += velocity.0);

        let seen = std::sync::Mutex::new(std::collections::HashSet::new());
        let sum = std::sync::atomic::AtomicUsize::new(0);
        world
            .query::<&Position>()
            .par_for_each_with_entity(|entity, position| {
                seen.lock().unwrap().insert(entity);
                sum.fetch_add(position.0 as usize, Ordering::Relaxed);
            });

        assert_eq!(*seen.lock().unwrap(), expected);
        assert_eq!(sum.load(Ordering::Relaxed), (1..=1_024).sum());
    }
}
