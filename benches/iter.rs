#[path = "common.rs"]
mod common;
use common::*;

use cgmath::{Matrix4, SquareMatrix, Transform as _};
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use hecs::World as HecsWorld;
use sky_engine::ecs::{
    raw::PreparedQuery as SkyPreparedQuery, With as SkyWith, World as SkyWorld,
};

fn bench_simple_iter(c: &mut Criterion) {
    let mut sky_world = sky_world_with_entities(SIMPLE_ENTITY_COUNT);
    let mut sky_query =
        SkyPreparedQuery::<(&mut PositionComponent, &VelocityComponent)>::new();

    let mut hecs_world = hecs_world_with_entities(SIMPLE_ENTITY_COUNT);

    let mut bevy_world = bevy_ecs::world::World::new();
    bevy_world.spawn_batch((0..SIMPLE_ENTITY_COUNT).map(|_| {
        (
            suite_transform(),
            suite_position(),
            suite_rotation(),
            suite_velocity(),
        )
    }));

    let mut group = c.benchmark_group("simple_iter");

    group.bench_function("sky_for_each", |b| {
        b.iter(|| {
            sky_query.for_each(&sky_world, |(pos, vel)| {
                pos.0 += vel.0;
            });
        });
    });

    group.bench_function("sky_chunk", |b| {
        b.iter(|| {
            sky_query.for_each_chunk(&sky_world, |(positions, velocities)| {
                for (pos, vel) in positions.iter_mut().zip(velocities.iter()) {
                    pos.0 += vel.0;
                }
            });
        });
    });

    group.bench_function("hecs", |b| {
        b.iter(|| {
            for (_, (vel, pos)) in
                hecs_world.query_mut::<(&VelocityComponent, &mut PositionComponent)>()
            {
                pos.0 += vel.0;
            }
        });
    });

    group.bench_function("bevy", |b| {
        b.iter(|| {
            let mut query =
                bevy_world.query::<(&mut PositionComponent, &VelocityComponent)>();
            for (mut pos, vel) in query.iter_mut(&mut bevy_world) {
                pos.0 += vel.0;
            }
        });
    });

    group.finish();
}

fn bench_fragmented_iter(c: &mut Criterion) {
    debug_assert_eq!(FRAGMENTED_VARIANT_COUNT, 26);

    let mut sky_world = SkyWorld::new();
    macro_rules! add_variant {
        ($tag:ty) => {{
            for _ in 0..FRAGMENTED_ENTITIES_PER_VARIANT {
                sky_world.spawn((<$tag>::default(), DataComponent(1.0)));
            }
        }};
    }
    add_variant!(A); add_variant!(B); add_variant!(C); add_variant!(D);
    add_variant!(E); add_variant!(F); add_variant!(G); add_variant!(H);
    add_variant!(I); add_variant!(J); add_variant!(K); add_variant!(L);
    add_variant!(M); add_variant!(N); add_variant!(O); add_variant!(P);
    add_variant!(Q); add_variant!(R); add_variant!(S); add_variant!(T);
    add_variant!(U); add_variant!(V); add_variant!(W); add_variant!(X);
    add_variant!(Y); add_variant!(Z);

    let mut sky_query = SkyPreparedQuery::<&mut DataComponent>::new();

    let mut hecs_world = HecsWorld::default();
    macro_rules! hecs_variant {
        ($world:ident; $($tag:ident),*) => {
            $( $world.spawn_batch((0..FRAGMENTED_ENTITIES_PER_VARIANT).map(|_| ($tag(0.0), DataComponent(1.0)))); )*
        };
    }
    hecs_variant!(hecs_world; A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y, Z);

    let mut bevy_world = bevy_ecs::world::World::new();
    macro_rules! bevy_variant {
        ($world:ident; $($tag:ident),*) => {
            $( for _ in 0..FRAGMENTED_ENTITIES_PER_VARIANT { $world.spawn(($tag(0.0), DataComponent(1.0))); } )*
        };
    }
    bevy_variant!(bevy_world; A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y, Z);

    let mut group = c.benchmark_group("fragmented_iter");

    group.bench_function("sky", |b| {
        b.iter(|| {
            sky_query.for_each(&sky_world, |data| {
                data.0 *= 2.0;
            });
        });
    });

    group.bench_function("hecs", |b| {
        b.iter(|| {
            for (_, data) in hecs_world.query_mut::<&mut DataComponent>() {
                data.0 *= 2.0;
            }
        });
    });

    group.bench_function("bevy", |b| {
        b.iter(|| {
            let mut query = bevy_world.query::<&mut DataComponent>();
            for mut data in query.iter_mut(&mut bevy_world) {
                data.0 *= 2.0;
            }
        });
    });

    group.finish();
}

fn bench_iter_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("iter_scaling");

    for count in [1_000, 10_000, 100_000] {
        let mut world = sky_world_with_entities(count);
        let mut query =
            SkyPreparedQuery::<(&mut PositionComponent, &VelocityComponent)>::new();

        group.bench_with_input(BenchmarkId::new("sky", count), &count, |b, _| {
            b.iter(|| {
                query.for_each(&world, |(pos, vel)| {
                    pos.0 += vel.0;
                });
            });
        });
    }

    group.finish();
}

fn bench_filtered_query(c: &mut Criterion) {
    let mut world = SkyWorld::new();

    for i in 0..SIMPLE_ENTITY_COUNT {
        if i % 2 == 0 {
            world.spawn((suite_position(), suite_velocity(), IsEnemy));
        } else {
            world.spawn((suite_position(), suite_velocity(), IsAlly));
        }
    }

    let mut query_all =
        SkyPreparedQuery::<(&mut PositionComponent, &VelocityComponent)>::new();
    let mut query_filtered = SkyPreparedQuery::<
        (&mut PositionComponent, &VelocityComponent),
        SkyWith<IsEnemy>,
    >::new();

    let mut group = c.benchmark_group("filtered_query");

    group.bench_function("all_10k", |b| {
        b.iter(|| {
            query_all.for_each(&world, |(pos, vel)| {
                pos.0 += vel.0;
            });
        });
    });

    group.bench_function("with_enemy_5k", |b| {
        b.iter(|| {
            query_filtered.for_each(&world, |(pos, vel)| {
                pos.0 += vel.0;
            });
        });
    });

    group.finish();
}

fn bench_heavy_compute(c: &mut Criterion) {
    let mut sky_world = SkyWorld::new();
    for _ in 0..HEAVY_ENTITY_COUNT {
        sky_world.spawn((
            heavy_matrix(),
            suite_position(),
            suite_rotation(),
            suite_velocity(),
        ));
    }
    let mut sky_query =
        SkyPreparedQuery::<(&mut PositionComponent, &mut Matrix4<f32>)>::new();

    let mut hecs_world = HecsWorld::default();
    hecs_world.spawn_batch((0..HEAVY_ENTITY_COUNT).map(|_| {
        (
            heavy_matrix(),
            suite_position(),
            suite_rotation(),
            suite_velocity(),
        )
    }));

    let mut group = c.benchmark_group("heavy_compute");

    group.bench_function("sky", |b| {
        b.iter(|| {
            sky_query.for_each_chunk(&sky_world, |(positions, matrices)| {
                for (position, matrix) in positions.iter_mut().zip(matrices.iter_mut()) {
                    for _ in 0..HEAVY_INVERT_COUNT {
                        *matrix = matrix.invert().unwrap();
                    }
                    position.0 = matrix.transform_vector(position.0);
                }
            });
        });
    });

    group.bench_function("hecs", |b| {
        b.iter(|| {
            for (_, (position, matrix)) in
                hecs_world.query_mut::<(&mut PositionComponent, &mut Matrix4<f32>)>()
            {
                for _ in 0..HEAVY_INVERT_COUNT {
                    *matrix = matrix.invert().unwrap();
                }
                position.0 = matrix.transform_vector(position.0);
            }
        });
    });

    group.finish();
}

criterion_group!(
    iter_benches,
    bench_simple_iter,
    bench_fragmented_iter,
    bench_iter_scaling,
    bench_filtered_query,
    bench_heavy_compute,
);
criterion_main!(iter_benches);
