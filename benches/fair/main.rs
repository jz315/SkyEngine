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

use bevy_ecs::world::World as BevyWorld;
use cgmath::{SquareMatrix, Transform as _};
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use hecs::{PreparedQuery as HecsPreparedQuery, World as HecsWorld};
use sky_engine::ecs::{raw::PreparedQuery as SkyPreparedQuery, World as SkyWorld};

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
        });
    });

    group.bench_function("simple/hecs", |b| {
        b.iter(|| {
            for (_, (pos, vel)) in hecs_query.query(&hecs_world).iter() {
                pos.0 += vel.0;
            }
        });
    });

    group.bench_function("simple/bevy", |b| {
        b.iter(|| {
            for (mut pos, vel) in bevy_query.iter_mut(&mut bevy_world) {
                pos.0 += vel.0;
            }
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
        });
    });

    group.bench_function("fragmented/hecs", |b| {
        b.iter(|| {
            for (_, data) in hecs_query.query(&hecs_world).iter() {
                data.0 *= 2.0;
            }
        });
    });

    group.bench_function("fragmented/bevy", |b| {
        b.iter(|| {
            for mut data in bevy_query.iter_mut(&mut bevy_world) {
                data.0 *= 2.0;
            }
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
        });
    });

    group.finish();
}

fn bench_random_access(c: &mut Criterion) {
    let mut sky_world = SkyWorld::new();
    let sky_entities: Vec<_> = (0..SIMPLE_ENTITY_COUNT)
        .map(|_| sky_world.spawn(light_bundle()))
        .collect();

    let mut hecs_world = HecsWorld::new();
    let hecs_entities: Vec<_> = (0..SIMPLE_ENTITY_COUNT)
        .map(|_| hecs_world.spawn(light_bundle()))
        .collect();

    let mut bevy_world = BevyWorld::new();
    let bevy_entities: Vec<_> = (0..SIMPLE_ENTITY_COUNT)
        .map(|_| bevy_world.spawn(light_bundle()).id())
        .collect();

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
        });
    });

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
);
criterion_main!(fair_benches);
