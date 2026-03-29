use bevy_ecs::prelude::*;
use cgmath::{Matrix4, Rad, SquareMatrix, Transform as _, Vector3};
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use hecs::World as HecsWorld;
use sky_engine::ecs::{
    raw::PreparedQuery as SkyPreparedQuery, Commands, With as SkyWith, Without as SkyWithout,
    World as SkyWorld,
};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const SIMPLE_ENTITY_COUNT: usize = 10_000;
const FRAGMENTED_VARIANT_COUNT: usize = 26;
const FRAGMENTED_ENTITIES_PER_VARIANT: usize = 20;
const HEAVY_ENTITY_COUNT: usize = 1_000;
const HEAVY_INVERT_COUNT: usize = 100;

// ---------------------------------------------------------------------------
// Components
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Component)]
struct TransformComponent(Matrix4<f32>);

#[derive(Clone, Copy, Component)]
struct PositionComponent(Vector3<f32>);

#[derive(Clone, Copy, Component)]
struct RotationComponent(Vector3<f32>);

#[derive(Clone, Copy, Component)]
struct VelocityComponent(Vector3<f32>);

#[derive(Clone, Copy, Component)]
struct DataComponent(f32);

#[derive(Clone, Copy, Default, Component)]
struct Health(f32);

#[derive(Clone, Copy, Default, Component)]
struct Damage(f32);

#[derive(Clone, Copy, Default, Component)]
struct IsEnemy;

#[derive(Clone, Copy, Default, Component)]
struct IsAlly;

macro_rules! define_fragment_tags {
    ($($name:ident),+ $(,)?) => {
        $(
            #[derive(Clone, Copy, Default, Component)]
            struct $name(f32);
        )+
    };
}

define_fragment_tags!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y, Z);

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn suite_transform() -> TransformComponent {
    TransformComponent(Matrix4::from_scale(1.0))
}

fn suite_position() -> PositionComponent {
    PositionComponent(Vector3::unit_x())
}

fn suite_rotation() -> RotationComponent {
    RotationComponent(Vector3::unit_x())
}

fn suite_velocity() -> VelocityComponent {
    VelocityComponent(Vector3::unit_x())
}

fn heavy_matrix() -> Matrix4<f32> {
    Matrix4::<f32>::from_angle_x(Rad(1.2))
}

fn sky_world_with_entities(n: usize) -> SkyWorld {
    let mut world = SkyWorld::new();
    world.spawn_batch((0..n).map(|_| {
        (
            suite_transform(),
            suite_position(),
            suite_rotation(),
            suite_velocity(),
        )
    }));
    world
}

fn hecs_world_with_entities(n: usize) -> HecsWorld {
    let mut world = HecsWorld::new();
    world.spawn_batch((0..n).map(|_| {
        (
            suite_transform(),
            suite_position(),
            suite_rotation(),
            suite_velocity(),
        )
    }));
    world
}

// ===========================================================================
// 1. INSERT BENCHMARKS
// ===========================================================================

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

// ===========================================================================
// 2. ITERATION BENCHMARKS
// ===========================================================================

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
            for (_, (vel, pos)) in hecs_world.query_mut::<(&VelocityComponent, &mut PositionComponent)>()
            {
                pos.0 += vel.0;
            }
        });
    });

    group.bench_function("bevy", |b| {
        b.iter(|| {
            let mut query = bevy_world.query::<(&mut PositionComponent, &VelocityComponent)>();
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

// ===========================================================================
// 3. ENTITY SCALING
// ===========================================================================

fn bench_iter_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("iter_scaling");

    for count in [1_000, 10_000, 100_000] {
        let mut world = sky_world_with_entities(count);
        let mut query =
            SkyPreparedQuery::<(&mut PositionComponent, &VelocityComponent)>::new();

        group.bench_with_input(
            BenchmarkId::new("sky", count),
            &count,
            |b, _| {
                b.iter(|| {
                    query.for_each(&world, |(pos, vel)| {
                        pos.0 += vel.0;
                    });
                });
            },
        );
    }

    group.finish();
}

// ===========================================================================
// 4. RANDOM ACCESS
// ===========================================================================

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

// ===========================================================================
// 5. SPAWN + DESPAWN CHURN
// ===========================================================================

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

// ===========================================================================
// 6. FILTERED QUERY
// ===========================================================================

fn bench_filtered_query(c: &mut Criterion) {
    let mut world = SkyWorld::new();

    // Half enemies, half allies, all have Position + Velocity
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

// ===========================================================================
// 7. HEAVY COMPUTE
// ===========================================================================

fn bench_heavy_compute(c: &mut Criterion) {
    let mut sky_world = SkyWorld::new();
    for _ in 0..HEAVY_ENTITY_COUNT {
        sky_world.spawn((heavy_matrix(), suite_position(), suite_rotation(), suite_velocity()));
    }
    let mut sky_query = SkyPreparedQuery::<(
        &mut PositionComponent,
        &mut Matrix4<f32>,
    )>::new();

    let mut hecs_world = HecsWorld::default();
    hecs_world.spawn_batch((0..HEAVY_ENTITY_COUNT).map(|_| {
        (heavy_matrix(), suite_position(), suite_rotation(), suite_velocity())
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

// ===========================================================================
// 8. ADD / REMOVE COMPONENT (ARCHETYPE MIGRATION)
// ===========================================================================

fn bench_add_remove_component(c: &mut Criterion) {
    let mut group = c.benchmark_group("add_remove_component");

    group.bench_function("sky", |b| {
        let mut world = SkyWorld::new();
        let entities: Vec<_> = (0..1_000)
            .map(|_| world.spawn((suite_position(), suite_velocity())))
            .collect();

        b.iter(|| {
            // Add Health then remove it
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

// ===========================================================================
// 9. COMMANDS (DEFERRED OPS)
// ===========================================================================

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

// ===========================================================================

criterion_group!(
    suite_benches,
    bench_batch_insert,
    bench_single_insert,
    bench_simple_iter,
    bench_fragmented_iter,
    bench_iter_scaling,
    bench_random_access,
    bench_spawn_despawn,
    bench_filtered_query,
    bench_heavy_compute,
    bench_add_remove_component,
    bench_commands,
);
criterion_main!(suite_benches);
