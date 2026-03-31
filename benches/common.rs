// Shared components, constants, and helpers for all bench files.
// Include via: #[path = "../common.rs"] mod common; use common::*;

#![allow(dead_code)]

use bevy_ecs::prelude::Component;
use cgmath::{Matrix4, Rad, Vector3};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

pub const SIMPLE_ENTITY_COUNT: usize = 10_000;
pub const FRAGMENTED_VARIANT_COUNT: usize = 26;
pub const FRAGMENTED_ENTITIES_PER_VARIANT: usize = 20;
pub const HEAVY_ENTITY_COUNT: usize = 1_000;
pub const HEAVY_INVERT_COUNT: usize = 100;
pub const ENTITY_OP_COUNT: usize = 1_000;

/// Entity count for the head-to-head hot-path benchmarks (sky vs hecs).
pub const HOT_PATH_ENTITY_COUNT: usize = 5_000_000;
pub const HOT_PATH_DELTA: f32 = 0.1;

// ---------------------------------------------------------------------------
// Components — used across all engines
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Component)]
pub struct TransformComponent(pub Matrix4<f32>);

#[derive(Clone, Copy, Component)]
pub struct PositionComponent(pub Vector3<f32>);

#[derive(Clone, Copy, Component)]
pub struct RotationComponent(pub Vector3<f32>);

#[derive(Clone, Copy, Component)]
pub struct VelocityComponent(pub Vector3<f32>);

#[derive(Clone, Copy, Component)]
pub struct DataComponent(pub f32);

#[derive(Clone, Copy, Default, Component)]
pub struct Health(pub f32);

#[derive(Clone, Copy, Default, Component)]
pub struct Damage(pub f32);

#[derive(Clone, Copy, Default, Component)]
pub struct IsEnemy;

#[derive(Clone, Copy, Default, Component)]
pub struct IsAlly;

/// Lightweight 2-field components for the hot-path head-to-head benchmarks.
#[derive(Clone, Copy, Component)]
pub struct Position2D {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, Component)]
pub struct Velocity2D {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, Component)]
pub struct AuxA {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, Component)]
pub struct AuxB {
    pub x: f32,
    pub y: f32,
}

macro_rules! define_fragment_tags {
    ($($name:ident),+ $(,)?) => {
        $(
            #[derive(Clone, Copy, Default, Component)]
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
