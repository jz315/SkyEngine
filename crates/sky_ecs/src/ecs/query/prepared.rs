use super::filter::QueryFilter;
use super::parallel::{self, ParallelQuerySpec};
use super::param::QuerySpec;
use super::{EntityId, PreparedCache, QueryDescriptor, World};
use core::marker::PhantomData;

/// A typed, cached query over component data.
///
/// Created via [`World::query`] or [`World::query_filtered`].  The query
/// caches which archetypes match and automatically refreshes when the
/// world's archetype set changes (epoch-based invalidation).
///
/// The type parameter `Q` determines which components are accessed
/// (e.g. `&Position`, `(&mut Position, &Velocity)`, `Option<&Health>`).
/// The optional `Flt` restricts matching to archetypes that satisfy
/// the filter (e.g. `With<Enemy>`, `Without<Dead>`).
pub struct PreparedQuery<Q, Flt = ()> {
    descriptor: QueryDescriptor,
    prepared: PreparedCache,
    parallel_jobs: parallel::ParallelJobCache,
    marker: PhantomData<fn() -> (Q, Flt)>,
}

impl<Q: QuerySpec, Flt: QueryFilter> Default for PreparedQuery<Q, Flt> {
    fn default() -> Self {
        Self {
            descriptor: Q::descriptor(),
            prepared: PreparedCache::default(),
            parallel_jobs: parallel::ParallelJobCache::default(),
            marker: PhantomData,
        }
    }
}

impl<Q: QuerySpec, Flt: QueryFilter> PreparedQuery<Q, Flt> {
    /// Creates a new, un-cached query.  Prefer [`World::query`] for
    /// convenience.
    #[inline(always)]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the number of archetypes currently matching this query.
    pub fn cached_archetype_count(&self) -> usize {
        self.prepared.cached_archetype_count()
    }

    #[inline(always)]
    fn prepare(&mut self, world: &World) {
        self.prepared.prepare::<Flt>(world, &self.descriptor);
    }

    /// Iterates *chunk-by-chunk*, providing full slices of each component
    /// column.
    ///
    /// This is the fastest iteration path — ideal for SIMD-style batch
    /// processing.
    #[inline(always)]
    pub fn for_each_chunk<F>(&mut self, world: &World, mut f: F)
    where
        F: for<'w> FnMut(Q::Chunk<'w>),
    {
        self.prepare(world);
        for cached in &self.prepared.archetypes {
            let data = &world.data[cached.data_index];

            for chunk in &data.chunks {
                debug_assert!(chunk.entity_count != 0);
                unsafe {
                    f(Q::chunk_from_raw(chunk, &cached.component_indices));
                }
            }
        }
    }

    /// Iterates *chunk-by-chunk* in parallel using Rayon.
    ///
    /// The closure may run on multiple worker threads and chunk execution
    /// order is unspecified.  Do not perform structural changes, access
    /// [`World`], or mutate shared state without explicit synchronization.
    ///
    /// This API is intended for systems that first gather any resource data
    /// they need, then process component slices in parallel, and finally
    /// apply any queued structural changes after the parallel phase.
    ///
    /// ```rust
    /// use sky_ecs::World;
    ///
    /// #[derive(Clone, Copy)]
    /// struct Position {
    ///     x: f32,
    ///     y: f32,
    /// }
    ///
    /// #[derive(Clone, Copy)]
    /// struct Velocity {
    ///     x: f32,
    ///     y: f32,
    /// }
    ///
    /// let mut world = World::new();
    /// world.spawn((
    ///     Position { x: 0.0, y: 0.0 },
    ///     Velocity { x: 1.0, y: 2.0 },
    /// ));
    ///
    /// let mut query = world.query::<(&mut Position, &Velocity)>();
    /// query.par_for_each_chunk(&world, |(positions, velocities)| {
    ///     for index in 0..positions.len() {
    ///         positions[index].x += velocities[index].x;
    ///         positions[index].y += velocities[index].y;
    ///     }
    /// });
    /// ```
    ///
    /// ```compile_fail
    /// use std::cell::Cell;
    /// use sky_ecs::World;
    ///
    /// let mut world = World::new();
    /// world.spawn((Cell::new(1u32),));
    ///
    /// let mut query = world.query::<&Cell<u32>>();
    /// query.par_for_each_chunk(&world, |_cells| {});
    /// ```
    ///
    /// ```compile_fail
    /// use std::rc::Rc;
    /// use sky_ecs::World;
    ///
    /// let mut world = World::new();
    /// world.spawn((Rc::new(1u32),));
    ///
    /// let mut query = world.query::<&mut Rc<u32>>();
    /// query.par_for_each_chunk(&world, |_values| {});
    /// ```
    pub fn par_for_each_chunk<F>(&mut self, world: &World, f: F)
    where
        Q: ParallelQuerySpec,
        F: for<'w> Fn(<Q as ParallelQuerySpec>::Chunk<'w>) + Send + Sync,
    {
        self.prepare(world);
        parallel::par_for_each_chunk::<Q, _>(&mut self.parallel_jobs, &self.prepared, world, f);
    }

    /// Iterates *entity-by-entity*, providing individual component
    /// references.
    #[inline(always)]
    pub fn for_each<F>(&mut self, world: &World, mut f: F)
    where
        F: for<'w> FnMut(Q::Item<'w>),
    {
        self.prepare(world);
        for cached in &self.prepared.archetypes {
            let data = &world.data[cached.data_index];

            for chunk in &data.chunks {
                debug_assert!(chunk.entity_count != 0);
                unsafe {
                    Q::for_each_entity(chunk, &cached.component_indices, &mut f);
                }
            }
        }
    }

    /// Like [`for_each`](Self::for_each), but also provides the
    /// [`EntityId`] for each matched entity.
    pub fn for_each_with_entity<F>(&mut self, world: &World, mut f: F)
    where
        F: for<'w> FnMut(EntityId, Q::Item<'w>),
    {
        self.prepare(world);
        for cached in &self.prepared.archetypes {
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

    /// Like [`for_each_chunk`](Self::for_each_chunk), but also provides
    /// the entity ID slice for each chunk.
    pub fn for_each_chunk_with_entities<F>(&mut self, world: &World, mut f: F)
    where
        F: for<'w> FnMut(&[EntityId], Q::Chunk<'w>),
    {
        self.prepare(world);
        for cached in &self.prepared.archetypes {
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

    /// Like [`par_for_each_chunk`](Self::par_for_each_chunk), but also
    /// provides the entity ID slice for each chunk.
    ///
    /// Entity slices line up with the component slices for that chunk, but
    /// chunk visitation order remains unspecified.
    pub fn par_for_each_chunk_with_entities<F>(&mut self, world: &World, f: F)
    where
        Q: ParallelQuerySpec,
        F: for<'w> Fn(&'w [EntityId], <Q as ParallelQuerySpec>::Chunk<'w>) + Send + Sync,
    {
        self.prepare(world);
        parallel::par_for_each_chunk_with_entities::<Q, _>(
            &mut self.parallel_jobs,
            &self.prepared,
            world,
            f,
        );
    }

    /// Returns the total number of entities matched by this query.
    pub fn count(&mut self, world: &World) -> usize {
        self.prepare(world);
        self.prepared
            .archetypes
            .iter()
            .map(|cached| {
                world.data[cached.data_index]
                    .chunks
                    .iter()
                    .map(|c| c.entity_count)
                    .sum::<usize>()
            })
            .sum()
    }

    /// Returns `true` if no entities match this query.
    pub fn is_empty(&mut self, world: &World) -> bool {
        self.prepare(world);
        !self.prepared.archetypes.iter().any(|cached| {
            world.data[cached.data_index]
                .chunks
                .iter()
                .any(|c| c.entity_count > 0)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::{create_archetype, World};
    use super::super::{With, Without};
    use super::PreparedQuery;

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
        value: f32,
    }

    #[derive(Clone, Copy, Default)]
    struct Mass {
        value: f32,
    }

    fn spawn(world: &mut World, archetype: super::super::super::Archetype, count: usize) {
        for _ in 0..count {
            world.add_entity(archetype);
        }
    }

    #[test]
    fn typed_single_component_query_reads_and_writes() {
        let archetype = create_archetype().add_rust_component::<Velocity>().build();
        let mut world = World::new();
        spawn(&mut world, archetype, 4);

        let mut init = world.query::<&mut Velocity>();
        let mut expected = 1.0;
        init.for_each(&world, |velocity| {
            velocity.x = expected;
            expected += 1.0;
        });

        let mut sum = 0.0;
        let mut read = world.query::<&Velocity>();
        read.for_each(&world, |velocity| {
            sum += velocity.x;
        });

        assert_eq!(sum, 10.0);
    }

    #[test]
    fn typed_two_component_query_updates_positions() {
        let archetype = create_archetype()
            .add_rust_component::<Position>()
            .add_rust_component::<Velocity>()
            .build();
        let mut world = World::new();
        spawn(&mut world, archetype, 8);

        let mut init = world.query::<&mut Velocity>();
        init.for_each(&world, |velocity| {
            velocity.x = 1.0;
            velocity.y = 2.0;
        });

        let mut query = world.query::<(&mut Position, &Velocity)>();
        query.for_each_chunk(&world, |(positions, velocities)| {
            for index in 0..positions.len() {
                positions[index].x += velocities[index].x * 0.5;
                positions[index].y += velocities[index].y * 0.5;
            }
        });

        let mut check = world.query::<&Position>();
        check.for_each(&world, |position| {
            assert_eq!(position.x, 0.5);
            assert_eq!(position.y, 1.0);
        });
    }

    #[test]
    fn typed_four_component_query_runs_across_matching_archetypes() {
        let archetype = create_archetype()
            .add_rust_component::<Position>()
            .add_rust_component::<Velocity>()
            .add_rust_component::<Extra>()
            .add_rust_component::<Mass>()
            .build();
        let mut world = World::new();
        spawn(&mut world, archetype, 6);

        let mut init = world.query::<(&mut Velocity, &mut Extra, &mut Mass)>();
        init.for_each(&world, |(velocity, extra, mass)| {
            velocity.x = 2.0;
            velocity.y = 4.0;
            extra.value = 1.0;
            mass.value = 3.0;
        });

        let mut query = world.query::<(&mut Position, &Velocity, &mut Extra, &Mass)>();
        query.for_each_chunk(&world, |(positions, velocities, extras, masses)| {
            for index in 0..positions.len() {
                positions[index].x += velocities[index].x * masses[index].value;
                positions[index].y += velocities[index].y * masses[index].value;
                extras[index].value += masses[index].value;
            }
        });

        let mut position_check = world.query::<&Position>();
        position_check.for_each(&world, |position| {
            assert_eq!(position.x, 6.0);
            assert_eq!(position.y, 12.0);
        });

        let mut extra_check = world.query::<&Extra>();
        extra_check.for_each(&world, |extra| {
            assert_eq!(extra.value, 4.0);
        });
    }

    #[test]
    fn query_only_matches_archetypes_with_all_components() {
        let matching = create_archetype()
            .add_rust_component::<Position>()
            .add_rust_component::<Velocity>()
            .build();
        let position_only = create_archetype().add_rust_component::<Position>().build();

        let mut world = World::new();
        spawn(&mut world, matching, 2);
        spawn(&mut world, position_only, 3);

        let mut init = world.query::<&mut Velocity>();
        init.for_each(&world, |velocity| {
            velocity.x = 1.0;
        });

        let mut query = world.query::<(&mut Position, &Velocity)>();
        query.for_each(&world, |(position, velocity)| {
            position.x += velocity.x;
        });

        let mut changed = 0;
        let mut unchanged = 0;
        let mut positions = world.query::<&Position>();
        positions.for_each(&world, |position| {
            if position.x == 1.0 {
                changed += 1;
            } else {
                unchanged += 1;
            }
        });

        assert_eq!(changed, 2);
        assert_eq!(unchanged, 3);
    }

    #[test]
    fn prepared_query_refreshes_when_new_matching_archetype_appears() {
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
        world.add_entity(base);

        let mut init = world.query::<&mut Velocity>();
        init.for_each(&world, |velocity| {
            velocity.x = 1.0;
        });

        let mut prepared = PreparedQuery::<(&mut Position, &Velocity)>::new();
        prepared.for_each(&world, |(position, velocity)| {
            position.x += velocity.x;
        });
        assert_eq!(prepared.cached_archetype_count(), 1);

        world.add_entity(extended);
        let mut init_new = world.query::<(&mut Velocity, &mut Extra)>();
        init_new.for_each(&world, |(velocity, extra)| {
            velocity.x = 1.0;
            extra.value = 2.0;
        });

        prepared.for_each(&world, |(position, velocity)| {
            position.x += velocity.x;
        });
        assert_eq!(prepared.cached_archetype_count(), 2);

        let mut sum = 0.0;
        let mut positions = world.query::<&Position>();
        positions.for_each(&world, |position| {
            sum += position.x;
        });

        assert_eq!(sum, 3.0);
    }

    #[test]
    #[should_panic(expected = "duplicate component type")]
    fn typed_query_rejects_duplicate_types() {
        let archetype = create_archetype().add_rust_component::<Position>().build();
        let mut world = World::new();
        world.add_entity(archetype);

        let mut query = PreparedQuery::<(&mut Position, &mut Position)>::new();
        query.for_each(&world, |_| {});
    }

    #[test]
    fn count_returns_matching_entity_total() {
        let mut world = World::new();
        world.spawn((Position { x: 0.0, y: 0.0 }, Velocity { x: 1.0, y: 1.0 }));
        world.spawn((Position { x: 0.0, y: 0.0 }, Velocity { x: 1.0, y: 1.0 }));
        world.spawn((Position { x: 0.0, y: 0.0 },)); // no Velocity

        let mut query = world.query::<(&Position, &Velocity)>();
        assert_eq!(query.count(&world), 2);

        let mut all_positions = world.query::<&Position>();
        assert_eq!(all_positions.count(&world), 3);
    }

    #[test]
    fn is_empty_reflects_matching_state() {
        let mut world = World::new();
        let mut query = world.query::<(&Position, &Velocity)>();
        assert!(query.is_empty(&world));

        world.spawn((Position { x: 0.0, y: 0.0 }, Velocity { x: 1.0, y: 1.0 }));
        assert!(!query.is_empty(&world));
    }

    #[test]
    fn for_each_with_entity_provides_correct_ids() {
        let mut world = World::new();
        let e1 = world.spawn((Position { x: 1.0, y: 0.0 },));
        let e2 = world.spawn((Position { x: 2.0, y: 0.0 },));

        let mut seen = std::collections::HashMap::new();
        let mut query = world.query::<&Position>();
        query.for_each_with_entity(&world, |entity, position| {
            seen.insert(entity, position.x);
        });

        assert_eq!(seen.len(), 2);
        assert_eq!(seen[&e1], 1.0);
        assert_eq!(seen[&e2], 2.0);
    }

    #[test]
    fn for_each_chunk_with_entities_provides_entity_slices() {
        let mut world = World::new();
        let e1 = world.spawn((Position { x: 1.0, y: 0.0 }, Velocity { x: 0.0, y: 0.0 }));
        let e2 = world.spawn((Position { x: 2.0, y: 0.0 }, Velocity { x: 0.0, y: 0.0 }));

        let mut entity_ids = Vec::new();
        let mut query = world.query::<(&Position, &Velocity)>();
        query.for_each_chunk_with_entities(&world, |entities, (positions, _velocities)| {
            assert_eq!(entities.len(), positions.len());
            entity_ids.extend_from_slice(entities);
        });

        assert!(entity_ids.contains(&e1));
        assert!(entity_ids.contains(&e2));
    }

    #[test]
    fn optional_query_returns_some_when_present_none_when_absent() {
        let mut world = World::new();
        world.spawn((Position { x: 1.0, y: 0.0 }, Velocity { x: 10.0, y: 0.0 }));
        world.spawn((Position { x: 2.0, y: 0.0 },)); // no Velocity

        let mut with_vel = 0;
        let mut without_vel = 0;
        let mut query = world.query::<(&Position, Option<&Velocity>)>();
        query.for_each(&world, |(pos, vel)| {
            if let Some(v) = vel {
                assert_eq!(pos.x, 1.0);
                assert_eq!(v.x, 10.0);
                with_vel += 1;
            } else {
                assert_eq!(pos.x, 2.0);
                without_vel += 1;
            }
        });

        assert_eq!(with_vel, 1);
        assert_eq!(without_vel, 1);
    }

    #[test]
    fn optional_query_chunk_returns_some_slice_or_none() {
        let mut world = World::new();
        world.spawn((Position { x: 1.0, y: 0.0 }, Velocity { x: 5.0, y: 0.0 }));
        world.spawn((Position { x: 2.0, y: 0.0 },));

        let mut chunks_with_vel = 0;
        let mut chunks_without_vel = 0;
        let mut query = world.query::<(&Position, Option<&Velocity>)>();
        query.for_each_chunk(&world, |(positions, opt_velocities)| {
            assert!(!positions.is_empty());
            if opt_velocities.is_some() {
                chunks_with_vel += 1;
            } else {
                chunks_without_vel += 1;
            }
        });

        assert_eq!(chunks_with_vel, 1);
        assert_eq!(chunks_without_vel, 1);
    }

    #[test]
    fn optional_mut_query_writes_when_present() {
        let mut world = World::new();
        world.spawn((Position { x: 0.0, y: 0.0 }, Velocity { x: 1.0, y: 2.0 }));
        world.spawn((Position { x: 0.0, y: 0.0 },));

        let mut query = world.query::<(&mut Position, Option<&Velocity>)>();
        query.for_each(&world, |(pos, vel)| {
            if let Some(v) = vel {
                pos.x += v.x;
                pos.y += v.y;
            } else {
                pos.x = -1.0;
            }
        });

        let mut moved = 0;
        let mut stayed = 0;
        let mut check = world.query::<&Position>();
        check.for_each(&world, |pos| {
            if pos.x == 1.0 && pos.y == 2.0 {
                moved += 1;
            } else if pos.x == -1.0 {
                stayed += 1;
            }
        });

        assert_eq!(moved, 1);
        assert_eq!(stayed, 1);
    }

    #[test]
    fn with_filter_includes_only_matching_archetypes() {
        let mut world = World::new();
        world.spawn((Position { x: 1.0, y: 0.0 }, Velocity { x: 0.0, y: 0.0 }));
        world.spawn((Position { x: 2.0, y: 0.0 },)); // no Velocity

        let mut query = world.query_filtered::<&Position, With<Velocity>>();
        let mut count = 0;
        query.for_each(&world, |pos| {
            assert_eq!(pos.x, 1.0);
            count += 1;
        });

        assert_eq!(count, 1);
    }

    #[test]
    fn without_filter_excludes_matching_archetypes() {
        let mut world = World::new();
        world.spawn((Position { x: 1.0, y: 0.0 }, Velocity { x: 0.0, y: 0.0 }));
        world.spawn((Position { x: 2.0, y: 0.0 },));

        let mut query = world.query_filtered::<&Position, Without<Velocity>>();
        let mut count = 0;
        query.for_each(&world, |pos| {
            assert_eq!(pos.x, 2.0);
            count += 1;
        });

        assert_eq!(count, 1);
    }

    #[test]
    fn combined_filters_intersect() {
        let mut world = World::new();
        world.spawn((
            Position { x: 1.0, y: 0.0 },
            Velocity { x: 0.0, y: 0.0 },
            Extra { value: 0.0 },
        ));
        world.spawn((Position { x: 2.0, y: 0.0 }, Velocity { x: 0.0, y: 0.0 }));
        world.spawn((Position { x: 3.0, y: 0.0 },));

        // Want: has Velocity but NOT Extra
        let mut query = world.query_filtered::<&Position, (With<Velocity>, Without<Extra>)>();
        let mut count = 0;
        query.for_each(&world, |pos| {
            assert_eq!(pos.x, 2.0);
            count += 1;
        });

        assert_eq!(count, 1);
    }
}
