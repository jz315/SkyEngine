#[path = "common.rs"]
mod common;

use common::{AuxA, AuxB, Position2D, Velocity2D};
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use sky_engine::ecs::{EntityId, PreparedQuery, QueryData, World};
use std::hint::black_box;
use std::time::Duration;

const LIGHT_ENTITY_COUNTS: [usize; 3] = [100_000, 400_000, 2_000_000];
const HEAVY_ENTITY_COUNTS: [usize; 2] = [400_000, 2_000_000];
const REAL_SCENE_ENTITY_COUNTS: [usize; 2] = [250_000, 1_000_000];
const FACADE_ENTITY_COUNT: usize = 1_000_000;

const REAL_SCENE_WIDTH: f32 = 4096.0;
const REAL_SCENE_HEIGHT: f32 = 4096.0;
const REAL_SCENE_WALL_MARGIN: f32 = 160.0;
const REAL_SCENE_MAX_SPEED: f32 = 260.0;
const REAL_SCENE_MIN_SPEED: f32 = 35.0;
const REAL_SCENE_MAX_STEERING: f32 = 190.0;
const REAL_SCENE_DT: f32 = 1.0 / 60.0;

#[derive(Clone, Copy)]
struct SceneStatus {
    panic: f32,
    drag: f32,
    boost: f32,
}

#[derive(QueryData)]
struct Movement<'w> {
    position: &'w mut Position2D,
    velocity: &'w Velocity2D,
}

fn world_with_parallel_entities(count: usize) -> World {
    let mut world = World::new();
    world.spawn_batch((0..count).map(|i| {
        let base = i as f32;
        (
            Position2D {
                x: base * 0.25,
                y: base * 0.5,
            },
            Velocity2D { x: 1.0, y: 0.5 },
            AuxA {
                x: base * 0.1,
                y: 1.0,
            },
            AuxB {
                x: 0.25,
                y: base * 0.2,
            },
        )
    }));
    world
}

fn configure_group(group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>) {
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(2));
    group.sample_size(10);
}

#[inline(always)]
fn length_sq(x: f32, y: f32) -> f32 {
    x * x + y * y
}

#[inline(always)]
fn length(x: f32, y: f32) -> f32 {
    length_sq(x, y).sqrt()
}

#[inline(always)]
fn normalize_or_zero(x: f32, y: f32) -> (f32, f32) {
    let len_sq = length_sq(x, y);
    if len_sq > 1.0e-6 {
        let inv = len_sq.sqrt().recip();
        (x * inv, y * inv)
    } else {
        (0.0, 0.0)
    }
}

#[inline(always)]
fn limit_vector(x: f32, y: f32, max: f32) -> (f32, f32) {
    let len_sq = length_sq(x, y);
    if len_sq > max * max {
        let scale = max / len_sq.sqrt();
        (x * scale, y * scale)
    } else {
        (x, y)
    }
}

#[inline(always)]
fn heavy_kernel(position: &mut Position2D, velocity: &Velocity2D, aux_a: &AuxA, aux_b: &AuxB) {
    let mut x = position.x;
    let mut y = position.y;
    let mut vx = velocity.x + aux_a.x * 0.05 - aux_b.x * 0.03;
    let mut vy = velocity.y + aux_b.y * 0.02 - aux_a.y * 0.04;

    for _ in 0..8 {
        let len_sq = x.mul_add(x, y * y) + 1.0;
        let inv_len = len_sq.sqrt().recip();
        let steer_x = (aux_a.x - x) * 0.001 + aux_b.x * 0.0005;
        let steer_y = (aux_b.y - y) * 0.001 - aux_a.y * 0.0005;

        vx = (vx + steer_x).mul_add(0.975, inv_len * 0.125);
        vy = (vy + steer_y).mul_add(0.975, inv_len * 0.125);
        x = (x + vx).mul_add(0.999, aux_a.y * 0.0001);
        y = (y + vy).mul_add(0.999, aux_b.x * 0.0001);
    }

    position.x = x;
    position.y = y;
}

fn world_with_real_scene_entities(count: usize) -> (World, Vec<[f32; 2]>) {
    let mut world = World::new();
    let buffed_count = count / 4;
    let normal_count = count - buffed_count;

    world.spawn_batch((0..normal_count).map(|i| {
        let base = i as f32;
        (
            Position2D {
                x: (base * 0.37) % REAL_SCENE_WIDTH,
                y: (base * 0.61) % REAL_SCENE_HEIGHT,
            },
            Velocity2D {
                x: 25.0 + (base % 13.0),
                y: 12.0 + (base % 7.0),
            },
            AuxA {
                x: (base * 0.017).sin() * 220.0,
                y: (base * 0.011).cos() * 180.0,
            },
            AuxB {
                x: (base * 0.013).cos() * 140.0,
                y: (base * 0.019).sin() * 260.0,
            },
        )
    }));

    world.spawn_batch((0..buffed_count).map(|i| {
        let base = (normal_count + i) as f32;
        (
            Position2D {
                x: (base * 0.37) % REAL_SCENE_WIDTH,
                y: (base * 0.61) % REAL_SCENE_HEIGHT,
            },
            Velocity2D {
                x: 25.0 + (base % 13.0),
                y: 12.0 + (base % 7.0),
            },
            AuxA {
                x: (base * 0.017).sin() * 220.0,
                y: (base * 0.011).cos() * 180.0,
            },
            AuxB {
                x: (base * 0.013).cos() * 140.0,
                y: (base * 0.019).sin() * 260.0,
            },
            SceneStatus {
                panic: (base * 0.007).sin().abs(),
                drag: 0.10 + ((base as usize % 5) as f32) * 0.03,
                boost: 1.0 + ((base as usize % 7) as f32) * 0.04,
            },
        )
    }));

    let gusts = (0..count)
        .map(|i| {
            let base = i as f32;
            [
                (base * 0.0051).sin() * 24.0 + (base * 0.0007).cos() * 9.0,
                (base * 0.0043).cos() * 24.0 - (base * 0.0009).sin() * 9.0,
            ]
        })
        .collect();

    (world, gusts)
}

#[inline(always)]
#[allow(clippy::too_many_arguments)]
fn real_scene_kernel(
    position: &mut Position2D,
    velocity: &mut Velocity2D,
    aux_a: &AuxA,
    aux_b: &AuxB,
    status: Option<&SceneStatus>,
    gust: [f32; 2],
    attractor: [f32; 2],
    predator: [f32; 2],
) {
    let mut ax = gust[0] * 0.18;
    let mut ay = gust[1] * 0.18;

    let goal_x = attractor[0] + aux_a.x * 0.6 + aux_b.x * 0.35;
    let goal_y = attractor[1] + aux_a.y * 0.45 + aux_b.y * 0.25;
    let to_goal_x = goal_x - position.x;
    let to_goal_y = goal_y - position.y;
    let (goal_dx, goal_dy) = normalize_or_zero(to_goal_x, to_goal_y);
    ax += goal_dx * 90.0;
    ay += goal_dy * 90.0;

    let flee_x = position.x - predator[0];
    let flee_y = position.y - predator[1];
    let flee_dist_sq = length_sq(flee_x, flee_y);
    if flee_dist_sq < 900.0 * 900.0 {
        let strength = 1.0 - flee_dist_sq.sqrt() / 900.0;
        let (dx, dy) = normalize_or_zero(flee_x, flee_y);
        ax += dx * 230.0 * strength;
        ay += dy * 230.0 * strength;
    }

    let (wander_x, wander_y) =
        normalize_or_zero(aux_b.y - velocity.y * 0.35, aux_a.x - velocity.x * 0.35);
    ax += wander_x * 48.0;
    ay += wander_y * 48.0;

    let mut drag = 0.14;
    let mut max_speed = REAL_SCENE_MAX_SPEED;
    if let Some(status) = status {
        ax += goal_dx * 35.0 * status.boost;
        ay += goal_dy * 35.0 * status.boost;
        ax += wander_x * 60.0 * status.panic;
        ay += wander_y * 60.0 * status.panic;
        drag += status.drag;
        max_speed *= status.boost + status.panic * 0.2;
    }

    if position.x < REAL_SCENE_WALL_MARGIN {
        ax += (1.0 - position.x / REAL_SCENE_WALL_MARGIN) * 260.0;
    } else if position.x > REAL_SCENE_WIDTH - REAL_SCENE_WALL_MARGIN {
        ax -= (1.0 - (REAL_SCENE_WIDTH - position.x) / REAL_SCENE_WALL_MARGIN) * 260.0;
    }
    if position.y < REAL_SCENE_WALL_MARGIN {
        ay += (1.0 - position.y / REAL_SCENE_WALL_MARGIN) * 260.0;
    } else if position.y > REAL_SCENE_HEIGHT - REAL_SCENE_WALL_MARGIN {
        ay -= (1.0 - (REAL_SCENE_HEIGHT - position.y) / REAL_SCENE_WALL_MARGIN) * 260.0;
    }

    let (ax, ay) = limit_vector(ax, ay, REAL_SCENE_MAX_STEERING);
    velocity.x += ax * REAL_SCENE_DT;
    velocity.y += ay * REAL_SCENE_DT;

    let velocity_drag = (1.0 - drag * REAL_SCENE_DT).max(0.0);
    velocity.x *= velocity_drag;
    velocity.y *= velocity_drag;

    let speed = length(velocity.x, velocity.y);
    if speed > max_speed {
        velocity.x = velocity.x / speed * max_speed;
        velocity.y = velocity.y / speed * max_speed;
    } else if speed < REAL_SCENE_MIN_SPEED && speed > 1.0e-4 {
        velocity.x = velocity.x / speed * REAL_SCENE_MIN_SPEED;
        velocity.y = velocity.y / speed * REAL_SCENE_MIN_SPEED;
    }

    position.x = (position.x + velocity.x * REAL_SCENE_DT).clamp(0.0, REAL_SCENE_WIDTH - 1.0);
    position.y = (position.y + velocity.y * REAL_SCENE_DT).clamp(0.0, REAL_SCENE_HEIGHT - 1.0);
}

fn bench_chunk_update(c: &mut Criterion) {
    let mut group = c.benchmark_group("parallel_query_chunk_update");
    configure_group(&mut group);

    for entity_count in LIGHT_ENTITY_COUNTS {
        let mut world_seq = world_with_parallel_entities(entity_count);
        let mut world_par = world_with_parallel_entities(entity_count);
        let mut seq_query = PreparedQuery::<(&mut Position2D, &Velocity2D)>::new();
        let mut par_query = PreparedQuery::<(&mut Position2D, &Velocity2D)>::new();

        group.bench_with_input(
            BenchmarkId::new("sequential", entity_count),
            &entity_count,
            |b, &_count| {
                b.iter(|| {
                    seq_query.for_each_chunk(&mut world_seq, |(positions, velocities)| {
                        for index in 0..positions.len() {
                            positions[index].x += velocities[index].x;
                            positions[index].y += velocities[index].y;
                        }
                    });
                    black_box(&world_seq);
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("parallel", entity_count),
            &entity_count,
            |b, &_count| {
                b.iter(|| {
                    par_query.par_for_each_chunk(&mut world_par, |(positions, velocities)| {
                        for index in 0..positions.len() {
                            positions[index].x += velocities[index].x;
                            positions[index].y += velocities[index].y;
                        }
                    });
                    black_box(&world_par);
                });
            },
        );
    }

    group.finish();
}

fn bench_chunk_update_with_entities(c: &mut Criterion) {
    let mut group = c.benchmark_group("parallel_query_chunk_with_entities");
    configure_group(&mut group);

    for entity_count in LIGHT_ENTITY_COUNTS {
        let mut world_seq = world_with_parallel_entities(entity_count);
        let mut world_par = world_with_parallel_entities(entity_count);
        let mut seq_query = PreparedQuery::<(&mut Position2D, &Velocity2D)>::new();
        let mut par_query = PreparedQuery::<(&mut Position2D, &Velocity2D)>::new();

        group.bench_with_input(
            BenchmarkId::new("sequential", entity_count),
            &entity_count,
            |b, &_count| {
                b.iter(|| {
                    seq_query.for_each_chunk_with_entities(
                        &mut world_seq,
                        |entities: &[EntityId], (positions, velocities)| {
                            black_box(entities.first().map(|entity| entity.index()));
                            for index in 0..positions.len() {
                                positions[index].x += velocities[index].x;
                                positions[index].y += velocities[index].y;
                            }
                        },
                    );
                    black_box(&world_seq);
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("parallel", entity_count),
            &entity_count,
            |b, &_count| {
                b.iter(|| {
                    par_query.par_for_each_chunk_with_entities(
                        &mut world_par,
                        |entities: &[EntityId], (positions, velocities)| {
                            black_box(entities.first().map(|entity| entity.index()));
                            for index in 0..positions.len() {
                                positions[index].x += velocities[index].x;
                                positions[index].y += velocities[index].y;
                            }
                        },
                    );
                    black_box(&world_par);
                });
            },
        );
    }

    group.finish();
}

fn bench_chunk_update_heavy(c: &mut Criterion) {
    let mut group = c.benchmark_group("parallel_query_chunk_update_heavy");
    configure_group(&mut group);

    for entity_count in HEAVY_ENTITY_COUNTS {
        let mut world_seq = world_with_parallel_entities(entity_count);
        let mut world_par = world_with_parallel_entities(entity_count);
        let mut seq_query = PreparedQuery::<(&mut Position2D, &Velocity2D, &AuxA, &AuxB)>::new();
        let mut par_query = PreparedQuery::<(&mut Position2D, &Velocity2D, &AuxA, &AuxB)>::new();

        group.bench_with_input(
            BenchmarkId::new("sequential", entity_count),
            &entity_count,
            |b, &_count| {
                b.iter(|| {
                    seq_query.for_each_chunk(
                        &mut world_seq,
                        |(positions, velocities, aux_a, aux_b)| {
                            for index in 0..positions.len() {
                                heavy_kernel(
                                    &mut positions[index],
                                    &velocities[index],
                                    &aux_a[index],
                                    &aux_b[index],
                                );
                            }
                        },
                    );
                    black_box(&world_seq);
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("parallel", entity_count),
            &entity_count,
            |b, &_count| {
                b.iter(|| {
                    par_query.par_for_each_chunk(
                        &mut world_par,
                        |(positions, velocities, aux_a, aux_b)| {
                            for index in 0..positions.len() {
                                heavy_kernel(
                                    &mut positions[index],
                                    &velocities[index],
                                    &aux_a[index],
                                    &aux_b[index],
                                );
                            }
                        },
                    );
                    black_box(&world_par);
                });
            },
        );
    }

    group.finish();
}

fn bench_real_scene(c: &mut Criterion) {
    let mut group = c.benchmark_group("parallel_query_real_scene");
    configure_group(&mut group);

    for entity_count in REAL_SCENE_ENTITY_COUNTS {
        let (mut world_seq, gusts_seq) = world_with_real_scene_entities(entity_count);
        let (mut world_par, gusts_par) = world_with_real_scene_entities(entity_count);
        let mut seq_query = PreparedQuery::<(
            &mut Position2D,
            &mut Velocity2D,
            &AuxA,
            &AuxB,
            Option<&SceneStatus>,
        )>::new();
        let mut par_query = PreparedQuery::<(
            &mut Position2D,
            &mut Velocity2D,
            &AuxA,
            &AuxB,
            Option<&SceneStatus>,
        )>::new();
        let attractor = [REAL_SCENE_WIDTH * 0.62, REAL_SCENE_HEIGHT * 0.41];
        let predator = [REAL_SCENE_WIDTH * 0.28, REAL_SCENE_HEIGHT * 0.67];

        group.bench_with_input(
            BenchmarkId::new("sequential", entity_count),
            &entity_count,
            |b, &_count| {
                b.iter(|| {
                    seq_query.for_each_chunk_with_entities(
                        &mut world_seq,
                        |entities, (positions, velocities, aux_a, aux_b, statuses)| {
                            for index in 0..positions.len() {
                                let status = statuses.map(|slice| &slice[index]);
                                let gust = gusts_seq[entities[index].index() as usize];
                                real_scene_kernel(
                                    &mut positions[index],
                                    &mut velocities[index],
                                    &aux_a[index],
                                    &aux_b[index],
                                    status,
                                    gust,
                                    attractor,
                                    predator,
                                );
                            }
                        },
                    );
                    black_box(&world_seq);
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("parallel", entity_count),
            &entity_count,
            |b, &_count| {
                b.iter(|| {
                    par_query.par_for_each_chunk_with_entities(
                        &mut world_par,
                        |entities, (positions, velocities, aux_a, aux_b, statuses)| {
                            for index in 0..positions.len() {
                                let status = statuses.map(|slice| &slice[index]);
                                let gust = gusts_par[entities[index].index() as usize];
                                real_scene_kernel(
                                    &mut positions[index],
                                    &mut velocities[index],
                                    &aux_a[index],
                                    &aux_b[index],
                                    status,
                                    gust,
                                    attractor,
                                    predator,
                                );
                            }
                        },
                    );
                    black_box(&world_par);
                });
            },
        );
    }

    group.finish();
}

#[inline(always)]
fn facade_update(position: &mut Position2D, velocity: &Velocity2D) {
    position.x = black_box(position.x + velocity.x * 0.000_001);
    position.y = black_box(position.y + velocity.y * 0.000_001);
}

fn bench_bound_facade(c: &mut Criterion) {
    let mut group = c.benchmark_group("parallel_query_bound_facade");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(3));
    group.sample_size(30);
    group.throughput(Throughput::Elements(FACADE_ENTITY_COUNT as u64));

    let mut sequential_world = world_with_parallel_entities(FACADE_ENTITY_COUNT);
    group.bench_function("tuple_sequential", |b| {
        b.iter(|| {
            sequential_world
                .query_mut::<(&mut Position2D, &Velocity2D)>()
                .for_each(|(position, velocity)| facade_update(position, velocity));
        });
    });

    let mut tuple_world = world_with_parallel_entities(FACADE_ENTITY_COUNT);
    group.bench_function("tuple_parallel", |b| {
        b.iter(|| {
            tuple_world
                .query_mut::<(&mut Position2D, &Velocity2D)>()
                .par_for_each(|(position, velocity)| facade_update(position, velocity));
        });
    });

    let mut named_world = world_with_parallel_entities(FACADE_ENTITY_COUNT);
    group.bench_function("named_parallel", |b| {
        b.iter(|| {
            named_world
                .query_mut::<Movement>()
                .par_for_each(|item| facade_update(item.position, item.velocity));
        });
    });

    let mut chunk_world = world_with_parallel_entities(FACADE_ENTITY_COUNT);
    group.bench_function("tuple_parallel_chunk", |b| {
        b.iter(|| {
            chunk_world
                .query_mut::<(&mut Position2D, &Velocity2D)>()
                .par_for_each_chunk(|(positions, velocities)| {
                    for (position, velocity) in positions.iter_mut().zip(velocities) {
                        facade_update(position, velocity);
                    }
                });
        });
    });
    group.finish();
}

criterion_group!(
    parallel_query_benches,
    bench_chunk_update,
    bench_chunk_update_with_entities,
    bench_chunk_update_heavy,
    bench_real_scene,
    bench_bound_facade
);
criterion_main!(parallel_query_benches);
