// Bevy ECS benchmarks — engine-side regression/reference suite.

#[path = "../common.rs"]
mod common;
use common::*;

use cgmath::{SquareMatrix, Transform as _};
use criterion::{criterion_group, criterion_main, Criterion};
use std::hint::black_box;

fn world_with_entities(n: usize) -> bevy_ecs::world::World {
    let mut world = bevy_ecs::world::World::new();
    world.spawn_batch((0..n).map(|_| suite_bundle()));
    world
}

// ===========================================================================
//  Insert
// ===========================================================================

fn bench_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("bevy_insert");

    group.bench_function("batch_10k", |b| {
        b.iter(|| {
            let mut world = bevy_ecs::world::World::new();
            world.spawn_batch((0..SIMPLE_ENTITY_COUNT).map(|_| suite_bundle()));
            black_box(&world);
        });
    });

    group.bench_function("single_10k", |b| {
        b.iter(|| {
            let mut world = bevy_ecs::world::World::new();
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
    let mut world = world_with_entities(SIMPLE_ENTITY_COUNT);
    let mut query = world.query::<(&mut PositionComponent, &VelocityComponent)>();

    c.bench_function("bevy_simple_iter", |b| {
        b.iter(|| {
            for (mut pos, vel) in query.iter_mut(&mut world) {
                pos.0 += vel.0;
            }
        });
    });
}

fn bench_fragmented_iter(c: &mut Criterion) {
    let mut world = bevy_ecs::world::World::new();
    macro_rules! bevy_variant {
        ($world:ident; $($tag:ident),*) => {
            $( for _ in 0..FRAGMENTED_ENTITIES_PER_VARIANT { $world.spawn(($tag(0.0), DataComponent(1.0))); } )*
        };
    }
    bevy_variant!(world; A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y, Z);
    let mut query = world.query::<&mut DataComponent>();

    c.bench_function("bevy_fragmented_iter", |b| {
        b.iter(|| {
            for mut data in query.iter_mut(&mut world) {
                data.0 *= 2.0;
            }
        });
    });
}

fn bench_heavy_compute(c: &mut Criterion) {
    let mut world = bevy_ecs::world::World::new();
    world.spawn_batch((0..HEAVY_ENTITY_COUNT).map(|_| heavy_bundle()));
    let mut query = world.query::<(&mut PositionComponent, &mut TransformComponent)>();

    c.bench_function("bevy_heavy_compute", |b| {
        b.iter(|| {
            for (mut position, mut transform) in query.iter_mut(&mut world) {
                for _ in 0..HEAVY_INVERT_COUNT {
                    transform.0 = transform.0.invert().unwrap();
                }
                position.0 = transform.0.transform_vector(position.0);
            }
        });
    });
}

fn bench_random_access(c: &mut Criterion) {
    let mut world = bevy_ecs::world::World::new();
    let entities: Vec<_> = (0..SIMPLE_ENTITY_COUNT)
        .map(|_| world.spawn(light_bundle()).id())
        .collect();

    c.bench_function("bevy_random_access", |b| {
        b.iter(|| {
            for &entity in &entities {
                black_box(world.get::<PositionComponent>(entity));
            }
        });
    });
}

fn bench_spawn_despawn(c: &mut Criterion) {
    c.bench_function("bevy_spawn_despawn_1k", |b| {
        let mut world = bevy_ecs::world::World::new();
        b.iter(|| {
            let entities: Vec<_> = (0..ENTITY_OP_COUNT)
                .map(|_| world.spawn(light_bundle()).id())
                .collect();
            for entity in entities {
                world.despawn(entity);
            }
        });
    });
}

fn bench_add_remove_component(c: &mut Criterion) {
    let mut world = bevy_ecs::world::World::new();
    let entities: Vec<_> = (0..ENTITY_OP_COUNT)
        .map(|_| world.spawn(light_bundle()).id())
        .collect();

    c.bench_function("bevy_add_remove_component_1k", |b| {
        b.iter(|| {
            for &entity in &entities {
                world.entity_mut(entity).insert(Health(100.0));
            }
            for &entity in &entities {
                world.entity_mut(entity).remove::<Health>();
            }
        });
    });
}

// ===========================================================================
//  Main
// ===========================================================================

criterion_group!(
    bevy_benches,
    bench_insert,
    bench_simple_iter,
    bench_fragmented_iter,
    bench_heavy_compute,
    bench_random_access,
    bench_spawn_despawn,
    bench_add_remove_component,
);
criterion_main!(bevy_benches);
