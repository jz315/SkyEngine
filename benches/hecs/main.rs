// hecs benchmarks — engine-side regression/reference suite.

#[path = "../common.rs"]
mod common;
use common::*;

use cgmath::{SquareMatrix, Transform as _};
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use hecs::{PreparedQuery, World};

// ===========================================================================
//  Helpers
// ===========================================================================

fn world_with_entities(n: usize) -> World {
    let mut world = World::new();
    world.spawn_batch((0..n).map(|_| suite_bundle()));
    world
}

// ===========================================================================
//  Hot-path: hecs_2_of_4 / hecs_4_of_4
// ===========================================================================

fn bench_hot_path(c: &mut Criterion) {
    let mut world = World::new();
    for _ in 0..HOT_PATH_ENTITY_COUNT {
        world.spawn(hot_path_bundle());
    }

    let mut query2 = PreparedQuery::<(&mut Position2D, &Velocity2D)>::default();
    let mut query4 = PreparedQuery::<(&mut Position2D, &Velocity2D, &mut AuxA, &AuxB)>::default();

    let mut group = c.benchmark_group("hecs_hot_path");

    group.bench_function("2_of_4", |b| {
        b.iter(|| {
            for (_, (p, v)) in query2.query(&world).iter() {
                p.x += v.x * HOT_PATH_DELTA;
                p.y += v.y * HOT_PATH_DELTA;
            }
        })
    });

    group.bench_function("4_of_4", |b| {
        b.iter(|| {
            for (_, (p, v, a, aux_b)) in query4.query(&world).iter() {
                p.x += v.x * HOT_PATH_DELTA + aux_b.x * HOT_PATH_DELTA;
                p.y += v.y * HOT_PATH_DELTA + aux_b.y * HOT_PATH_DELTA;
                a.x += v.x;
                a.y += aux_b.y;
            }
        })
    });

    group.finish();
}

// ===========================================================================
//  Insert
// ===========================================================================

fn bench_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("hecs_insert");

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
    let mut query = PreparedQuery::<(&mut PositionComponent, &VelocityComponent)>::default();

    c.bench_function("hecs_simple_iter", |b| {
        b.iter(|| {
            for (_, (pos, vel)) in query.query(&world).iter() {
                pos.0 += vel.0;
            }
        });
    });
}

fn bench_fragmented_iter(c: &mut Criterion) {
    let world = {
        let mut world = World::default();
        macro_rules! hecs_variant {
            ($world:ident; $($tag:ident),*) => {
                $( $world.spawn_batch((0..FRAGMENTED_ENTITIES_PER_VARIANT).map(|_| ($tag(0.0), DataComponent(1.0)))); )*
            };
        }
        hecs_variant!(world; A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y, Z);
        world
    };
    let mut query = PreparedQuery::<&mut DataComponent>::default();

    c.bench_function("hecs_fragmented_iter", |b| {
        b.iter(|| {
            for (_, data) in query.query(&world).iter() {
                data.0 *= 2.0;
            }
        });
    });
}

fn bench_heavy_compute(c: &mut Criterion) {
    let mut world = World::default();
    world.spawn_batch((0..HEAVY_ENTITY_COUNT).map(|_| heavy_bundle()));
    let mut query = PreparedQuery::<(&mut PositionComponent, &mut TransformComponent)>::default();

    c.bench_function("hecs_heavy_compute", |b| {
        b.iter(|| {
            for (_, (position, transform)) in query.query(&world).iter() {
                for _ in 0..HEAVY_INVERT_COUNT {
                    transform.0 = transform.0.invert().unwrap();
                }
                position.0 = transform.0.transform_vector(position.0);
            }
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

    c.bench_function("hecs_random_access", |b| {
        b.iter(|| {
            for &entity in &entities {
                let _ = black_box(world.get::<&PositionComponent>(entity));
            }
        });
    });
}

fn bench_spawn_despawn(c: &mut Criterion) {
    c.bench_function("hecs_spawn_despawn_1k", |b| {
        let mut world = World::new();
        b.iter(|| {
            let entities: Vec<_> = (0..ENTITY_OP_COUNT)
                .map(|_| world.spawn(light_bundle()))
                .collect();
            for entity in entities {
                world.despawn(entity).ok();
            }
        });
    });
}

fn bench_add_remove_component(c: &mut Criterion) {
    let mut world = World::new();
    let entities: Vec<_> = (0..ENTITY_OP_COUNT)
        .map(|_| world.spawn(light_bundle()))
        .collect();

    c.bench_function("hecs_add_remove_component_1k", |b| {
        b.iter(|| {
            for &entity in &entities {
                world.insert_one(entity, Health(100.0)).ok();
            }
            for &entity in &entities {
                world.remove_one::<Health>(entity).ok();
            }
        });
    });
}

// ===========================================================================
//  Main
// ===========================================================================

criterion_group!(
    hecs_benches,
    bench_hot_path,
    bench_insert,
    bench_simple_iter,
    bench_fragmented_iter,
    bench_heavy_compute,
    bench_random_access,
    bench_spawn_despawn,
    bench_add_remove_component,
);
criterion_main!(hecs_benches);
