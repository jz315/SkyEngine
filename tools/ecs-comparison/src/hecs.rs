use crate::common::*;
use crate::shared::sample_entities;
use cgmath::{SquareMatrix, Transform as _};
use criterion::{measurement::WallTime, BenchmarkGroup};
use hecs::{Entity as HecsEntity, PreparedQuery, World};
use std::hint::black_box;

fn world_with_entities(n: usize) -> World {
    let mut world = World::new();
    world.spawn_batch((0..n).map(|_| suite_bundle()));
    world
}

fn fragmented_world() -> World {
    let mut world = World::new();

    macro_rules! add_variant {
        ($world:ident; $($tag:ident),* $(,)?) => {
            $( $world.spawn_batch((0..FRAGMENTED_ENTITIES_PER_VARIANT).map(|_| ($tag(0.0), DataComponent(1.0)))); )*
        };
    }

    add_variant!(world; A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y, Z);
    world
}

fn heavy_world() -> World {
    let mut world = World::new();
    world.spawn_batch((0..HEAVY_ENTITY_COUNT).map(|_| heavy_bundle()));
    world
}

fn mixed_world() -> (World, Vec<HecsEntity>, Vec<HecsEntity>) {
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
    world: &World,
    move_query: &mut PreparedQuery<(&mut PositionComponent, &VelocityComponent)>,
) {
    for (_, (position, velocity)) in move_query.query(world).iter() {
        position.0 += velocity.0;
    }
}

fn mixed_health_step(
    world: &World,
    enemy_query: &mut PreparedQuery<(&mut Health, &Damage)>,
    ally_query: &mut PreparedQuery<(&mut Health, &Regen)>,
) {
    for (_, (health, damage)) in enemy_query.query(world).iter() {
        health.0 -= damage.0;
    }

    for (_, (health, regen)) in ally_query.query(world).iter() {
        health.0 += regen.0;
    }
}

fn mixed_heavy_step(
    world: &World,
    heavy_query: &mut PreparedQuery<(&mut PositionComponent, &TransformComponent)>,
) {
    for (_, (position, transform)) in heavy_query.query(world).iter() {
        let base = transform.0;
        let mut matrix = base;
        for _ in 0..MIXED_FRAME_INVERT_COUNT {
            matrix = black_box(base)
                .invert()
                .expect("mixed-frame matrix should be invertible");
        }
        position.0 = matrix.transform_vector(position.0);
    }
}

fn mixed_random_step(world: &World, random_entities: &[HecsEntity]) {
    for &entity in random_entities {
        let _ = black_box(world.get::<&PositionComponent>(entity));
    }
}

fn mixed_churn_step(world: &mut World, churn_entities: &[HecsEntity]) {
    for &entity in churn_entities {
        world.insert_one(entity, Health(100.0)).ok();
    }
    for &entity in churn_entities {
        world.remove_one::<Health>(entity).ok();
    }
}

fn mixed_spawn_step(world: &mut World, spawned_entities: &mut Vec<HecsEntity>) {
    spawned_entities.clear();
    for _ in 0..MIXED_FRAME_SPAWN_COUNT {
        spawned_entities.push(world.spawn(light_bundle()));
    }
    for &entity in spawned_entities.iter() {
        world.despawn(entity).ok();
    }
}

fn run_mixed_frame(
    world: &mut World,
    move_query: &mut PreparedQuery<(&mut PositionComponent, &VelocityComponent)>,
    enemy_query: &mut PreparedQuery<(&mut Health, &Damage)>,
    ally_query: &mut PreparedQuery<(&mut Health, &Regen)>,
    heavy_query: &mut PreparedQuery<(&mut PositionComponent, &TransformComponent)>,
    random_entities: &[HecsEntity],
    churn_entities: &[HecsEntity],
    spawned_entities: &mut Vec<HecsEntity>,
) {
    mixed_move_step(world, move_query);
    mixed_health_step(world, enemy_query, ally_query);
    mixed_heavy_step(world, heavy_query);
    mixed_random_step(world, random_entities);
    mixed_churn_step(world, churn_entities);
    mixed_spawn_step(world, spawned_entities);
}

pub fn bench_insert(group: &mut BenchmarkGroup<'_, WallTime>) {
    group.bench_function("batch_10k/hecs", |b| {
        b.iter(|| {
            let mut world = World::new();
            world.spawn_batch((0..SIMPLE_ENTITY_COUNT).map(|_| suite_bundle()));
            black_box(&world);
        });
    });

    group.bench_function("single_10k/hecs", |b| {
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
    let world = world_with_entities(SIMPLE_ENTITY_COUNT);
    let mut query = PreparedQuery::<(&mut PositionComponent, &VelocityComponent)>::default();

    group.bench_function("simple/hecs", |b| {
        b.iter(|| {
            for (_, (pos, vel)) in query.query(&world).iter() {
                pos.0 += vel.0;
            }
            black_box(&world);
        });
    });
}

pub fn bench_iteration_repeated(group: &mut BenchmarkGroup<'_, WallTime>) {
    let world = world_with_entities(SIMPLE_ENTITY_COUNT);
    let mut query = PreparedQuery::<(&mut PositionComponent, &VelocityComponent)>::default();

    group.bench_function("simple_x32/hecs", |b| {
        b.iter(|| {
            for _ in 0..REPEATED_ITERATION_COUNT {
                for (_, (pos, vel)) in query.query(&world).iter() {
                    pos.0 += vel.0;
                }
            }
            black_box(&world);
        });
    });
}

pub fn bench_iteration_large(group: &mut BenchmarkGroup<'_, WallTime>) {
    let world = world_with_entities(LARGE_ITERATION_ENTITY_COUNT);
    let mut query = PreparedQuery::<(&mut PositionComponent, &VelocityComponent)>::default();

    group.bench_function("simple_100k/hecs", |b| {
        b.iter(|| {
            for (_, (pos, vel)) in query.query(&world).iter() {
                pos.0 += vel.0;
            }
            black_box(&world);
        });
    });
}

pub fn bench_fragmented_iteration(group: &mut BenchmarkGroup<'_, WallTime>) {
    debug_assert_eq!(FRAGMENTED_VARIANT_COUNT, 26);

    let world = fragmented_world();
    let mut query = PreparedQuery::<&mut DataComponent>::default();

    group.bench_function("fragmented/hecs", |b| {
        b.iter(|| {
            for (_, data) in query.query(&world).iter() {
                data.0 *= 2.0;
            }
            black_box(&world);
        });
    });
}

pub fn bench_heavy_compute(group: &mut BenchmarkGroup<'_, WallTime>) {
    let world = heavy_world();
    let mut query = PreparedQuery::<(&mut PositionComponent, &mut TransformComponent)>::default();

    group.bench_function("heavy/hecs", |b| {
        b.iter(|| {
            for (_, (position, transform)) in query.query(&world).iter() {
                let base = transform.0;
                let mut matrix = base;
                for _ in 0..HEAVY_INVERT_COUNT {
                    matrix = black_box(base)
                        .invert()
                        .expect("base heavy matrix should be invertible");
                }
                position.0 = matrix.transform_vector(position.0);
            }
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

    group.bench_function("get/hecs", |b| {
        b.iter(|| {
            for &entity in &entities {
                let _ = black_box(world.get::<&PositionComponent>(entity));
            }
        });
    });
}

pub fn bench_entity_ops(group: &mut BenchmarkGroup<'_, WallTime>) {
    group.bench_function("spawn_despawn_1k/hecs", |b| {
        let mut world = World::new();
        b.iter(|| {
            let entities: Vec<_> = (0..ENTITY_OP_COUNT)
                .map(|_| world.spawn(light_bundle()))
                .collect();
            for entity in entities {
                world.despawn(entity).ok();
            }
            black_box(&world);
        });
    });

    group.bench_function("add_remove_component_1k/hecs", |b| {
        let mut world = World::new();
        let entities: Vec<_> = (0..ENTITY_OP_COUNT)
            .map(|_| world.spawn(light_bundle()))
            .collect();

        b.iter(|| {
            for &entity in &entities {
                world.insert_one(entity, Health(100.0)).ok();
            }
            for &entity in &entities {
                world.remove_one::<Health>(entity).ok();
            }
            black_box(&world);
        });
    });
}

pub fn bench_mixed_frame(group: &mut BenchmarkGroup<'_, WallTime>) {
    let (mut world, random_entities, churn_entities) = mixed_world();
    let mut move_query = PreparedQuery::<(&mut PositionComponent, &VelocityComponent)>::default();
    let mut enemy_query = PreparedQuery::<(&mut Health, &Damage)>::default();
    let mut ally_query = PreparedQuery::<(&mut Health, &Regen)>::default();
    let mut heavy_query = PreparedQuery::<(&mut PositionComponent, &TransformComponent)>::default();
    let mut spawned_entities = Vec::with_capacity(MIXED_FRAME_SPAWN_COUNT);

    group.bench_function("frame/hecs", |b| {
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
        let (world, _, _) = mixed_world();
        let mut query = PreparedQuery::<(&mut PositionComponent, &VelocityComponent)>::default();
        group.bench_function("movement/hecs", |b| {
            b.iter(|| {
                mixed_move_step(&world, &mut query);
            });
        });
    }

    {
        let (world, _, _) = mixed_world();
        let mut enemy_query = PreparedQuery::<(&mut Health, &Damage)>::default();
        let mut ally_query = PreparedQuery::<(&mut Health, &Regen)>::default();
        group.bench_function("health/hecs", |b| {
            b.iter(|| {
                for _ in 0..MIXED_PHASE_HEALTH_REPEAT {
                    mixed_health_step(&world, &mut enemy_query, &mut ally_query);
                }
                black_box(&world);
            });
        });
    }

    {
        let (world, _, _) = mixed_world();
        let mut query = PreparedQuery::<(&mut PositionComponent, &TransformComponent)>::default();
        group.bench_function("heavy/hecs", |b| {
            b.iter(|| {
                mixed_heavy_step(&world, &mut query);
                black_box(&world);
            });
        });
    }

    {
        let (world, random_entities, _) = mixed_world();
        group.bench_function("random_access/hecs", |b| {
            b.iter(|| {
                mixed_random_step(&world, &random_entities);
                black_box(&world);
            });
        });
    }

    {
        let (mut world, _, churn_entities) = mixed_world();
        group.bench_function("structural_churn/hecs", |b| {
            b.iter(|| {
                mixed_churn_step(&mut world, &churn_entities);
                black_box(&world);
            });
        });
    }

    {
        let (mut world, _, _) = mixed_world();
        let mut spawned_entities = Vec::with_capacity(MIXED_FRAME_SPAWN_COUNT);
        group.bench_function("spawn_despawn/hecs", |b| {
            b.iter(|| {
                for _ in 0..MIXED_PHASE_SPAWN_REPEAT {
                    mixed_spawn_step(&mut world, &mut spawned_entities);
                }
                black_box(&world);
            });
        });
    }
}
