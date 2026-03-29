// Shared components, constants, and helpers for all bench files.
// Include via: #[path = "common.rs"] mod common; use common::*;

#![allow(dead_code)]

use bevy_ecs::prelude::Component;
use cgmath::{Matrix4, Rad, SquareMatrix, Vector3};
use hecs::World as HecsWorld;
use sky_engine::ecs::World as SkyWorld;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

pub const SIMPLE_ENTITY_COUNT: usize = 10_000;
pub const FRAGMENTED_VARIANT_COUNT: usize = 26;
pub const FRAGMENTED_ENTITIES_PER_VARIANT: usize = 20;
pub const HEAVY_ENTITY_COUNT: usize = 1_000;
pub const HEAVY_INVERT_COUNT: usize = 100;

// ---------------------------------------------------------------------------
// Components
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

pub fn sky_world_with_entities(n: usize) -> SkyWorld {
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

pub fn hecs_world_with_entities(n: usize) -> HecsWorld {
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
