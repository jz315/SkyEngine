// Canonical apples-to-apples benchmark suite.
//
// Rules:
// - Only workloads that all engines can express through safe public APIs live here.
// - World population happens outside the timed loop unless construction itself is the benchmark.
// - Query/prepared state is created outside the timed loop for every engine.
// - Sky-specific chunk hot paths stay in benches/sky and are not mixed into the fair suite.

#[path = "../common.rs"]
mod common;
use common::*;

use bevy_ecs::entity::Entity as BevyEntity;
use bevy_ecs::query::QueryState as BevyQueryState;
use bevy_ecs::world::World as BevyWorld;
use cgmath::{SquareMatrix, Transform as _};
use criterion::{criterion_group, criterion_main, Criterion};
use hecs::{Entity as HecsEntity, PreparedQuery as HecsPreparedQuery, World as HecsWorld};
use sky_engine::ecs::{
    raw::PreparedQuery as SkyPreparedQuery, EntityId as SkyEntityId, World as SkyWorld,
};
use std::hint::black_box;

fn sky_world_with_entities(n: usize) -> SkyWorld {
    let mut world = SkyWorld::new();
    world.spawn_batch((0..n).map(|_| suite_bundle()));
    world
}

fn hecs_world_with_entities(n: usize) -> HecsWorld {
    let mut world = HecsWorld::new();
    world.spawn_batch((0..n).map(|_| suite_bundle()));
    world
}

fn bevy_world_with_entities(n: usize) -> BevyWorld {
    let mut world = BevyWorld::new();
    world.spawn_batch((0..n).map(|_| suite_bundle()));
    world
}

fn sky_fragmented_world() -> SkyWorld {
    let mut world = SkyWorld::new();
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

fn hecs_fragmented_world() -> HecsWorld {
    let mut world = HecsWorld::new();
    macro_rules! add_variant {
        ($world:ident; $($tag:ident),* $(,)?) => {
            $( $world.spawn_batch((0..FRAGMENTED_ENTITIES_PER_VARIANT).map(|_| ($tag(0.0), DataComponent(1.0)))); )*
        };
    }
    add_variant!(world; A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y, Z);
    world
}

fn bevy_fragmented_world() -> BevyWorld {
    let mut world = BevyWorld::new();
    macro_rules! add_variant {
        ($world:ident; $($tag:ident),* $(,)?) => {
            $( for _ in 0..FRAGMENTED_ENTITIES_PER_VARIANT { $world.spawn(($tag(0.0), DataComponent(1.0))); } )*
        };
    }
    add_variant!(world; A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y, Z);
    world
}

fn sky_heavy_world() -> SkyWorld {
    let mut world = SkyWorld::new();
    world.spawn_batch((0..HEAVY_ENTITY_COUNT).map(|_| heavy_bundle()));
    world
}

fn hecs_heavy_world() -> HecsWorld {
    let mut world = HecsWorld::new();
    world.spawn_batch((0..HEAVY_ENTITY_COUNT).map(|_| heavy_bundle()));
    world
}

fn bevy_heavy_world() -> BevyWorld {
    let mut world = BevyWorld::new();
    world.spawn_batch((0..HEAVY_ENTITY_COUNT).map(|_| heavy_bundle()));
    world
}

fn sample_entities<T: Copy>(entities: &[T], count: usize) -> Vec<T> {
    assert!(count > 0);
    assert!(entities.len() >= count);

    let mut sampled: Vec<T> = (0..count)
        .map(|index| entities[index * entities.len() / count])
        .collect();
    deterministic_shuffle(&mut sampled);
    sampled
}

fn sky_mixed_world() -> (SkyWorld, Vec<SkyEntityId>, Vec<SkyEntityId>) {
    let mut world = SkyWorld::new();
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

fn hecs_mixed_world() -> (HecsWorld, Vec<HecsEntity>, Vec<HecsEntity>) {
    let mut world = HecsWorld::new();
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

fn bevy_mixed_world() -> (BevyWorld, Vec<BevyEntity>, Vec<BevyEntity>) {
    let mut world = BevyWorld::new();
    let mut all_entities = Vec::with_capacity(
        MIXED_FRAME_MOVERS + MIXED_FRAME_ENEMIES + MIXED_FRAME_ALLIES + MIXED_FRAME_HEAVY,
    );
    let mut churn_entities = Vec::with_capacity(MIXED_FRAME_CHURN_COUNT);

    for _ in 0..MIXED_FRAME_MOVERS {
        let entity = world.spawn(mixed_mover_bundle()).id();
        if churn_entities.len() < MIXED_FRAME_CHURN_COUNT {
            churn_entities.push(entity);
        }
        all_entities.push(entity);
    }

    for _ in 0..MIXED_FRAME_ENEMIES {
        all_entities.push(world.spawn(mixed_enemy_bundle()).id());
    }

    for _ in 0..MIXED_FRAME_ALLIES {
        all_entities.push(world.spawn(mixed_ally_bundle()).id());
    }

    for _ in 0..MIXED_FRAME_HEAVY {
        all_entities.push(world.spawn(mixed_heavy_bundle()).id());
    }

    let random_entities = sample_entities(&all_entities, MIXED_FRAME_RANDOM_COUNT);
    (world, random_entities, churn_entities)
}

fn sky_mixed_move_step(
    world: &SkyWorld,
    move_query: &mut SkyPreparedQuery<(&mut PositionComponent, &VelocityComponent)>,
) {
    move_query.for_each(world, |(position, velocity)| {
        position.0 += velocity.0;
    });
}

fn sky_mixed_health_step(
    world: &SkyWorld,
    enemy_query: &mut SkyPreparedQuery<(&mut Health, &Damage)>,
    ally_query: &mut SkyPreparedQuery<(&mut Health, &Regen)>,
) {
    enemy_query.for_each(world, |(health, damage)| {
        health.0 -= damage.0;
    });

    ally_query.for_each(world, |(health, regen)| {
        health.0 += regen.0;
    });
}

fn sky_mixed_heavy_step(
    world: &SkyWorld,
    heavy_query: &mut SkyPreparedQuery<(&mut PositionComponent, &TransformComponent)>,
) {
    heavy_query.for_each(world, |(position, transform)| {
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

fn sky_mixed_random_step(world: &SkyWorld, random_entities: &[SkyEntityId]) {
    for &entity in random_entities {
        black_box(world.get::<PositionComponent>(entity));
    }
}

fn sky_mixed_churn_step(world: &mut SkyWorld, churn_entities: &[SkyEntityId]) {
    for &entity in churn_entities {
        world.insert(entity, Health(100.0));
    }
    for &entity in churn_entities {
        world.remove::<Health>(entity);
    }
}

fn sky_mixed_spawn_step(world: &mut SkyWorld, spawned_entities: &mut Vec<SkyEntityId>) {
    spawned_entities.clear();
    for _ in 0..MIXED_FRAME_SPAWN_COUNT {
        spawned_entities.push(world.spawn(light_bundle()));
    }
    for &entity in spawned_entities.iter() {
        world.despawn(entity);
    }
}

fn hecs_mixed_move_step(
    world: &HecsWorld,
    move_query: &mut HecsPreparedQuery<(&mut PositionComponent, &VelocityComponent)>,
) {
    for (_, (position, velocity)) in move_query.query(world).iter() {
        position.0 += velocity.0;
    }
}

fn hecs_mixed_health_step(
    world: &HecsWorld,
    enemy_query: &mut HecsPreparedQuery<(&mut Health, &Damage)>,
    ally_query: &mut HecsPreparedQuery<(&mut Health, &Regen)>,
) {
    for (_, (health, damage)) in enemy_query.query(world).iter() {
        health.0 -= damage.0;
    }

    for (_, (health, regen)) in ally_query.query(world).iter() {
        health.0 += regen.0;
    }
}

fn hecs_mixed_heavy_step(
    world: &HecsWorld,
    heavy_query: &mut HecsPreparedQuery<(&mut PositionComponent, &TransformComponent)>,
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

fn hecs_mixed_random_step(world: &HecsWorld, random_entities: &[HecsEntity]) {
    for &entity in random_entities {
        let _ = black_box(world.get::<&PositionComponent>(entity));
    }
}

fn hecs_mixed_churn_step(world: &mut HecsWorld, churn_entities: &[HecsEntity]) {
    for &entity in churn_entities {
        world.insert_one(entity, Health(100.0)).ok();
    }
    for &entity in churn_entities {
        world.remove_one::<Health>(entity).ok();
    }
}

fn hecs_mixed_spawn_step(world: &mut HecsWorld, spawned_entities: &mut Vec<HecsEntity>) {
    spawned_entities.clear();
    for _ in 0..MIXED_FRAME_SPAWN_COUNT {
        spawned_entities.push(world.spawn(light_bundle()));
    }
    for &entity in spawned_entities.iter() {
        world.despawn(entity).ok();
    }
}

fn bevy_mixed_move_step(
    world: &mut BevyWorld,
    move_query: &mut BevyQueryState<(&mut PositionComponent, &VelocityComponent)>,
) {
    for (mut position, velocity) in move_query.iter_mut(world) {
        position.0 += velocity.0;
    }
}

fn bevy_mixed_health_step(
    world: &mut BevyWorld,
    enemy_query: &mut BevyQueryState<(&mut Health, &Damage)>,
    ally_query: &mut BevyQueryState<(&mut Health, &Regen)>,
) {
    for (mut health, damage) in enemy_query.iter_mut(world) {
        health.0 -= damage.0;
    }

    for (mut health, regen) in ally_query.iter_mut(world) {
        health.0 += regen.0;
    }
}

fn bevy_mixed_heavy_step(
    world: &mut BevyWorld,
    heavy_query: &mut BevyQueryState<(&mut PositionComponent, &TransformComponent)>,
) {
    for (mut position, transform) in heavy_query.iter_mut(world) {
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

fn bevy_mixed_random_step(world: &BevyWorld, random_entities: &[BevyEntity]) {
    for &entity in random_entities {
        black_box(world.get::<PositionComponent>(entity));
    }
}

fn bevy_mixed_churn_step(world: &mut BevyWorld, churn_entities: &[BevyEntity]) {
    for &entity in churn_entities {
        world.entity_mut(entity).insert(Health(100.0));
    }
    for &entity in churn_entities {
        world.entity_mut(entity).remove::<Health>();
    }
}

fn bevy_mixed_spawn_step(world: &mut BevyWorld, spawned_entities: &mut Vec<BevyEntity>) {
    spawned_entities.clear();
    for _ in 0..MIXED_FRAME_SPAWN_COUNT {
        spawned_entities.push(world.spawn(light_bundle()).id());
    }
    for &entity in spawned_entities.iter() {
        world.despawn(entity);
    }
}

fn run_sky_mixed_frame(
    world: &mut SkyWorld,
    move_query: &mut SkyPreparedQuery<(&mut PositionComponent, &VelocityComponent)>,
    enemy_query: &mut SkyPreparedQuery<(&mut Health, &Damage)>,
    ally_query: &mut SkyPreparedQuery<(&mut Health, &Regen)>,
    heavy_query: &mut SkyPreparedQuery<(&mut PositionComponent, &TransformComponent)>,
    random_entities: &[SkyEntityId],
    churn_entities: &[SkyEntityId],
    spawned_entities: &mut Vec<SkyEntityId>,
) {
    sky_mixed_move_step(world, move_query);
    sky_mixed_health_step(world, enemy_query, ally_query);
    sky_mixed_heavy_step(world, heavy_query);
    sky_mixed_random_step(world, random_entities);
    sky_mixed_churn_step(world, churn_entities);
    sky_mixed_spawn_step(world, spawned_entities);
}

fn run_hecs_mixed_frame(
    world: &mut HecsWorld,
    move_query: &mut HecsPreparedQuery<(&mut PositionComponent, &VelocityComponent)>,
    enemy_query: &mut HecsPreparedQuery<(&mut Health, &Damage)>,
    ally_query: &mut HecsPreparedQuery<(&mut Health, &Regen)>,
    heavy_query: &mut HecsPreparedQuery<(&mut PositionComponent, &TransformComponent)>,
    random_entities: &[HecsEntity],
    churn_entities: &[HecsEntity],
    spawned_entities: &mut Vec<HecsEntity>,
) {
    hecs_mixed_move_step(world, move_query);
    hecs_mixed_health_step(world, enemy_query, ally_query);
    hecs_mixed_heavy_step(world, heavy_query);
    hecs_mixed_random_step(world, random_entities);
    hecs_mixed_churn_step(world, churn_entities);
    hecs_mixed_spawn_step(world, spawned_entities);
}

fn run_bevy_mixed_frame(
    world: &mut BevyWorld,
    move_query: &mut BevyQueryState<(&mut PositionComponent, &VelocityComponent)>,
    enemy_query: &mut BevyQueryState<(&mut Health, &Damage)>,
    ally_query: &mut BevyQueryState<(&mut Health, &Regen)>,
    heavy_query: &mut BevyQueryState<(&mut PositionComponent, &TransformComponent)>,
    random_entities: &[BevyEntity],
    churn_entities: &[BevyEntity],
    spawned_entities: &mut Vec<BevyEntity>,
) {
    bevy_mixed_move_step(world, move_query);
    bevy_mixed_health_step(world, enemy_query, ally_query);
    bevy_mixed_heavy_step(world, heavy_query);
    bevy_mixed_random_step(world, random_entities);
    bevy_mixed_churn_step(world, churn_entities);
    bevy_mixed_spawn_step(world, spawned_entities);
}

fn bench_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("fair_insert");

    group.bench_function("batch_10k/sky", |b| {
        b.iter(|| {
            let mut world = SkyWorld::new();
            world.spawn_batch((0..SIMPLE_ENTITY_COUNT).map(|_| suite_bundle()));
            black_box(&world);
        });
    });

    group.bench_function("batch_10k/hecs", |b| {
        b.iter(|| {
            let mut world = HecsWorld::new();
            world.spawn_batch((0..SIMPLE_ENTITY_COUNT).map(|_| suite_bundle()));
            black_box(&world);
        });
    });

    group.bench_function("batch_10k/bevy", |b| {
        b.iter(|| {
            let mut world = BevyWorld::new();
            world.spawn_batch((0..SIMPLE_ENTITY_COUNT).map(|_| suite_bundle()));
            black_box(&world);
        });
    });

    group.bench_function("single_10k/sky", |b| {
        b.iter(|| {
            let mut world = SkyWorld::new();
            for _ in 0..SIMPLE_ENTITY_COUNT {
                world.spawn(suite_bundle());
            }
            black_box(&world);
        });
    });

    group.bench_function("single_10k/hecs", |b| {
        b.iter(|| {
            let mut world = HecsWorld::new();
            for _ in 0..SIMPLE_ENTITY_COUNT {
                world.spawn(suite_bundle());
            }
            black_box(&world);
        });
    });

    group.bench_function("single_10k/bevy", |b| {
        b.iter(|| {
            let mut world = BevyWorld::new();
            for _ in 0..SIMPLE_ENTITY_COUNT {
                world.spawn(suite_bundle());
            }
            black_box(&world);
        });
    });

    group.finish();
}

fn bench_iteration(c: &mut Criterion) {
    let sky_world = sky_world_with_entities(SIMPLE_ENTITY_COUNT);
    let mut sky_query = SkyPreparedQuery::<(&mut PositionComponent, &VelocityComponent)>::new();

    let hecs_world = hecs_world_with_entities(SIMPLE_ENTITY_COUNT);
    let mut hecs_query =
        HecsPreparedQuery::<(&mut PositionComponent, &VelocityComponent)>::default();

    let mut bevy_world = bevy_world_with_entities(SIMPLE_ENTITY_COUNT);
    let mut bevy_query = bevy_world.query::<(&mut PositionComponent, &VelocityComponent)>();

    let mut group = c.benchmark_group("fair_iteration");

    group.bench_function("simple/sky", |b| {
        b.iter(|| {
            sky_query.for_each(&sky_world, |(pos, vel)| {
                pos.0 += vel.0;
            });
            black_box(&sky_world);
        });
    });

    group.bench_function("simple/hecs", |b| {
        b.iter(|| {
            for (_, (pos, vel)) in hecs_query.query(&hecs_world).iter() {
                pos.0 += vel.0;
            }
            black_box(&hecs_world);
        });
    });

    group.bench_function("simple/bevy", |b| {
        b.iter(|| {
            for (mut pos, vel) in bevy_query.iter_mut(&mut bevy_world) {
                pos.0 += vel.0;
            }
            black_box(&bevy_world);
        });
    });

    group.finish();
}

fn bench_fragmented_iteration(c: &mut Criterion) {
    debug_assert_eq!(FRAGMENTED_VARIANT_COUNT, 26);

    let sky_world = sky_fragmented_world();
    let mut sky_query = SkyPreparedQuery::<&mut DataComponent>::new();

    let hecs_world = hecs_fragmented_world();
    let mut hecs_query = HecsPreparedQuery::<&mut DataComponent>::default();

    let mut bevy_world = bevy_fragmented_world();
    let mut bevy_query = bevy_world.query::<&mut DataComponent>();

    let mut group = c.benchmark_group("fair_fragmented_iteration");

    group.bench_function("fragmented/sky", |b| {
        b.iter(|| {
            sky_query.for_each(&sky_world, |data| {
                data.0 *= 2.0;
            });
            black_box(&sky_world);
        });
    });

    group.bench_function("fragmented/hecs", |b| {
        b.iter(|| {
            for (_, data) in hecs_query.query(&hecs_world).iter() {
                data.0 *= 2.0;
            }
            black_box(&hecs_world);
        });
    });

    group.bench_function("fragmented/bevy", |b| {
        b.iter(|| {
            for mut data in bevy_query.iter_mut(&mut bevy_world) {
                data.0 *= 2.0;
            }
            black_box(&bevy_world);
        });
    });

    group.finish();
}

fn bench_heavy_compute(c: &mut Criterion) {
    let sky_world = sky_heavy_world();
    let mut sky_query =
        SkyPreparedQuery::<(&mut PositionComponent, &mut TransformComponent)>::new();

    let hecs_world = hecs_heavy_world();
    let mut hecs_query =
        HecsPreparedQuery::<(&mut PositionComponent, &mut TransformComponent)>::default();

    let mut bevy_world = bevy_heavy_world();
    let mut bevy_query = bevy_world.query::<(&mut PositionComponent, &mut TransformComponent)>();

    let mut group = c.benchmark_group("fair_heavy_compute");

    group.bench_function("heavy/sky", |b| {
        b.iter(|| {
            sky_query.for_each(&sky_world, |(position, transform)| {
                let base = transform.0;
                let mut matrix = base;
                for _ in 0..HEAVY_INVERT_COUNT {
                    matrix = black_box(base)
                        .invert()
                        .expect("base heavy matrix should be invertible");
                }
                position.0 = matrix.transform_vector(position.0);
            });
            black_box(&sky_world);
        });
    });

    group.bench_function("heavy/hecs", |b| {
        b.iter(|| {
            for (_, (position, transform)) in hecs_query.query(&hecs_world).iter() {
                let base = transform.0;
                let mut matrix = base;
                for _ in 0..HEAVY_INVERT_COUNT {
                    matrix = black_box(base)
                        .invert()
                        .expect("base heavy matrix should be invertible");
                }
                position.0 = matrix.transform_vector(position.0);
            }
            black_box(&hecs_world);
        });
    });

    group.bench_function("heavy/bevy", |b| {
        b.iter(|| {
            for (mut position, transform) in bevy_query.iter_mut(&mut bevy_world) {
                let base = transform.0;
                let mut matrix = base;
                for _ in 0..HEAVY_INVERT_COUNT {
                    matrix = black_box(base)
                        .invert()
                        .expect("base heavy matrix should be invertible");
                }
                position.0 = matrix.transform_vector(position.0);
            }
            black_box(&bevy_world);
        });
    });

    group.finish();
}

fn bench_random_access(c: &mut Criterion) {
    let mut sky_world = SkyWorld::new();
    let mut sky_entities: Vec<_> = (0..SIMPLE_ENTITY_COUNT)
        .map(|_| sky_world.spawn(light_bundle()))
        .collect();
    deterministic_shuffle(&mut sky_entities);

    let mut hecs_world = HecsWorld::new();
    let mut hecs_entities: Vec<_> = (0..SIMPLE_ENTITY_COUNT)
        .map(|_| hecs_world.spawn(light_bundle()))
        .collect();
    deterministic_shuffle(&mut hecs_entities);

    let mut bevy_world = BevyWorld::new();
    let mut bevy_entities: Vec<_> = (0..SIMPLE_ENTITY_COUNT)
        .map(|_| bevy_world.spawn(light_bundle()).id())
        .collect();
    deterministic_shuffle(&mut bevy_entities);

    let mut group = c.benchmark_group("fair_random_access");

    group.bench_function("get/sky", |b| {
        b.iter(|| {
            for &entity in &sky_entities {
                black_box(sky_world.get::<PositionComponent>(entity));
            }
        });
    });

    group.bench_function("get/hecs", |b| {
        b.iter(|| {
            for &entity in &hecs_entities {
                let _ = black_box(hecs_world.get::<&PositionComponent>(entity));
            }
        });
    });

    group.bench_function("get/bevy", |b| {
        b.iter(|| {
            for &entity in &bevy_entities {
                black_box(bevy_world.get::<PositionComponent>(entity));
            }
        });
    });

    group.finish();
}

fn bench_entity_ops(c: &mut Criterion) {
    let mut group = c.benchmark_group("fair_entity_ops");

    group.bench_function("spawn_despawn_1k/sky", |b| {
        let mut world = SkyWorld::new();
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

    group.bench_function("spawn_despawn_1k/hecs", |b| {
        let mut world = HecsWorld::new();
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

    group.bench_function("spawn_despawn_1k/bevy", |b| {
        let mut world = BevyWorld::new();
        b.iter(|| {
            let entities: Vec<_> = (0..ENTITY_OP_COUNT)
                .map(|_| world.spawn(light_bundle()).id())
                .collect();
            for entity in entities {
                world.despawn(entity);
            }
            black_box(&world);
        });
    });

    group.bench_function("add_remove_component_1k/sky", |b| {
        let mut world = SkyWorld::new();
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

    group.bench_function("add_remove_component_1k/hecs", |b| {
        let mut world = HecsWorld::new();
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

    group.bench_function("add_remove_component_1k/bevy", |b| {
        let mut world = BevyWorld::new();
        let entities: Vec<_> = (0..ENTITY_OP_COUNT)
            .map(|_| world.spawn(light_bundle()).id())
            .collect();

        b.iter(|| {
            for &entity in &entities {
                world.entity_mut(entity).insert(Health(100.0));
            }
            for &entity in &entities {
                world.entity_mut(entity).remove::<Health>();
            }
            black_box(&world);
        });
    });

    group.finish();
}

fn bench_mixed_frame(c: &mut Criterion) {
    let (mut sky_world, sky_random_entities, sky_churn_entities) = sky_mixed_world();
    let mut sky_move_query =
        SkyPreparedQuery::<(&mut PositionComponent, &VelocityComponent)>::new();
    let mut sky_enemy_query = SkyPreparedQuery::<(&mut Health, &Damage)>::new();
    let mut sky_ally_query = SkyPreparedQuery::<(&mut Health, &Regen)>::new();
    let mut sky_heavy_query =
        SkyPreparedQuery::<(&mut PositionComponent, &TransformComponent)>::new();
    let mut sky_spawned_entities = Vec::with_capacity(MIXED_FRAME_SPAWN_COUNT);

    let (mut hecs_world, hecs_random_entities, hecs_churn_entities) = hecs_mixed_world();
    let mut hecs_move_query =
        HecsPreparedQuery::<(&mut PositionComponent, &VelocityComponent)>::default();
    let mut hecs_enemy_query = HecsPreparedQuery::<(&mut Health, &Damage)>::default();
    let mut hecs_ally_query = HecsPreparedQuery::<(&mut Health, &Regen)>::default();
    let mut hecs_heavy_query =
        HecsPreparedQuery::<(&mut PositionComponent, &TransformComponent)>::default();
    let mut hecs_spawned_entities = Vec::with_capacity(MIXED_FRAME_SPAWN_COUNT);

    let (mut bevy_world, bevy_random_entities, bevy_churn_entities) = bevy_mixed_world();
    let mut bevy_move_query = bevy_world.query::<(&mut PositionComponent, &VelocityComponent)>();
    let mut bevy_enemy_query = bevy_world.query::<(&mut Health, &Damage)>();
    let mut bevy_ally_query = bevy_world.query::<(&mut Health, &Regen)>();
    let mut bevy_heavy_query = bevy_world.query::<(&mut PositionComponent, &TransformComponent)>();
    let mut bevy_spawned_entities = Vec::with_capacity(MIXED_FRAME_SPAWN_COUNT);

    let mut group = c.benchmark_group("fair_mixed_frame");

    group.bench_function("frame/sky", |b| {
        b.iter(|| {
            run_sky_mixed_frame(
                &mut sky_world,
                &mut sky_move_query,
                &mut sky_enemy_query,
                &mut sky_ally_query,
                &mut sky_heavy_query,
                &sky_random_entities,
                &sky_churn_entities,
                &mut sky_spawned_entities,
            );
            black_box(&sky_world);
        });
    });

    group.bench_function("frame/hecs", |b| {
        b.iter(|| {
            run_hecs_mixed_frame(
                &mut hecs_world,
                &mut hecs_move_query,
                &mut hecs_enemy_query,
                &mut hecs_ally_query,
                &mut hecs_heavy_query,
                &hecs_random_entities,
                &hecs_churn_entities,
                &mut hecs_spawned_entities,
            );
            black_box(&hecs_world);
        });
    });

    group.bench_function("frame/bevy", |b| {
        b.iter(|| {
            run_bevy_mixed_frame(
                &mut bevy_world,
                &mut bevy_move_query,
                &mut bevy_enemy_query,
                &mut bevy_ally_query,
                &mut bevy_heavy_query,
                &bevy_random_entities,
                &bevy_churn_entities,
                &mut bevy_spawned_entities,
            );
            black_box(&bevy_world);
        });
    });

    group.finish();
}

fn bench_mixed_frame_phases(c: &mut Criterion) {
    let mut group = c.benchmark_group("fair_mixed_frame_phases");

    {
        let (sky_world, _, _) = sky_mixed_world();
        let mut sky_query = SkyPreparedQuery::<(&mut PositionComponent, &VelocityComponent)>::new();
        group.bench_function("movement/sky", |b| {
            b.iter(|| {
                sky_mixed_move_step(&sky_world, &mut sky_query);
            });
        });
    }

    {
        let (hecs_world, _, _) = hecs_mixed_world();
        let mut hecs_query =
            HecsPreparedQuery::<(&mut PositionComponent, &VelocityComponent)>::default();
        group.bench_function("movement/hecs", |b| {
            b.iter(|| {
                hecs_mixed_move_step(&hecs_world, &mut hecs_query);
            });
        });
    }

    {
        let (mut bevy_world, _, _) = bevy_mixed_world();
        let mut bevy_query = bevy_world.query::<(&mut PositionComponent, &VelocityComponent)>();
        group.bench_function("movement/bevy", |b| {
            b.iter(|| {
                bevy_mixed_move_step(&mut bevy_world, &mut bevy_query);
            });
        });
    }

    {
        let (sky_world, _, _) = sky_mixed_world();
        let mut sky_enemy_query = SkyPreparedQuery::<(&mut Health, &Damage)>::new();
        let mut sky_ally_query = SkyPreparedQuery::<(&mut Health, &Regen)>::new();
        group.bench_function("health/sky", |b| {
            b.iter(|| {
                for _ in 0..MIXED_PHASE_HEALTH_REPEAT {
                    sky_mixed_health_step(&sky_world, &mut sky_enemy_query, &mut sky_ally_query);
                }
                black_box(&sky_world);
            });
        });
    }

    {
        let (hecs_world, _, _) = hecs_mixed_world();
        let mut hecs_enemy_query = HecsPreparedQuery::<(&mut Health, &Damage)>::default();
        let mut hecs_ally_query = HecsPreparedQuery::<(&mut Health, &Regen)>::default();
        group.bench_function("health/hecs", |b| {
            b.iter(|| {
                for _ in 0..MIXED_PHASE_HEALTH_REPEAT {
                    hecs_mixed_health_step(
                        &hecs_world,
                        &mut hecs_enemy_query,
                        &mut hecs_ally_query,
                    );
                }
                black_box(&hecs_world);
            });
        });
    }

    {
        let (mut bevy_world, _, _) = bevy_mixed_world();
        let mut bevy_enemy_query = bevy_world.query::<(&mut Health, &Damage)>();
        let mut bevy_ally_query = bevy_world.query::<(&mut Health, &Regen)>();
        group.bench_function("health/bevy", |b| {
            b.iter(|| {
                for _ in 0..MIXED_PHASE_HEALTH_REPEAT {
                    bevy_mixed_health_step(
                        &mut bevy_world,
                        &mut bevy_enemy_query,
                        &mut bevy_ally_query,
                    );
                }
                black_box(&bevy_world);
            });
        });
    }

    {
        let (sky_world, _, _) = sky_mixed_world();
        let mut sky_query =
            SkyPreparedQuery::<(&mut PositionComponent, &TransformComponent)>::new();
        group.bench_function("heavy/sky", |b| {
            b.iter(|| {
                sky_mixed_heavy_step(&sky_world, &mut sky_query);
                black_box(&sky_world);
            });
        });
    }

    {
        let (hecs_world, _, _) = hecs_mixed_world();
        let mut hecs_query =
            HecsPreparedQuery::<(&mut PositionComponent, &TransformComponent)>::default();
        group.bench_function("heavy/hecs", |b| {
            b.iter(|| {
                hecs_mixed_heavy_step(&hecs_world, &mut hecs_query);
                black_box(&hecs_world);
            });
        });
    }

    {
        let (mut bevy_world, _, _) = bevy_mixed_world();
        let mut bevy_query = bevy_world.query::<(&mut PositionComponent, &TransformComponent)>();
        group.bench_function("heavy/bevy", |b| {
            b.iter(|| {
                bevy_mixed_heavy_step(&mut bevy_world, &mut bevy_query);
                black_box(&bevy_world);
            });
        });
    }

    {
        let (sky_world, sky_random_entities, _) = sky_mixed_world();
        group.bench_function("random_access/sky", |b| {
            b.iter(|| {
                sky_mixed_random_step(&sky_world, &sky_random_entities);
                black_box(&sky_world);
            });
        });
    }

    {
        let (hecs_world, hecs_random_entities, _) = hecs_mixed_world();
        group.bench_function("random_access/hecs", |b| {
            b.iter(|| {
                hecs_mixed_random_step(&hecs_world, &hecs_random_entities);
                black_box(&hecs_world);
            });
        });
    }

    {
        let (bevy_world, bevy_random_entities, _) = bevy_mixed_world();
        group.bench_function("random_access/bevy", |b| {
            b.iter(|| {
                bevy_mixed_random_step(&bevy_world, &bevy_random_entities);
                black_box(&bevy_world);
            });
        });
    }

    {
        let (mut sky_world, _, sky_churn_entities) = sky_mixed_world();
        group.bench_function("structural_churn/sky", |b| {
            b.iter(|| {
                sky_mixed_churn_step(&mut sky_world, &sky_churn_entities);
                black_box(&sky_world);
            });
        });
    }

    {
        let (mut hecs_world, _, hecs_churn_entities) = hecs_mixed_world();
        group.bench_function("structural_churn/hecs", |b| {
            b.iter(|| {
                hecs_mixed_churn_step(&mut hecs_world, &hecs_churn_entities);
                black_box(&hecs_world);
            });
        });
    }

    {
        let (mut bevy_world, _, bevy_churn_entities) = bevy_mixed_world();
        group.bench_function("structural_churn/bevy", |b| {
            b.iter(|| {
                bevy_mixed_churn_step(&mut bevy_world, &bevy_churn_entities);
                black_box(&bevy_world);
            });
        });
    }

    {
        let (mut sky_world, _, _) = sky_mixed_world();
        let mut sky_spawned_entities = Vec::with_capacity(MIXED_FRAME_SPAWN_COUNT);
        group.bench_function("spawn_despawn/sky", |b| {
            b.iter(|| {
                for _ in 0..MIXED_PHASE_SPAWN_REPEAT {
                    sky_mixed_spawn_step(&mut sky_world, &mut sky_spawned_entities);
                }
                black_box(&sky_world);
            });
        });
    }

    {
        let (mut hecs_world, _, _) = hecs_mixed_world();
        let mut hecs_spawned_entities = Vec::with_capacity(MIXED_FRAME_SPAWN_COUNT);
        group.bench_function("spawn_despawn/hecs", |b| {
            b.iter(|| {
                for _ in 0..MIXED_PHASE_SPAWN_REPEAT {
                    hecs_mixed_spawn_step(&mut hecs_world, &mut hecs_spawned_entities);
                }
                black_box(&hecs_world);
            });
        });
    }

    {
        let (mut bevy_world, _, _) = bevy_mixed_world();
        let mut bevy_spawned_entities = Vec::with_capacity(MIXED_FRAME_SPAWN_COUNT);
        group.bench_function("spawn_despawn/bevy", |b| {
            b.iter(|| {
                for _ in 0..MIXED_PHASE_SPAWN_REPEAT {
                    bevy_mixed_spawn_step(&mut bevy_world, &mut bevy_spawned_entities);
                }
                black_box(&bevy_world);
            });
        });
    }

    group.finish();
}

criterion_group!(
    fair_benches,
    bench_insert,
    bench_iteration,
    bench_fragmented_iteration,
    bench_heavy_compute,
    bench_random_access,
    bench_entity_ops,
    bench_mixed_frame,
    bench_mixed_frame_phases,
);
criterion_main!(fair_benches);
