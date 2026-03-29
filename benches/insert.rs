#[path = "common.rs"]
mod common;
use common::*;

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use hecs::World as HecsWorld;
use sky_engine::ecs::World as SkyWorld;

fn bench_batch_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("batch_insert");

    group.bench_function("sky", |b| {
        b.iter(|| {
            let mut world = SkyWorld::new();
            world.spawn_batch((0..SIMPLE_ENTITY_COUNT).map(|_| {
                (
                    suite_transform(),
                    suite_position(),
                    suite_rotation(),
                    suite_velocity(),
                )
            }));
            black_box(world);
        });
    });

    group.bench_function("hecs", |b| {
        b.iter(|| {
            let mut world = HecsWorld::new();
            world.spawn_batch((0..SIMPLE_ENTITY_COUNT).map(|_| {
                (
                    suite_transform(),
                    suite_position(),
                    suite_rotation(),
                    suite_velocity(),
                )
            }));
            black_box(&world);
        });
    });

    group.bench_function("bevy", |b| {
        b.iter(|| {
            let mut world = bevy_ecs::world::World::new();
            world.spawn_batch((0..SIMPLE_ENTITY_COUNT).map(|_| {
                (
                    suite_transform(),
                    suite_position(),
                    suite_rotation(),
                    suite_velocity(),
                )
            }));
            black_box(&world);
        });
    });

    group.finish();
}

fn bench_single_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("single_insert");

    group.bench_function("sky", |b| {
        b.iter(|| {
            let mut world = SkyWorld::new();
            for _ in 0..SIMPLE_ENTITY_COUNT {
                world.spawn((
                    suite_transform(),
                    suite_position(),
                    suite_rotation(),
                    suite_velocity(),
                ));
            }
            black_box(world);
        });
    });

    group.bench_function("hecs", |b| {
        b.iter(|| {
            let mut world = HecsWorld::new();
            for _ in 0..SIMPLE_ENTITY_COUNT {
                world.spawn((
                    suite_transform(),
                    suite_position(),
                    suite_rotation(),
                    suite_velocity(),
                ));
            }
            black_box(&world);
        });
    });

    group.finish();
}

criterion_group!(insert_benches, bench_batch_insert, bench_single_insert);
criterion_main!(insert_benches);
