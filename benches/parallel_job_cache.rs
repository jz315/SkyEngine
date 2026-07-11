#[path = "common.rs"]
mod common;

use common::{Position2D, Velocity2D};
use criterion::{criterion_group, criterion_main, Criterion};
use sky_engine::ecs::{PreparedQuery, World};
use std::hint::black_box;
use std::time::Duration;

const ENTITY_COUNT: usize = 100_000;

#[derive(Clone, Copy)]
struct Active;

fn populated_world() -> World {
    let mut world = World::new();
    world.spawn_batch((0..ENTITY_COUNT).map(|index| {
        (
            Position2D {
                x: index as f32,
                y: 0.0,
            },
            Velocity2D { x: 1.0, y: 0.5 },
            Active,
        )
    }));
    world
}

fn bench_parallel_job_cache(c: &mut Criterion) {
    let mut group = c.benchmark_group("parallel_job_cache");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(3));
    group.sample_size(30);

    let mut world = populated_world();
    let mut query = PreparedQuery::<(&Position2D, &Velocity2D)>::new();
    query.par_for_each_chunk(&mut world, |_| {});
    group.bench_function("rebuild_after_spawn_despawn_100k", |b| {
        b.iter(|| {
            let entity = world.spawn((
                Position2D { x: 0.0, y: 0.0 },
                Velocity2D { x: 1.0, y: 0.5 },
                Active,
            ));
            query.par_for_each_chunk(&mut world, |chunk| {
                black_box(chunk);
            });
            assert!(world.despawn(entity));
        });
    });
    group.finish();
}

criterion_group!(benches, bench_parallel_job_cache);
criterion_main!(benches);
