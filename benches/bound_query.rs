#[path = "common.rs"]
mod common;

use common::{Position2D, Velocity2D};
use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use sky_engine::ecs::dynamic::{DynamicBundle, WorldDynamicExt};
use sky_engine::ecs::{Any, PreparedQuery, QueryData, With, World};
use std::hint::black_box;
use std::time::Duration;

const ENTITY_COUNT: usize = 100_000;
const PARALLEL_ENTITY_COUNT: usize = 1_000_000;

#[derive(Clone, Copy)]
struct Active;

#[derive(Clone, Copy)]
struct Selected;

#[derive(Clone, Copy)]
struct MatchA;
#[derive(Clone, Copy)]
struct MatchB;
#[derive(Clone, Copy)]
struct MatchC;
#[derive(Clone, Copy)]
struct MatchD;
#[derive(Clone, Copy)]
struct MatchE;
#[derive(Clone, Copy)]
struct MatchF;
#[derive(Clone, Copy)]
struct MatchG;
#[derive(Clone, Copy)]
struct MatchH;
#[derive(Clone, Copy)]
struct ShapeI;
#[derive(Clone, Copy)]
struct ShapeJ;
#[derive(Clone, Copy)]
struct ShapeK;
#[derive(Clone, Copy)]
struct ShapeL;
#[derive(Clone, Copy)]
struct ShapeM;
#[derive(Clone, Copy)]
struct ShapeN;
#[derive(Clone, Copy)]
struct ShapeO;
#[derive(Clone, Copy)]
struct ShapeP;
#[derive(Clone, Copy, Default)]
struct WideExtra1;
#[derive(Clone, Copy, Default)]
struct WideExtra2;
#[derive(Clone, Copy, Default)]
struct WideExtra3;
#[derive(Clone, Copy, Default)]
struct WideExtra4;
#[derive(Clone, Copy, Default)]
struct WideExtra5;
#[derive(Clone, Copy, Default)]
struct WideExtra6;
#[derive(Clone, Copy, Default)]
struct WideExtra7;
#[derive(Clone, Copy, Default)]
struct WideExtra8;

#[derive(QueryData)]
#[allow(dead_code)]
struct WideMatch<'w> {
    a: &'w MatchA,
    b: &'w MatchB,
    c: &'w MatchC,
    d: &'w MatchD,
    e: &'w MatchE,
    component_f: &'w MatchF,
    g: &'w MatchG,
    h: &'w MatchH,
    i: &'w ShapeI,
    j: &'w ShapeJ,
    k: &'w ShapeK,
    l: &'w ShapeL,
    m: &'w ShapeM,
    n: &'w ShapeN,
    o: &'w ShapeO,
    p: &'w ShapeP,
}

#[derive(QueryData)]
struct Movement<'w> {
    position: &'w mut Position2D,
    velocity: &'w Velocity2D,
}

fn populated_world_with_count(entity_count: usize) -> World {
    let mut world = World::new();
    world.spawn_batch((0..entity_count / 2).map(|index| {
        (
            Position2D {
                x: index as f32,
                y: 0.0,
            },
            Velocity2D { x: 1.0, y: 0.5 },
            Active,
        )
    }));
    world.spawn_batch((entity_count / 2..entity_count).map(|index| {
        (
            Position2D {
                x: index as f32,
                y: 0.0,
            },
            Velocity2D { x: 1.0, y: 0.5 },
            Selected,
        )
    }));
    world
}

fn populated_world() -> World {
    populated_world_with_count(ENTITY_COUNT)
}

fn archetype_match_world() -> World {
    let mut world = World::new();
    world.spawn((MatchA,));
    world.spawn((MatchA, MatchB));
    world.spawn((MatchA, MatchB, MatchC));
    world.spawn((MatchA, MatchB, MatchC, MatchD));
    world.spawn((MatchA, MatchB, MatchC, MatchD, MatchE));
    world.spawn((MatchA, MatchB, MatchC, MatchD, MatchE, MatchF));
    world.spawn((MatchA, MatchB, MatchC, MatchD, MatchE, MatchF, MatchG));
    world.spawn((
        MatchA, MatchB, MatchC, MatchD, MatchE, MatchF, MatchG, MatchH,
    ));
    world
}

fn dense_archetype_match_world() -> World {
    let mut world = World::new();
    world.spawn((
        MatchA, MatchB, MatchC, MatchD, MatchE, MatchF, MatchG, MatchH,
    ));
    world.spawn((
        MatchA, MatchB, MatchC, MatchD, MatchE, MatchF, MatchG, ShapeI,
    ));
    world.spawn((
        MatchA, MatchB, MatchC, MatchD, MatchE, MatchF, MatchG, ShapeJ,
    ));
    world.spawn((
        MatchA, MatchB, MatchC, MatchD, MatchE, MatchF, MatchG, ShapeK,
    ));
    world.spawn((
        MatchA, MatchB, MatchC, MatchD, MatchE, MatchF, MatchG, ShapeL,
    ));
    world.spawn((
        MatchA, MatchB, MatchC, MatchD, MatchE, MatchF, MatchG, ShapeM,
    ));
    world.spawn((
        MatchA, MatchB, MatchC, MatchD, MatchE, MatchF, MatchG, ShapeN,
    ));
    world.spawn((
        MatchA, MatchB, MatchC, MatchD, MatchE, MatchF, MatchG, ShapeO,
    ));
    world.spawn((
        MatchA, MatchB, MatchC, MatchD, MatchE, MatchF, MatchG, ShapeP,
    ));
    world
}

fn wide_archetype_match_world() -> World {
    fn spawn_shape<T: Default + 'static>(world: &mut World) {
        world
            .spawn_dynamic(
                DynamicBundle::new()
                    .with(MatchA)
                    .with(MatchB)
                    .with(MatchC)
                    .with(MatchD)
                    .with(MatchE)
                    .with(MatchF)
                    .with(MatchG)
                    .with(MatchH)
                    .with(ShapeI)
                    .with(ShapeJ)
                    .with(ShapeK)
                    .with(ShapeL)
                    .with(ShapeM)
                    .with(ShapeN)
                    .with(ShapeO)
                    .with(ShapeP)
                    .with(T::default()),
            )
            .unwrap();
    }

    let mut world = World::new();
    spawn_shape::<WideExtra1>(&mut world);
    spawn_shape::<WideExtra2>(&mut world);
    spawn_shape::<WideExtra3>(&mut world);
    spawn_shape::<WideExtra4>(&mut world);
    spawn_shape::<WideExtra5>(&mut world);
    spawn_shape::<WideExtra6>(&mut world);
    spawn_shape::<WideExtra7>(&mut world);
    spawn_shape::<WideExtra8>(&mut world);
    world
}

fn update(position: &mut Position2D, velocity: &Velocity2D) {
    position.x = black_box(position.x + velocity.x * 0.000_001);
    position.y = black_box(position.y + velocity.y * 0.000_001);
}

fn bench_bound_query(c: &mut Criterion) {
    let mut group = c.benchmark_group("bound_query");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(2));
    group.sample_size(20);

    let cache_world = populated_world();
    let _ = cache_world
        .query::<(&Position2D, &Velocity2D)>()
        .filter::<Any<(With<Active>, With<Selected>)>>()
        .cached_archetype_count();
    group.bench_function("world_cache_hit", |b| {
        b.iter(|| {
            black_box(
                cache_world
                    .query::<(&Position2D, &Velocity2D)>()
                    .filter::<Any<(With<Active>, With<Selected>)>>()
                    .cached_archetype_count(),
            )
        });
    });

    let mut bound_world = populated_world();
    group.bench_function("bound_tuple_for_each", |b| {
        b.iter(|| {
            bound_world
                .query_mut::<(&mut Position2D, &Velocity2D)>()
                .for_each(|(position, velocity)| update(position, velocity));
        });
    });

    let mut named_world = populated_world();
    group.bench_function("bound_named_for_each", |b| {
        b.iter(|| {
            named_world
                .query_mut::<Movement>()
                .for_each(|item| update(item.position, item.velocity));
        });
    });

    let mut prepared_world = populated_world();
    let mut prepared = PreparedQuery::<(&mut Position2D, &Velocity2D)>::new();
    group.bench_function("prepared_tuple_for_each", |b| {
        b.iter(|| {
            prepared.for_each(&mut prepared_world, |(position, velocity)| {
                update(position, velocity);
            });
        });
    });

    group.finish();
}

fn bench_parallel_query(c: &mut Criterion) {
    let mut group = c.benchmark_group("parallel_query");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(3));
    group.sample_size(30);
    group.throughput(Throughput::Elements(PARALLEL_ENTITY_COUNT as u64));

    let mut sequential_world = populated_world_with_count(PARALLEL_ENTITY_COUNT);
    group.bench_function("bound_tuple_for_each_sequential", |b| {
        b.iter(|| {
            sequential_world
                .query_mut::<(&mut Position2D, &Velocity2D)>()
                .for_each(|(position, velocity)| update(position, velocity));
        });
    });

    let mut tuple_world = populated_world_with_count(PARALLEL_ENTITY_COUNT);
    group.bench_function("bound_tuple_par_for_each", |b| {
        b.iter(|| {
            tuple_world
                .query_mut::<(&mut Position2D, &Velocity2D)>()
                .par_for_each(|(position, velocity)| update(position, velocity));
        });
    });

    let mut named_world = populated_world_with_count(PARALLEL_ENTITY_COUNT);
    group.bench_function("bound_named_par_for_each", |b| {
        b.iter(|| {
            named_world
                .query_mut::<Movement>()
                .par_for_each(|item| update(item.position, item.velocity));
        });
    });

    let mut chunk_world = populated_world_with_count(PARALLEL_ENTITY_COUNT);
    group.bench_function("bound_tuple_par_for_each_chunk", |b| {
        b.iter(|| {
            chunk_world
                .query_mut::<(&mut Position2D, &Velocity2D)>()
                .par_for_each_chunk(|(positions, velocities)| {
                    for (position, velocity) in positions.iter_mut().zip(velocities) {
                        update(position, velocity);
                    }
                });
        });
    });

    group.finish();
}

fn bench_archetype_match(c: &mut Criterion) {
    let mut group = c.benchmark_group("archetype_match");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(3));
    group.sample_size(50);

    let world = archetype_match_world();
    group.bench_function("fresh_query_1_of_8_shapes", |b| {
        b.iter(|| {
            let mut query = PreparedQuery::<&MatchA>::new();
            black_box(query.count(&world));
        });
    });
    group.bench_function("fresh_query_8_of_8_shapes", |b| {
        b.iter(|| {
            let mut query = PreparedQuery::<(
                &MatchH,
                &MatchB,
                &MatchF,
                &MatchA,
                &MatchG,
                &MatchC,
                &MatchE,
                &MatchD,
            )>::new();
            black_box(query.count(&world));
        });
    });

    let dense_world = dense_archetype_match_world();
    group.bench_function("fresh_query_7_dense_matches", |b| {
        b.iter(|| {
            let mut query = PreparedQuery::<(
                &MatchB,
                &MatchF,
                &MatchA,
                &MatchG,
                &MatchC,
                &MatchE,
                &MatchD,
            )>::new();
            black_box(query.count(&dense_world));
        });
    });
    group.bench_function("fresh_query_7_redundant_with", |b| {
        b.iter(|| {
            let mut query = PreparedQuery::<
                (
                    &MatchB,
                    &MatchF,
                    &MatchA,
                    &MatchG,
                    &MatchC,
                    &MatchE,
                    &MatchD,
                ),
                With<MatchA>,
            >::new();
            black_box(query.count(&dense_world));
        });
    });
    group.bench_function("fresh_query_7_redundant_with_tuple", |b| {
        b.iter(|| {
            let mut query = PreparedQuery::<
                (
                    &MatchB,
                    &MatchF,
                    &MatchA,
                    &MatchG,
                    &MatchC,
                    &MatchE,
                    &MatchD,
                ),
                (
                    With<MatchA>,
                    With<MatchB>,
                    With<MatchC>,
                    With<MatchD>,
                    With<MatchE>,
                    With<MatchF>,
                    With<MatchG>,
                ),
            >::new();
            black_box(query.count(&dense_world));
        });
    });
    group.bench_function("fresh_query_7_selective_with", |b| {
        b.iter(|| {
            let mut query = PreparedQuery::<
                (
                    &MatchB,
                    &MatchF,
                    &MatchA,
                    &MatchG,
                    &MatchC,
                    &MatchE,
                    &MatchD,
                ),
                With<MatchH>,
            >::new();
            black_box(query.count(&dense_world));
        });
    });

    let wide_world = wide_archetype_match_world();
    group.bench_function("fresh_query_16_dense_matches", |b| {
        b.iter(|| {
            let mut query = PreparedQuery::<WideMatch>::new();
            black_box(query.count(&wide_world));
        });
    });

    group.finish();
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

criterion_group!(
    benches,
    bench_bound_query,
    bench_parallel_query,
    bench_archetype_match,
    bench_parallel_job_cache
);
criterion_main!(benches);
