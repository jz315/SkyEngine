#[path = "common.rs"]
mod common;

use common::{Position2D, Velocity2D};
use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use sky_engine::ecs::{ParView, ResMut, Update, View, World};
use std::hint::black_box;
use std::time::Duration;

const ENTITY_COUNT: usize = 100_000;
const PAR_ENTITY_COUNT: usize = 1_000_000;
const WORK_ITEMS: usize = 32_768;

#[derive(Default)]
struct Counter(u64);

fn count_a(mut counter: ResMut<Counter>) {
    counter.0 = black_box(counter.0.wrapping_add(1));
}

fn count_b(mut counter: ResMut<Counter>) {
    counter.0 = black_box(counter.0.wrapping_add(1));
}

fn count_c(mut counter: ResMut<Counter>) {
    counter.0 = black_box(counter.0.wrapping_add(1));
}

fn tiny_a() {
    black_box(());
}

fn tiny_b() {
    black_box(());
}

fn integrate(entities: View<(&mut Position2D, &Velocity2D)>) {
    entities.for_each(|(position, velocity)| {
        position.x = black_box(position.x + velocity.x);
        position.y = black_box(position.y + velocity.y);
    });
}

fn integrate_parallel(entities: ParView<(&mut Position2D, &Velocity2D)>) {
    entities.par_for_each(|(position, velocity)| {
        position.x = black_box(position.x + velocity.x);
        position.y = black_box(position.y + velocity.y);
    });
}

struct WorkA(Vec<u64>);
struct WorkB(Vec<u64>);
struct WorkC(Vec<u64>);
struct WorkD(Vec<u64>);

macro_rules! work_system {
    ($name:ident, $resource:ty) => {
        fn $name(mut work: ResMut<$resource>) {
            for value in &mut work.0 {
                *value = black_box(value.wrapping_mul(1_664_525).wrapping_add(1_013_904_223));
            }
        }
    };
}

work_system!(work_a, WorkA);
work_system!(work_b, WorkB);
work_system!(work_c, WorkC);
work_system!(work_d, WorkD);

fn bench_schedule(c: &mut Criterion) {
    let mut group = c.benchmark_group("system_schedule");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(2));
    group.sample_size(20);

    let mut empty = World::new();
    group.bench_function("empty_tick", |b| {
        b.iter(|| black_box(empty.tick_with_delta(0.0).unwrap()));
    });

    let mut conflict_chain = World::new();
    conflict_chain.insert_resource(Counter::default());
    conflict_chain
        .stage(Update)
        .add(count_a)
        .add(count_b)
        .add(count_c);
    group.bench_function("three_conflicting_systems", |b| {
        b.iter(|| black_box(conflict_chain.tick_with_delta(0.0).unwrap()));
    });

    let mut tiny_wave = World::new();
    tiny_wave.stage(Update).add(tiny_a).add(tiny_b);
    group.bench_function("two_tiny_compatible_systems", |b| {
        b.iter(|| black_box(tiny_wave.tick_with_delta(0.0).unwrap()));
    });

    let mut parallel_wave = World::new();
    parallel_wave.insert_resource(WorkA(vec![1; WORK_ITEMS]));
    parallel_wave.insert_resource(WorkB(vec![2; WORK_ITEMS]));
    parallel_wave.insert_resource(WorkC(vec![3; WORK_ITEMS]));
    parallel_wave.insert_resource(WorkD(vec![4; WORK_ITEMS]));
    parallel_wave
        .stage(Update)
        .add(work_a)
        .add(work_b)
        .add(work_c)
        .add(work_d);
    group.throughput(Throughput::Elements((WORK_ITEMS * 4) as u64));
    group.bench_function("four_system_parallel_wave", |b| {
        b.iter(|| black_box(parallel_wave.tick_with_delta(0.0).unwrap()));
    });

    let mut component_world = World::new();
    component_world.spawn_batch((0..ENTITY_COUNT).map(|index| {
        (
            Position2D {
                x: index as f32,
                y: 0.0,
            },
            Velocity2D { x: 1.0, y: 0.5 },
        )
    }));
    component_world.stage(Update).add(integrate);
    group.throughput(Throughput::Elements(ENTITY_COUNT as u64));
    group.bench_function("typed_view_for_each_100k", |b| {
        b.iter(|| black_box(component_world.tick_with_delta(0.0).unwrap()));
    });

    let mut parallel_component_world = World::new();
    parallel_component_world.spawn_batch((0..PAR_ENTITY_COUNT).map(|index| {
        (
            Position2D {
                x: index as f32,
                y: 0.0,
            },
            Velocity2D { x: 1.0, y: 0.5 },
        )
    }));
    parallel_component_world
        .stage(Update)
        .add(integrate_parallel);
    group.throughput(Throughput::Elements(PAR_ENTITY_COUNT as u64));
    group.bench_function("typed_par_view_for_each_1m", |b| {
        b.iter(|| black_box(parallel_component_world.tick_with_delta(0.0).unwrap()));
    });

    group.finish();
}

criterion_group!(benches, bench_schedule);
criterion_main!(benches);
