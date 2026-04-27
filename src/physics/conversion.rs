use rapier2d::prelude::{ActiveCollisionTypes, ColliderBuilder, Pose, Vector};

use crate::ecs::EntityId;
use crate::math::Transform;

use super::components::{Collider2D, ColliderShape2D};
use super::world::PhysicsWorld2D;

impl PhysicsWorld2D {
    #[inline]
    pub(crate) fn pixels_per_meter(&self) -> f32 {
        safe_pixels_per_meter(self.config.pixels_per_meter)
    }
}

#[inline]
pub(crate) fn safe_pixels_per_meter(pixels_per_meter: f32) -> f32 {
    pixels_per_meter.max(f32::EPSILON)
}

pub(crate) fn collider_builder(collider: Collider2D, pixels_per_meter: f32) -> ColliderBuilder {
    collider_builder_for_shape(collider.shape, pixels_per_meter)
        .sensor(collider.sensor)
        .enabled(collider.enabled)
        .friction(collider.friction)
        .restitution(collider.restitution)
        .collision_groups(collider.collision_groups.to_rapier())
        .solver_groups(collider.solver_groups.to_rapier())
}

pub(crate) fn active_collision_types() -> ActiveCollisionTypes {
    ActiveCollisionTypes::default()
        | ActiveCollisionTypes::KINEMATIC_FIXED
        | ActiveCollisionTypes::KINEMATIC_KINEMATIC
}

pub(crate) fn collider_builder_for_shape(
    shape: ColliderShape2D,
    pixels_per_meter: f32,
) -> ColliderBuilder {
    let ppm = safe_pixels_per_meter(pixels_per_meter);
    match shape {
        ColliderShape2D::Rectangle { width, height } => {
            ColliderBuilder::cuboid((width * 0.5).max(0.0) / ppm, (height * 0.5).max(0.0) / ppm)
        }
        ColliderShape2D::Circle { radius } => ColliderBuilder::ball(radius.max(0.0) / ppm),
        ColliderShape2D::CapsuleY {
            half_height,
            radius,
        } => ColliderBuilder::capsule_y(half_height.max(0.0) / ppm, radius.max(0.0) / ppm),
    }
}

pub(crate) fn transform_translation(transform: Transform, pixels_per_meter: f32) -> Vector {
    physics_vec(
        transform.position.x() / pixels_per_meter,
        transform.position.y() / pixels_per_meter,
    )
}

#[inline]
pub(crate) fn physics_vec(x: f32, y: f32) -> Vector {
    Vector::new(x, y)
}

pub(crate) fn transform_pose(transform: Transform, pixels_per_meter: f32) -> Pose {
    Pose::new(
        transform_translation(transform, pixels_per_meter),
        transform.rotation_z(),
    )
}

pub(crate) fn entity_user_data(entity: EntityId) -> u128 {
    ((entity.generation() as u128) << 64) | entity.index() as u128
}
