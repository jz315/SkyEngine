use criterion::{criterion_group, criterion_main, BatchSize, Criterion, Throughput};
use sky_ecs::ecs::__private::QuerySpec;
use sky_engine::ecs::dynamic::{DynamicBundle, WorldDynamicExt};
use sky_engine::ecs::{Any, PreparedQuery, QueryData, QueryFilter, With, Without, World};
use std::hint::black_box;
use std::time::Duration;

const REFRESH_BATCH: usize = 32;

macro_rules! markers {
    ($($name:ident),+ $(,)?) => {
        $(#[derive(Clone, Copy, Default)] struct $name;)+
    };
}

markers!(
    MatchA,
    MatchB,
    MatchC,
    MatchD,
    MatchE,
    MatchF,
    MatchG,
    MatchH,
    ShapeI,
    ShapeJ,
    ShapeK,
    ShapeL,
    ShapeM,
    ShapeN,
    ShapeO,
    ShapeP,
    WideExtra1,
    WideExtra2,
    WideExtra3,
    WideExtra4,
    WideExtra5,
    WideExtra6,
    WideExtra7,
    WideExtra8,
    RefreshMarker,
    NonMatchingMarker,
);

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

type DenseQuery = (
    &'static MatchB,
    &'static MatchF,
    &'static MatchA,
    &'static MatchG,
    &'static MatchC,
    &'static MatchE,
    &'static MatchD,
);

fn configure(group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>) {
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(3));
    group.sample_size(50);
}

fn staggered_world() -> World {
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

fn dense_world() -> World {
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

fn wide_world() -> World {
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

fn prepare_batch<Q, F>(world: &World) -> Vec<PreparedQuery<Q, F>>
where
    Q: QuerySpec,
    F: QueryFilter,
{
    (0..REFRESH_BATCH)
        .map(|_| {
            let mut query = PreparedQuery::<Q, F>::new();
            black_box(query.count(world));
            query
        })
        .collect()
}

fn count_batch<Q, F>(queries: &mut [PreparedQuery<Q, F>], world: &World) -> usize
where
    Q: QuerySpec,
    F: QueryFilter,
{
    queries.iter_mut().map(|query| query.count(world)).sum()
}

fn bench_fresh(c: &mut Criterion) {
    let mut group = c.benchmark_group("archetype_fresh");
    configure(&mut group);

    let staggered = staggered_world();
    group.bench_function("query_1_staggered_8", |b| {
        b.iter(|| {
            let mut query = PreparedQuery::<&MatchA>::new();
            black_box(query.count(&staggered));
        });
    });
    group.bench_function("query_2_staggered_8", |b| {
        b.iter(|| {
            let mut query = PreparedQuery::<(&MatchA, &MatchB)>::new();
            black_box(query.count(&staggered));
        });
    });
    group.bench_function("query_8_early_reject", |b| {
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
            black_box(query.count(&staggered));
        });
    });
    group.bench_function("query_optional_missing", |b| {
        b.iter(|| {
            let mut query = PreparedQuery::<(&MatchA, Option<&MatchH>)>::new();
            black_box(query.count(&staggered));
        });
    });

    let dense = dense_world();
    group.bench_function("query_7_dense_9", |b| {
        b.iter(|| {
            let mut query = PreparedQuery::<DenseQuery>::new();
            black_box(query.count(&dense));
        });
    });

    let wide = wide_world();
    group.bench_function("query_16_dense_8", |b| {
        b.iter(|| {
            let mut query = PreparedQuery::<WideMatch>::new();
            black_box(query.count(&wide));
        });
    });
    group.finish();
}

fn bench_cache(c: &mut Criterion) {
    let mut group = c.benchmark_group("archetype_cache");
    configure(&mut group);

    let world = dense_world();
    let mut query = PreparedQuery::<DenseQuery>::new();
    black_box(query.count(&world));
    group.bench_function("prepared_epoch_hit", |b| {
        b.iter(|| black_box(query.count(&world)));
    });
    group.finish();
}

fn bench_refresh(c: &mut Criterion) {
    let mut group = c.benchmark_group("archetype_refresh");
    configure(&mut group);
    group.throughput(Throughput::Elements(REFRESH_BATCH as u64));

    group.bench_function("append_matching_one", |b| {
        b.iter_batched(
            || {
                let mut world = World::new();
                world.spawn((MatchA,));
                let queries = prepare_batch::<(&MatchA, &MatchB), ()>(&world);
                world.spawn((MatchA, MatchB, RefreshMarker));
                (world, queries)
            },
            |(world, mut queries)| black_box(count_batch(&mut queries, &world)),
            BatchSize::SmallInput,
        );
    });

    group.bench_function("append_nonmatching_one", |b| {
        b.iter_batched(
            || {
                let mut world = World::new();
                world.spawn((MatchA, MatchB));
                let queries = prepare_batch::<(&MatchA, &MatchB), ()>(&world);
                world.spawn((MatchA, NonMatchingMarker));
                (world, queries)
            },
            |(world, mut queries)| black_box(count_batch(&mut queries, &world)),
            BatchSize::SmallInput,
        );
    });

    group.bench_function("rebuild_after_clear", |b| {
        b.iter_batched(
            || {
                let mut world = dense_world();
                let queries = prepare_batch::<DenseQuery, ()>(&world);
                world.clear();
                world.spawn((MatchA, MatchB, MatchC, MatchD, MatchE, MatchF, MatchG));
                (world, queries)
            },
            |(world, mut queries)| black_box(count_batch(&mut queries, &world)),
            BatchSize::SmallInput,
        );
    });

    group.bench_function("switch_world_same_epoch", |b| {
        b.iter_batched(
            || {
                let mut source = World::new();
                source.spawn((MatchA, MatchB));
                let queries = prepare_batch::<(&MatchA, &MatchB), ()>(&source);
                let mut target = World::new();
                target.spawn((MatchA,));
                (target, queries)
            },
            |(world, mut queries)| black_box(count_batch(&mut queries, &world)),
            BatchSize::SmallInput,
        );
    });
    group.finish();
}

fn bench_filters(c: &mut Criterion) {
    let mut group = c.benchmark_group("archetype_filter");
    configure(&mut group);
    let world = dense_world();

    group.bench_function("single_selective_with", |b| {
        b.iter(|| {
            let mut query = PreparedQuery::<DenseQuery, With<MatchH>>::new();
            black_box(query.count(&world));
        });
    });
    group.bench_function("single_without", |b| {
        b.iter(|| {
            let mut query = PreparedQuery::<DenseQuery, Without<MatchH>>::new();
            black_box(query.count(&world));
        });
    });
    group.bench_function("and_unique", |b| {
        b.iter(|| {
            let mut query =
                PreparedQuery::<&MatchA, (With<MatchB>, With<MatchC>, Without<ShapeP>)>::new();
            black_box(query.count(&world));
        });
    });
    group.bench_function("and_redundant_7", |b| {
        b.iter(|| {
            let mut query = PreparedQuery::<
                DenseQuery,
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
            black_box(query.count(&world));
        });
    });
    group.bench_function("and_contradiction", |b| {
        b.iter(|| {
            let mut query = PreparedQuery::<&MatchA, (With<MatchH>, Without<MatchH>)>::new();
            black_box(query.count(&world));
        });
    });
    group.bench_function("any_fallback", |b| {
        b.iter(|| {
            let mut query = PreparedQuery::<&MatchA, Any<(With<MatchH>, With<ShapeI>)>>::new();
            black_box(query.count(&world));
        });
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_fresh,
    bench_cache,
    bench_refresh,
    bench_filters
);
criterion_main!(benches);
