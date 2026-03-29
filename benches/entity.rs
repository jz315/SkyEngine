#[path = "common.rs"]
mod common;
use common::*;

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use hecs::World as HecsWorld;
use sky_engine::ecs::{Commands, World as SkyWorld};

fn bench_random_access(c: &mut Criterion) {
    let mut sky_world = SkyWorld::new();
    let sky_entities: Vec<_> = (0..SIMPLE_ENTITY_COUNT)
        .map(|_| sky_world.spawn((suite_position(), suite_velocity())))
        .collect();

    let mut hecs_world = HecsWorld::new();
    let hecs_entities: Vec<_> = (0..SIMPLE_ENTITY_COUNT)
        .map(|_| hecs_world.spawn((suite_position(), suite_velocity())))
        .collect();

    let mut group = c.benchmark_group("random_access");

    group.bench_function("sky_get", |b| {
        b.iter(|| {
            for &entity in &sky_entities {
                black_box(sky_world.get::<PositionComponent>(entity));
            }
        });
    });

    group.bench_function("hecs_get", |b| {
        b.iter(|| {
            for &entity in &hecs_entities {
                black_box(hecs_world.get::<&PositionComponent>(entity));
            }
        });
    });

    group.finish();
}

fn bench_spawn_despawn(c: &mut Criterion) {
    let mut group = c.benchmark_group("spawn_despawn");

    group.bench_function("sky", |b| {
        let mut world = SkyWorld::new();
        b.iter(|| {
            let entities: Vec<_> = (0..1_000)
                .map(|_| world.spawn((suite_position(), suite_velocity())))
                .collect();
            for entity in entities {
                world.despawn(entity);
            }
        });
    });

    group.bench_function("hecs", |b| {
        let mut world = HecsWorld::new();
        b.iter(|| {
            let entities: Vec<_> = (0..1_000)
                .map(|_| world.spawn((suite_position(), suite_velocity())))
                .collect();
            for entity in entities {
                world.despawn(entity).ok();
            }
        });
    });

    group.finish();
}

fn bench_add_remove_component(c: &mut Criterion) {
    let mut group = c.benchmark_group("add_remove_component");

    group.bench_function("sky", |b| {
        let mut world = SkyWorld::new();
        let entities: Vec<_> = (0..1_000)
            .map(|_| world.spawn((suite_position(), suite_velocity())))
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

    group.bench_function("hecs", |b| {
        let mut world = HecsWorld::new();
        let entities: Vec<_> = (0..1_000)
            .map(|_| world.spawn((suite_position(), suite_velocity())))
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

    group.finish();
}

fn bench_commands(c: &mut Criterion) {
    let mut group = c.benchmark_group("commands");

    group.bench_function("spawn_1k_deferred", |b| {
        let mut world = SkyWorld::new();
        b.iter(|| {
            let mut cmds = Commands::new();
            for _ in 0..1_000 {
                cmds.spawn((suite_position(), suite_velocity()));
            }
            cmds.apply(&mut world);
        });
    });

    group.bench_function("spawn_1k_direct", |b| {
        let mut world = SkyWorld::new();
        b.iter(|| {
            for _ in 0..1_000 {
                world.spawn((suite_position(), suite_velocity()));
            }
        });
    });

    group.finish();
}

criterion_group!(
    entity_benches,
    bench_random_access,
    bench_spawn_despawn,
    bench_add_remove_component,
    bench_commands,
);
criterion_main!(entity_benches);
