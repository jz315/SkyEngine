// Sky Engine benchmarks — the project-side performance regression suite.

#[path = "../common.rs"]
mod common;
use common::*;

use cgmath::{SquareMatrix, Transform as _};
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use sky_engine::ecs::{raw::PreparedQuery, Commands, With, World};
use std::hint::black_box;
use std::time::{Duration, Instant};

// ===========================================================================
//  Helpers
// ===========================================================================

fn world_with_entities(n: usize) -> World {
    let mut world = World::new();
    world.spawn_batch((0..n).map(|_| suite_bundle()));
    world
}

fn world_with_light_entities_ids(n: usize) -> (World, Vec<sky_engine::ecs::EntityId>) {
    let mut world = World::new();
    let entities = (0..n).map(|_| world.spawn(light_bundle())).collect();
    (world, entities)
}

fn world_with_light_entities_and_health_ids(n: usize) -> (World, Vec<sky_engine::ecs::EntityId>) {
    let mut world = World::new();
    let entities = (0..n)
        .map(|_| world.spawn((suite_position(), suite_velocity(), Health(100.0))))
        .collect();
    (world, entities)
}

// ===========================================================================
//  Hot-path: sky_2_of_4 / sky_4_of_4
// ===========================================================================

fn bench_hot_path(c: &mut Criterion) {
    let mut world = World::new();
    for _ in 0..HOT_PATH_ENTITY_COUNT {
        world.spawn(hot_path_bundle());
    }

    let mut query2 = world.query::<(&mut Position2D, &Velocity2D)>();
    let mut query4 = world.query::<(&mut Position2D, &Velocity2D, &mut AuxA, &AuxB)>();

    let mut group = c.benchmark_group("sky_hot_path");

    group.bench_function("2_of_4", |b| {
        b.iter(|| {
            query2.for_each_chunk(&world, |(positions, velocities)| {
                for (position, velocity) in positions.iter_mut().zip(velocities.iter()) {
                    position.x += velocity.x * HOT_PATH_DELTA;
                    position.y += velocity.y * HOT_PATH_DELTA;
                }
            });
        })
    });

    group.bench_function("4_of_4", |b| {
        b.iter(|| {
            query4.for_each_chunk(&world, |(positions, velocities, aux_a, aux_b)| {
                for index in 0..positions.len() {
                    positions[index].x +=
                        velocities[index].x * HOT_PATH_DELTA + aux_b[index].x * HOT_PATH_DELTA;
                    positions[index].y +=
                        velocities[index].y * HOT_PATH_DELTA + aux_b[index].y * HOT_PATH_DELTA;
                    aux_a[index].x += velocities[index].x;
                    aux_a[index].y += aux_b[index].y;
                }
            });
        })
    });

    group.finish();
}

// ===========================================================================
//  Insert
// ===========================================================================

fn bench_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("sky_insert");

    group.bench_function("batch_10k", |b| {
        b.iter(|| {
            let mut world = World::new();
            world.spawn_batch((0..SIMPLE_ENTITY_COUNT).map(|_| suite_bundle()));
            black_box(&world);
        });
    });

    group.bench_function("single_10k", |b| {
        b.iter(|| {
            let mut world = World::new();
            for _ in 0..SIMPLE_ENTITY_COUNT {
                world.spawn(suite_bundle());
            }
            black_box(&world);
        });
    });

    group.finish();
}

// ===========================================================================
//  Iteration
// ===========================================================================

fn bench_simple_iter(c: &mut Criterion) {
    let world = world_with_entities(SIMPLE_ENTITY_COUNT);
    let mut query = PreparedQuery::<(&mut PositionComponent, &VelocityComponent)>::new();

    let mut group = c.benchmark_group("sky_simple_iter");

    group.bench_function("for_each", |b| {
        b.iter(|| {
            query.for_each(&world, |(pos, vel)| {
                pos.0 += vel.0;
            });
        });
    });

    group.bench_function("for_each_chunk", |b| {
        b.iter(|| {
            query.for_each_chunk(&world, |(positions, velocities)| {
                for (pos, vel) in positions.iter_mut().zip(velocities.iter()) {
                    pos.0 += vel.0;
                }
            });
        });
    });

    group.finish();
}

fn bench_fragmented_iter(c: &mut Criterion) {
    debug_assert_eq!(FRAGMENTED_VARIANT_COUNT, 26);

    let mut world = World::new();
    macro_rules! add_variant {
        ($tag:ty) => {{
            for _ in 0..FRAGMENTED_ENTITIES_PER_VARIANT {
                world.spawn((<$tag>::default(), DataComponent(1.0)));
            }
        }};
    }
    add_variant!(A);
    add_variant!(B);
    add_variant!(C);
    add_variant!(D);
    add_variant!(E);
    add_variant!(F);
    add_variant!(G);
    add_variant!(H);
    add_variant!(I);
    add_variant!(J);
    add_variant!(K);
    add_variant!(L);
    add_variant!(M);
    add_variant!(N);
    add_variant!(O);
    add_variant!(P);
    add_variant!(Q);
    add_variant!(R);
    add_variant!(S);
    add_variant!(T);
    add_variant!(U);
    add_variant!(V);
    add_variant!(W);
    add_variant!(X);
    add_variant!(Y);
    add_variant!(Z);

    let mut query = PreparedQuery::<&mut DataComponent>::new();

    c.bench_function("sky_fragmented_iter", |b| {
        b.iter(|| {
            query.for_each(&world, |data| {
                data.0 *= 2.0;
            });
        });
    });
}

fn bench_iter_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("sky_iter_scaling");

    for count in [1_000, 10_000, 100_000] {
        let world = world_with_entities(count);
        let mut query = PreparedQuery::<(&mut PositionComponent, &VelocityComponent)>::new();

        group.bench_with_input(BenchmarkId::new("for_each", count), &count, |b, _| {
            b.iter(|| {
                query.for_each(&world, |(pos, vel)| {
                    pos.0 += vel.0;
                });
            });
        });
    }

    group.finish();
}

fn bench_filtered_query(c: &mut Criterion) {
    let mut world = World::new();

    for i in 0..SIMPLE_ENTITY_COUNT {
        if i % 2 == 0 {
            world.spawn((suite_position(), suite_velocity(), IsEnemy));
        } else {
            world.spawn((suite_position(), suite_velocity(), IsAlly));
        }
    }

    let mut query_all = PreparedQuery::<(&mut PositionComponent, &VelocityComponent)>::new();
    let mut query_filtered =
        PreparedQuery::<(&mut PositionComponent, &VelocityComponent), With<IsEnemy>>::new();

    let mut group = c.benchmark_group("sky_filtered_query");

    group.bench_function("all_10k", |b| {
        b.iter(|| {
            query_all.for_each(&world, |(pos, vel)| {
                pos.0 += vel.0;
            });
        });
    });

    group.bench_function("with_enemy_5k", |b| {
        b.iter(|| {
            query_filtered.for_each(&world, |(pos, vel)| {
                pos.0 += vel.0;
            });
        });
    });

    group.finish();
}

fn bench_heavy_compute(c: &mut Criterion) {
    let mut world = World::new();
    for _ in 0..HEAVY_ENTITY_COUNT {
        world.spawn(heavy_bundle());
    }
    let mut query = PreparedQuery::<(&mut PositionComponent, &mut TransformComponent)>::new();

    c.bench_function("sky_heavy_compute", |b| {
        b.iter(|| {
            query.for_each_chunk(&world, |(positions, transforms)| {
                for (position, transform) in positions.iter_mut().zip(transforms.iter_mut()) {
                    for _ in 0..HEAVY_INVERT_COUNT {
                        transform.0 = transform.0.invert().unwrap();
                    }
                    position.0 = transform.0.transform_vector(position.0);
                }
            });
        });
    });
}

// ===========================================================================
//  Entity operations
// ===========================================================================

fn bench_random_access(c: &mut Criterion) {
    let mut world = World::new();
    let entities: Vec<_> = (0..SIMPLE_ENTITY_COUNT)
        .map(|_| world.spawn(light_bundle()))
        .collect();

    c.bench_function("sky_random_access", |b| {
        b.iter(|| {
            for &entity in &entities {
                black_box(world.get::<PositionComponent>(entity));
            }
        });
    });
}

fn bench_spawn_despawn(c: &mut Criterion) {
    c.bench_function("sky_spawn_despawn_1k", |b| {
        let mut world = World::new();
        b.iter(|| {
            let entities: Vec<_> = (0..ENTITY_OP_COUNT)
                .map(|_| world.spawn(light_bundle()))
                .collect();
            for entity in entities {
                world.despawn(entity);
            }
        });
    });
}

fn bench_spawn_despawn_isolated(c: &mut Criterion) {
    let mut group = c.benchmark_group("sky_spawn_despawn_isolated");

    group.bench_function("spawn_only_1k", |b| {
        let mut world = World::new();
        // Pre-warm: spawn+despawn once so free list is populated
        let warm: Vec<_> = (0..ENTITY_OP_COUNT).map(|_| world.spawn(light_bundle())).collect();
        for e in warm { world.despawn(e); }

        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                let start = Instant::now();
                let entities: Vec<_> = (0..ENTITY_OP_COUNT)
                    .map(|_| world.spawn(light_bundle()))
                    .collect();
                total += start.elapsed();
                black_box(&entities);
                // cleanup outside timing
                for entity in entities {
                    world.despawn(entity);
                }
            }
            total
        });
    });

    group.bench_function("despawn_only_1k", |b| {
        let mut world = World::new();
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                let entities: Vec<_> = (0..ENTITY_OP_COUNT)
                    .map(|_| world.spawn(light_bundle()))
                    .collect();
                let start = Instant::now();
                for entity in entities {
                    world.despawn(entity);
                }
                total += start.elapsed();
            }
            total
        });
    });

    group.finish();
}

fn bench_add_remove_component(c: &mut Criterion) {
    let mut world = World::new();
    let entities: Vec<_> = (0..ENTITY_OP_COUNT)
        .map(|_| world.spawn(light_bundle()))
        .collect();

    c.bench_function("sky_add_remove_component_1k", |b| {
        b.iter(|| {
            for &entity in &entities {
                world.insert(entity, Health(100.0));
            }
            for &entity in &entities {
                world.remove::<Health>(entity);
            }
        });
    });
}

fn bench_component_transition_ops(c: &mut Criterion) {
    let mut group = c.benchmark_group("sky_component_transition");

    group.bench_function("add_component_1k_direct", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                let (mut world, entities) = world_with_light_entities_ids(ENTITY_OP_COUNT);
                let start = Instant::now();
                for &entity in &entities {
                    world.insert(entity, Health(100.0));
                }
                total += start.elapsed();
                black_box(&world);
            }
            total
        });
    });

    group.bench_function("remove_component_1k_direct", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                let (mut world, entities) =
                    world_with_light_entities_and_health_ids(ENTITY_OP_COUNT);
                let start = Instant::now();
                for &entity in &entities {
                    world.remove::<Health>(entity);
                }
                total += start.elapsed();
                black_box(&world);
            }
            total
        });
    });

    group.bench_function("add_component_1k_deferred", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                let (mut world, entities) = world_with_light_entities_ids(ENTITY_OP_COUNT);
                let mut cmds = Commands::new();
                for &entity in &entities {
                    cmds.insert(entity, Health(100.0));
                }
                let start = Instant::now();
                cmds.apply(&mut world);
                total += start.elapsed();
                black_box(&world);
            }
            total
        });
    });

    group.bench_function("add_component_1k_generic_direct", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                let (mut world, entities) = world_with_light_entities_ids(ENTITY_OP_COUNT);
                let start = Instant::now();
                for (index, &entity) in entities.iter().enumerate() {
                    if index & 1 == 0 {
                        world.insert(entity, Health(100.0));
                    } else {
                        world.insert(entity, Damage(5.0));
                    }
                }
                total += start.elapsed();
                black_box(&world);
            }
            total
        });
    });

    group.bench_function("add_component_1k_generic_deferred", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                let (mut world, entities) = world_with_light_entities_ids(ENTITY_OP_COUNT);
                let mut cmds = Commands::new();
                for (index, &entity) in entities.iter().enumerate() {
                    if index & 1 == 0 {
                        cmds.insert(entity, Health(100.0));
                    } else {
                        cmds.insert(entity, Damage(5.0));
                    }
                }
                let start = Instant::now();
                cmds.apply(&mut world);
                total += start.elapsed();
                black_box(&world);
            }
            total
        });
    });

    group.bench_function("remove_component_1k_deferred", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                let (mut world, entities) =
                    world_with_light_entities_and_health_ids(ENTITY_OP_COUNT);
                let mut cmds = Commands::new();
                for &entity in &entities {
                    cmds.remove::<Health>(entity);
                }
                let start = Instant::now();
                cmds.apply(&mut world);
                total += start.elapsed();
                black_box(&world);
            }
            total
        });
    });

    group.finish();
}

fn bench_commands(c: &mut Criterion) {
    let mut group = c.benchmark_group("sky_commands");

    group.bench_function("spawn_1k_deferred", |b| {
        let mut world = World::new();
        b.iter(|| {
            let mut cmds = Commands::new();
            for _ in 0..ENTITY_OP_COUNT {
                cmds.spawn(light_bundle());
            }
            cmds.apply(&mut world);
        });
    });

    group.bench_function("add_remove_component_1k_deferred", |b| {
        let mut world = World::new();
        let entities: Vec<_> = (0..ENTITY_OP_COUNT)
            .map(|_| world.spawn(light_bundle()))
            .collect();

        b.iter(|| {
            let mut cmds = Commands::new();
            for &entity in &entities {
                cmds.insert(entity, Health(100.0));
            }
            for &entity in &entities {
                cmds.remove::<Health>(entity);
            }
            cmds.apply(&mut world);
        });
    });

    group.bench_function("spawn_1k_direct", |b| {
        let mut world = World::new();
        b.iter(|| {
            for _ in 0..ENTITY_OP_COUNT {
                world.spawn(light_bundle());
            }
        });
    });

    group.finish();
}

// ===========================================================================
//  Main
// ===========================================================================

criterion_group!(
    sky_benches,
    bench_hot_path,
    bench_insert,
    bench_simple_iter,
    bench_fragmented_iter,
    bench_iter_scaling,
    bench_filtered_query,
    bench_heavy_compute,
    bench_random_access,
    bench_spawn_despawn,
    bench_spawn_despawn_isolated,
    bench_add_remove_component,
    bench_component_transition_ops,
    bench_commands,
);
criterion_main!(sky_benches);
