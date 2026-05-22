// Shared components, constants, and helpers for all bench files.
// Include via: #[path = "../common.rs"] mod common; use common::*;

#![allow(dead_code)]

use cgmath::{Matrix4, Rad, Vector3};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

pub const SIMPLE_ENTITY_COUNT: usize = 10_000;
pub const REPEATED_ITERATION_COUNT: usize = 32;
pub const LARGE_ITERATION_ENTITY_COUNT: usize = 100_000;
pub const FRAGMENTED_VARIANT_COUNT: usize = 26;
pub const FRAGMENTED_ENTITIES_PER_VARIANT: usize = 400;
pub const HEAVY_ENTITY_COUNT: usize = 1_000;
pub const HEAVY_INVERT_COUNT: usize = 100;
pub const ENTITY_OP_COUNT: usize = 1_000;
pub const MIXED_FRAME_MOVERS: usize = 16_000;
pub const MIXED_FRAME_ENEMIES: usize = 4_000;
pub const MIXED_FRAME_ALLIES: usize = 4_000;
pub const MIXED_FRAME_HEAVY: usize = 1_000;
pub const MIXED_FRAME_RANDOM_COUNT: usize = 512;
pub const MIXED_FRAME_CHURN_COUNT: usize = 256;
pub const MIXED_FRAME_SPAWN_COUNT: usize = 64;
pub const MIXED_FRAME_INVERT_COUNT: usize = 8;
pub const MIXED_PHASE_HEALTH_REPEAT: usize = 8;
pub const MIXED_PHASE_SPAWN_REPEAT: usize = 32;

/// Entity count for system schedule benchmarks.
pub const SCHEDULE_ENTITY_COUNT: usize = 10_000;
/// System counts for scaling tests.
pub const SCHEDULE_SYSTEM_COUNTS: [usize; 3] = [1, 4, 16];

/// Entity count for the head-to-head hot-path benchmarks (sky vs hecs).
pub const HOT_PATH_ENTITY_COUNT: usize = 5_000_000;
pub const HOT_PATH_DELTA: f32 = 0.1;

// ---------------------------------------------------------------------------
// Components — used across all engines
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, bevy_ecs::prelude::Component, flecs_ecs::prelude::Component)]
pub struct TransformComponent(pub Matrix4<f32>);

#[derive(Clone, Copy, bevy_ecs::prelude::Component, flecs_ecs::prelude::Component)]
pub struct PositionComponent(pub Vector3<f32>);

#[derive(Clone, Copy, bevy_ecs::prelude::Component, flecs_ecs::prelude::Component)]
pub struct RotationComponent(pub Vector3<f32>);

#[derive(Clone, Copy, bevy_ecs::prelude::Component, flecs_ecs::prelude::Component)]
pub struct VelocityComponent(pub Vector3<f32>);

#[derive(Clone, Copy, bevy_ecs::prelude::Component, flecs_ecs::prelude::Component)]
pub struct DataComponent(pub f32);

#[derive(Clone, Copy, Default, bevy_ecs::prelude::Component, flecs_ecs::prelude::Component)]
pub struct Health(pub f32);

#[derive(Clone, Copy, Default, bevy_ecs::prelude::Component, flecs_ecs::prelude::Component)]
pub struct Damage(pub f32);

#[derive(Clone, Copy, Default, bevy_ecs::prelude::Component, flecs_ecs::prelude::Component)]
pub struct Regen(pub f32);

#[derive(Clone, Copy, Default, bevy_ecs::prelude::Component, flecs_ecs::prelude::Component)]
pub struct IsEnemy;

#[derive(Clone, Copy, Default, bevy_ecs::prelude::Component, flecs_ecs::prelude::Component)]
pub struct IsAlly;

/// Lightweight 2-field components for the hot-path head-to-head benchmarks.
#[derive(Clone, Copy, bevy_ecs::prelude::Component, flecs_ecs::prelude::Component)]
pub struct Position2D {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, bevy_ecs::prelude::Component, flecs_ecs::prelude::Component)]
pub struct Velocity2D {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, bevy_ecs::prelude::Component, flecs_ecs::prelude::Component)]
pub struct AuxA {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, bevy_ecs::prelude::Component, flecs_ecs::prelude::Component)]
pub struct AuxB {
    pub x: f32,
    pub y: f32,
}

macro_rules! define_fragment_tags {
    ($($name:ident),+ $(,)?) => {
        $(
            #[derive(
                Clone,
                Copy,
                Default,
                bevy_ecs::prelude::Component,
                flecs_ecs::prelude::Component,
            )]
            pub struct $name(pub f32);
        )+
    };
}

define_fragment_tags!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y, Z);

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

pub fn suite_transform() -> TransformComponent {
    TransformComponent(Matrix4::from_scale(1.0))
}

pub fn suite_position() -> PositionComponent {
    PositionComponent(Vector3::unit_x())
}

pub fn suite_rotation() -> RotationComponent {
    RotationComponent(Vector3::unit_x())
}

pub fn suite_velocity() -> VelocityComponent {
    VelocityComponent(Vector3::unit_x())
}

pub fn heavy_matrix() -> Matrix4<f32> {
    Matrix4::<f32>::from_angle_x(Rad(1.2))
}

pub fn suite_bundle() -> (
    TransformComponent,
    PositionComponent,
    RotationComponent,
    VelocityComponent,
) {
    (
        suite_transform(),
        suite_position(),
        suite_rotation(),
        suite_velocity(),
    )
}

pub fn light_bundle() -> (PositionComponent, VelocityComponent) {
    (suite_position(), suite_velocity())
}

pub fn heavy_bundle() -> (
    TransformComponent,
    PositionComponent,
    RotationComponent,
    VelocityComponent,
) {
    (
        TransformComponent(heavy_matrix()),
        suite_position(),
        suite_rotation(),
        suite_velocity(),
    )
}

pub fn hot_path_bundle() -> (Velocity2D, Position2D, AuxA, AuxB) {
    (
        Velocity2D { x: 1.0, y: 1.0 },
        Position2D { x: 0.0, y: 0.0 },
        AuxA { x: 0.0, y: 0.0 },
        AuxB { x: 1.0, y: 1.0 },
    )
}

pub fn mixed_mover_bundle() -> (PositionComponent, VelocityComponent) {
    (
        PositionComponent(Vector3::new(0.0, 1.0, 0.0)),
        VelocityComponent(Vector3::new(1.0, 0.5, 0.25)),
    )
}

pub fn mixed_enemy_bundle() -> (
    PositionComponent,
    VelocityComponent,
    Health,
    Damage,
    IsEnemy,
) {
    (
        PositionComponent(Vector3::new(2.0, 0.0, 0.0)),
        VelocityComponent(Vector3::new(0.25, 1.0, 0.0)),
        Health(100.0),
        Damage(0.75),
        IsEnemy,
    )
}

pub fn mixed_ally_bundle() -> (PositionComponent, VelocityComponent, Health, Regen, IsAlly) {
    (
        PositionComponent(Vector3::new(-2.0, 0.0, 0.0)),
        VelocityComponent(Vector3::new(0.0, 0.75, 0.25)),
        Health(60.0),
        Regen(0.35),
        IsAlly,
    )
}

pub fn mixed_heavy_bundle() -> (TransformComponent, PositionComponent, VelocityComponent) {
    (
        TransformComponent(heavy_matrix()),
        PositionComponent(Vector3::new(1.0, 0.0, 0.0)),
        VelocityComponent(Vector3::new(0.5, 0.0, 0.5)),
    )
}

/// Deterministic Fisher-Yates shuffle using xorshift64.
/// Fixed seed so benchmark runs are reproducible without external deps.
pub fn deterministic_shuffle<T>(slice: &mut [T]) {
    let mut state: u64 = 0xDEAD_BEEF_CAFE_BABE;
    for i in (1..slice.len()).rev() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let j = (state as usize) % (i + 1);
        slice.swap(i, j);
    }
}
