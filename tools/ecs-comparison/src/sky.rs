use crate::common::*;
use crate::shared::sample_entities;
use cgmath::{SquareMatrix, Transform as _};
use criterion::{measurement::WallTime, BenchmarkGroup};
use sky_engine::ecs::{EntityId, PreparedQuery, World};
use std::hint::black_box;

fn world_with_entities(n: usize) -> World {
    let mut world = World::new();
    world.spawn_batch((0..n).map(|_| suite_bundle()));
    world
}

fn fragmented_world() -> World {
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

    world
}

fn heavy_world() -> World {
    let mut world = World::new();
    world.spawn_batch((0..HEAVY_ENTITY_COUNT).map(|_| heavy_bundle()));
    world
}

fn mixed_world() -> (World, Vec<EntityId>, Vec<EntityId>) {
    let mut world = World::new();
    let mut all_entities = Vec::with_capacity(
        MIXED_FRAME_MOVERS + MIXED_FRAME_ENEMIES + MIXED_FRAME_ALLIES + MIXED_FRAME_HEAVY,
    );
    let mut churn_entities = Vec::with_capacity(MIXED_FRAME_CHURN_COUNT);

    for _ in 0..MIXED_FRAME_MOVERS {
        let entity = world.spawn(mixed_mover_bundle());
        if churn_entities.len() < MIXED_FRAME_CHURN_COUNT {
            churn_entities.push(entity);
        }
        all_entities.push(entity);
    }

    for _ in 0..MIXED_FRAME_ENEMIES {
        all_entities.push(world.spawn(mixed_enemy_bundle()));
    }

    for _ in 0..MIXED_FRAME_ALLIES {
        all_entities.push(world.spawn(mixed_ally_bundle()));
    }

    for _ in 0..MIXED_FRAME_HEAVY {
        all_entities.push(world.spawn(mixed_heavy_bundle()));
    }

    let random_entities = sample_entities(&all_entities, MIXED_FRAME_RANDOM_COUNT);
    (world, random_entities, churn_entities)
}

fn mixed_move_step(
    world: &mut World,
    move_query: &mut PreparedQuery<(&mut PositionComponent, &VelocityComponent)>,
) {
    move_query.for_each(&mut *world, |(position, velocity)| {
        position.0 += velocity.0;
    });
}

fn mixed_health_step(
    world: &mut World,
    enemy_query: &mut PreparedQuery<(&mut Health, &Damage)>,
    ally_query: &mut PreparedQuery<(&mut Health, &Regen)>,
) {
    enemy_query.for_each(&mut *world, |(health, damage)| {
        health.0 -= damage.0;
    });

    ally_query.for_each(&mut *world, |(health, regen)| {
        health.0 += regen.0;
    });
}

fn mixed_heavy_step(
    world: &mut World,
    heavy_query: &mut PreparedQuery<(&mut PositionComponent, &TransformComponent)>,
) {
    heavy_query.for_each(&mut *world, |(position, transform)| {
        let base = transform.0;
        let mut matrix = base;
        for _ in 0..MIXED_FRAME_INVERT_COUNT {
            matrix = black_box(base)
                .invert()
                .expect("mixed-frame matrix should be invertible");
        }
        position.0 = matrix.transform_vector(position.0);
    });
}

fn mixed_random_step(world: &World, random_entities: &[EntityId]) {
    for &entity in random_entities {
        black_box(world.get::<PositionComponent>(entity));
    }
}

fn mixed_churn_step(world: &mut World, churn_entities: &[EntityId]) {
    for &entity in churn_entities {
        world.insert(entity, Health(100.0));
    }
    for &entity in churn_entities {
        world.remove::<Health>(entity);
    }
}

fn mixed_spawn_step(world: &mut World, spawned_entities: &mut Vec<EntityId>) {
    spawned_entities.clear();
    for _ in 0..MIXED_FRAME_SPAWN_COUNT {
        spawned_entities.push(world.spawn(light_bundle()));
    }
    for &entity in spawned_entities.iter() {
        world.despawn(entity);
    }
}

fn run_mixed_frame(
    world: &mut World,
    move_query: &mut PreparedQuery<(&mut PositionComponent, &VelocityComponent)>,
    enemy_query: &mut PreparedQuery<(&mut Health, &Damage)>,
    ally_query: &mut PreparedQuery<(&mut Health, &Regen)>,
    heavy_query: &mut PreparedQuery<(&mut PositionComponent, &TransformComponent)>,
    random_entities: &[EntityId],
    churn_entities: &[EntityId],
    spawned_entities: &mut Vec<EntityId>,
) {
    mixed_move_step(world, move_query);
    mixed_health_step(world, enemy_query, ally_query);
    mixed_heavy_step(world, heavy_query);
    mixed_random_step(world, random_entities);
    mixed_churn_step(world, churn_entities);
    mixed_spawn_step(world, spawned_entities);
}

pub fn bench_insert(group: &mut BenchmarkGroup<'_, WallTime>) {
    group.bench_function("batch_10k/sky", |b| {
        b.iter(|| {
            let mut world = World::new();
            world.spawn_batch((0..SIMPLE_ENTITY_COUNT).map(|_| suite_bundle()));
            black_box(&world);
        });
    });

    group.bench_function("single_10k/sky", |b| {
        b.iter(|| {
            let mut world = World::new();
            for _ in 0..SIMPLE_ENTITY_COUNT {
                world.spawn(suite_bundle());
            }
            black_box(&world);
        });
    });
}

pub fn bench_iteration(group: &mut BenchmarkGroup<'_, WallTime>) {
    let mut world = world_with_entities(SIMPLE_ENTITY_COUNT);
    let mut query = PreparedQuery::<(&mut PositionComponent, &VelocityComponent)>::new();

    group.bench_function("simple/sky", |b| {
        b.iter(|| {
            query.for_each(&mut world, |(pos, vel)| {
                pos.0 += vel.0;
            });
            black_box(&world);
        });
    });
}

pub fn bench_iteration_repeated(group: &mut BenchmarkGroup<'_, WallTime>) {
    let mut world = world_with_entities(SIMPLE_ENTITY_COUNT);
    let mut query = PreparedQuery::<(&mut PositionComponent, &VelocityComponent)>::new();

    group.bench_function("simple_x32/sky", |b| {
        b.iter(|| {
            for _ in 0..REPEATED_ITERATION_COUNT {
                query.for_each(&mut world, |(pos, vel)| {
                    pos.0 += vel.0;
                });
            }
            black_box(&world);
        });
    });
}

pub fn bench_iteration_large(group: &mut BenchmarkGroup<'_, WallTime>) {
    let mut world = world_with_entities(LARGE_ITERATION_ENTITY_COUNT);
    let mut query = PreparedQuery::<(&mut PositionComponent, &VelocityComponent)>::new();

    group.bench_function("simple_100k/sky", |b| {
        b.iter(|| {
            query.for_each(&mut world, |(pos, vel)| {
                pos.0 += vel.0;
            });
            black_box(&world);
        });
    });
}

pub fn bench_fragmented_iteration(group: &mut BenchmarkGroup<'_, WallTime>) {
    debug_assert_eq!(FRAGMENTED_VARIANT_COUNT, 26);

    let mut world = fragmented_world();
    let mut query = PreparedQuery::<&mut DataComponent>::new();

    group.bench_function("fragmented/sky", |b| {
        b.iter(|| {
            query.for_each(&mut world, |data| {
                data.0 *= 2.0;
            });
            black_box(&world);
        });
    });
}

pub fn bench_heavy_compute(group: &mut BenchmarkGroup<'_, WallTime>) {
    let mut world = heavy_world();
    let mut query = PreparedQuery::<(&mut PositionComponent, &mut TransformComponent)>::new();

    group.bench_function("heavy/sky", |b| {
        b.iter(|| {
            query.for_each(&mut world, |(position, transform)| {
                let base = transform.0;
                let mut matrix = base;
                for _ in 0..HEAVY_INVERT_COUNT {
                    matrix = black_box(base)
                        .invert()
                        .expect("base heavy matrix should be invertible");
                }
                position.0 = matrix.transform_vector(position.0);
            });
            black_box(&world);
        });
    });
}

pub fn bench_random_access(group: &mut BenchmarkGroup<'_, WallTime>) {
    let mut world = World::new();
    let mut entities: Vec<_> = (0..SIMPLE_ENTITY_COUNT)
        .map(|_| world.spawn(light_bundle()))
        .collect();
    deterministic_shuffle(&mut entities);

    group.bench_function("get/sky", |b| {
        b.iter(|| {
            for &entity in &entities {
                black_box(world.get::<PositionComponent>(entity));
            }
        });
    });
}

pub fn bench_entity_ops(group: &mut BenchmarkGroup<'_, WallTime>) {
    group.bench_function("spawn_despawn_1k/sky", |b| {
        let mut world = World::new();
        b.iter(|| {
            let entities: Vec<_> = (0..ENTITY_OP_COUNT)
                .map(|_| world.spawn(light_bundle()))
                .collect();
            for entity in entities {
                world.despawn(entity);
            }
            black_box(&world);
        });
    });

    group.bench_function("add_remove_component_1k/sky", |b| {
        let mut world = World::new();
        let entities: Vec<_> = (0..ENTITY_OP_COUNT)
            .map(|_| world.spawn(light_bundle()))
            .collect();

        b.iter(|| {
            for &entity in &entities {
                world.insert(entity, Health(100.0));
            }
            for &entity in &entities {
                world.remove::<Health>(entity);
            }
            black_box(&world);
        });
    });
}

pub fn bench_mixed_frame(group: &mut BenchmarkGroup<'_, WallTime>) {
    let (mut world, random_entities, churn_entities) = mixed_world();
    let mut move_query = PreparedQuery::<(&mut PositionComponent, &VelocityComponent)>::new();
    let mut enemy_query = PreparedQuery::<(&mut Health, &Damage)>::new();
    let mut ally_query = PreparedQuery::<(&mut Health, &Regen)>::new();
    let mut heavy_query = PreparedQuery::<(&mut PositionComponent, &TransformComponent)>::new();
    let mut spawned_entities = Vec::with_capacity(MIXED_FRAME_SPAWN_COUNT);

    group.bench_function("frame/sky", |b| {
        b.iter(|| {
            run_mixed_frame(
                &mut world,
                &mut move_query,
                &mut enemy_query,
                &mut ally_query,
                &mut heavy_query,
                &random_entities,
                &churn_entities,
                &mut spawned_entities,
            );
            black_box(&world);
        });
    });
}

pub fn bench_mixed_frame_phases(group: &mut BenchmarkGroup<'_, WallTime>) {
    {
        let (mut world, _, _) = mixed_world();
        let mut query = PreparedQuery::<(&mut PositionComponent, &VelocityComponent)>::new();
        group.bench_function("movement/sky", |b| {
            b.iter(|| {
                mixed_move_step(&mut world, &mut query);
            });
        });
    }

    {
        let (mut world, _, _) = mixed_world();
        let mut enemy_query = PreparedQuery::<(&mut Health, &Damage)>::new();
        let mut ally_query = PreparedQuery::<(&mut Health, &Regen)>::new();
        group.bench_function("health/sky", |b| {
            b.iter(|| {
                for _ in 0..MIXED_PHASE_HEALTH_REPEAT {
                    mixed_health_step(&mut world, &mut enemy_query, &mut ally_query);
                }
                black_box(&world);
            });
        });
    }

    {
        let (mut world, _, _) = mixed_world();
        let mut query = PreparedQuery::<(&mut PositionComponent, &TransformComponent)>::new();
        group.bench_function("heavy/sky", |b| {
            b.iter(|| {
                mixed_heavy_step(&mut world, &mut query);
                black_box(&world);
            });
        });
    }

    {
        let (world, random_entities, _) = mixed_world();
        group.bench_function("random_access/sky", |b| {
            b.iter(|| {
                mixed_random_step(&world, &random_entities);
                black_box(&world);
            });
        });
    }

    {
        let (mut world, _, churn_entities) = mixed_world();
        group.bench_function("structural_churn/sky", |b| {
            b.iter(|| {
                mixed_churn_step(&mut world, &churn_entities);
                black_box(&world);
            });
        });
    }

    {
        let (mut world, _, _) = mixed_world();
        let mut spawned_entities = Vec::with_capacity(MIXED_FRAME_SPAWN_COUNT);
        group.bench_function("spawn_despawn/sky", |b| {
            b.iter(|| {
                for _ in 0..MIXED_PHASE_SPAWN_REPEAT {
                    mixed_spawn_step(&mut world, &mut spawned_entities);
                }
                black_box(&world);
            });
        });
    }
}
