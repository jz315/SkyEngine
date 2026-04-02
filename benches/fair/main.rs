#[path = "../common.rs"]
mod common;

mod bevy;
mod hecs;
mod shared;
mod sky;

use criterion::{criterion_group, criterion_main, Criterion};

fn bench_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("fair_insert");
    sky::bench_insert(&mut group);
    hecs::bench_insert(&mut group);
    bevy::bench_insert(&mut group);
    group.finish();
}

fn bench_iteration(c: &mut Criterion) {
    let mut group = c.benchmark_group("fair_iteration");
    sky::bench_iteration(&mut group);
    hecs::bench_iteration(&mut group);
    bevy::bench_iteration(&mut group);
    group.finish();
}

fn bench_fragmented_iteration(c: &mut Criterion) {
    let mut group = c.benchmark_group("fair_fragmented_iteration");
    sky::bench_fragmented_iteration(&mut group);
    hecs::bench_fragmented_iteration(&mut group);
    bevy::bench_fragmented_iteration(&mut group);
    group.finish();
}

fn bench_heavy_compute(c: &mut Criterion) {
    let mut group = c.benchmark_group("fair_heavy_compute");
    sky::bench_heavy_compute(&mut group);
    hecs::bench_heavy_compute(&mut group);
    bevy::bench_heavy_compute(&mut group);
    group.finish();
}

fn bench_random_access(c: &mut Criterion) {
    let mut group = c.benchmark_group("fair_random_access");
    sky::bench_random_access(&mut group);
    hecs::bench_random_access(&mut group);
    bevy::bench_random_access(&mut group);
    group.finish();
}

fn bench_entity_ops(c: &mut Criterion) {
    let mut group = c.benchmark_group("fair_entity_ops");
    sky::bench_entity_ops(&mut group);
    hecs::bench_entity_ops(&mut group);
    bevy::bench_entity_ops(&mut group);
    group.finish();
}

fn bench_mixed_frame(c: &mut Criterion) {
    let mut group = c.benchmark_group("fair_mixed_frame");
    sky::bench_mixed_frame(&mut group);
    hecs::bench_mixed_frame(&mut group);
    bevy::bench_mixed_frame(&mut group);
    group.finish();
}

fn bench_mixed_frame_phases(c: &mut Criterion) {
    let mut group = c.benchmark_group("fair_mixed_frame_phases");
    sky::bench_mixed_frame_phases(&mut group);
    hecs::bench_mixed_frame_phases(&mut group);
    bevy::bench_mixed_frame_phases(&mut group);
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
