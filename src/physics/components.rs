use rapier2d::prelude::{Group, InteractionGroups, InteractionTestMode, RigidBodyType};

use crate::math::Vec2;

/// Rigid body kind used by [`RigidBody2D`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyType2D {
    /// A fixed/static body. It participates in collision but does not move.
    Static,
    /// A position-based kinematic body. Movement is driven by `Transform` or `Velocity2D`.
    Kinematic,
    /// A fully simulated dynamic body.
    Dynamic,
}

impl BodyType2D {
    pub(crate) fn to_rapier(self) -> RigidBodyType {
        match self {
            Self::Static => RigidBodyType::Fixed,
            Self::Kinematic => RigidBodyType::KinematicPositionBased,
            Self::Dynamic => RigidBodyType::Dynamic,
        }
    }
}

/// ECS component describing a 2D rigid body.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RigidBody2D {
    pub body_type: BodyType2D,
    pub enabled: bool,
    pub gravity_scale: f32,
    pub can_sleep: bool,
    pub ccd_enabled: bool,
    pub lock_rotation: bool,
}

impl RigidBody2D {
    #[inline]
    pub const fn new(body_type: BodyType2D) -> Self {
        Self {
            body_type,
            enabled: true,
            gravity_scale: 1.0,
            can_sleep: true,
            ccd_enabled: false,
            lock_rotation: false,
        }
    }

    #[inline]
    pub const fn static_body() -> Self {
        Self::new(BodyType2D::Static)
    }

    #[inline]
    pub const fn kinematic() -> Self {
        Self::new(BodyType2D::Kinematic)
    }

    #[inline]
    pub const fn dynamic() -> Self {
        Self::new(BodyType2D::Dynamic)
    }

    #[inline]
    pub const fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }

    #[inline]
    pub const fn gravity_scale(mut self, gravity_scale: f32) -> Self {
        self.gravity_scale = gravity_scale;
        self
    }

    #[inline]
    pub const fn can_sleep(mut self, can_sleep: bool) -> Self {
        self.can_sleep = can_sleep;
        self
    }

    #[inline]
    pub const fn ccd(mut self, enabled: bool) -> Self {
        self.ccd_enabled = enabled;
        self
    }

    #[inline]
    pub const fn lock_rotation(mut self) -> Self {
        self.lock_rotation = true;
        self
    }
}

impl Default for RigidBody2D {
    fn default() -> Self {
        Self::dynamic()
    }
}

/// ECS component for linear/angular velocity.
///
/// Linear velocity is expressed in render/world units per second.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Velocity2D {
    pub linear: Vec2,
    pub angular: f32,
}

impl Velocity2D {
    #[inline]
    pub fn new(x: f32, y: f32) -> Self {
        Self {
            linear: Vec2::new(x, y),
            angular: 0.0,
        }
    }

    #[inline]
    pub const fn with_angular(mut self, angular: f32) -> Self {
        self.angular = angular;
        self
    }
}

impl Default for Velocity2D {
    fn default() -> Self {
        Self {
            linear: Vec2::ZERO,
            angular: 0.0,
        }
    }
}

/// Shape for a 2D collider. Dimensions are in render/world units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ColliderShape2D {
    Rectangle { width: f32, height: f32 },
    Circle { radius: f32 },
    CapsuleY { half_height: f32, radius: f32 },
}

impl ColliderShape2D {
    #[inline]
    pub const fn rectangle(width: f32, height: f32) -> Self {
        Self::Rectangle { width, height }
    }

    #[inline]
    pub const fn circle(radius: f32) -> Self {
        Self::Circle { radius }
    }

    #[inline]
    pub const fn capsule_y(half_height: f32, radius: f32) -> Self {
        Self::CapsuleY {
            half_height,
            radius,
        }
    }
}

/// Collision group membership/filter masks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CollisionGroups2D {
    pub memberships: u32,
    pub filters: u32,
}

impl CollisionGroups2D {
    #[inline]
    pub const fn new(memberships: u32, filters: u32) -> Self {
        Self {
            memberships,
            filters,
        }
    }

    #[inline]
    pub const fn all() -> Self {
        Self::new(u32::MAX, u32::MAX)
    }

    #[inline]
    pub const fn none() -> Self {
        Self::new(0, 0)
    }

    pub(crate) fn to_rapier(self) -> InteractionGroups {
        InteractionGroups::new(
            Group::from(self.memberships),
            Group::from(self.filters),
            InteractionTestMode::And,
        )
    }
}

impl Default for CollisionGroups2D {
    fn default() -> Self {
        Self::all()
    }
}

/// ECS component describing a collider attached to the entity's body.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Collider2D {
    pub shape: ColliderShape2D,
    pub sensor: bool,
    pub enabled: bool,
    pub friction: f32,
    pub restitution: f32,
    pub collision_groups: CollisionGroups2D,
    pub solver_groups: CollisionGroups2D,
    pub offset: Vec2,
    pub rotation: f32,
}

impl Collider2D {
    #[inline]
    pub fn new(shape: ColliderShape2D) -> Self {
        Self {
            shape,
            sensor: false,
            enabled: true,
            friction: 0.5,
            restitution: 0.0,
            collision_groups: CollisionGroups2D::all(),
            solver_groups: CollisionGroups2D::all(),
            offset: Vec2::ZERO,
            rotation: 0.0,
        }
    }

    #[inline]
    pub fn rectangle(width: f32, height: f32) -> Self {
        Self::new(ColliderShape2D::rectangle(width, height))
    }

    #[inline]
    pub fn circle(radius: f32) -> Self {
        Self::new(ColliderShape2D::circle(radius))
    }

    #[inline]
    pub fn capsule_y(half_height: f32, radius: f32) -> Self {
        Self::new(ColliderShape2D::capsule_y(half_height, radius))
    }

    #[inline]
    pub fn sensor(mut self, sensor: bool) -> Self {
        self.sensor = sensor;
        self
    }

    #[inline]
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    #[inline]
    pub fn friction(mut self, friction: f32) -> Self {
        self.friction = friction;
        self
    }

    #[inline]
    pub fn restitution(mut self, restitution: f32) -> Self {
        self.restitution = restitution;
        self
    }

    #[inline]
    pub fn collision_groups(mut self, groups: CollisionGroups2D) -> Self {
        self.collision_groups = groups;
        self
    }

    #[inline]
    pub fn solver_groups(mut self, groups: CollisionGroups2D) -> Self {
        self.solver_groups = groups;
        self
    }

    #[inline]
    pub fn offset(mut self, offset: Vec2) -> Self {
        self.offset = offset;
        self
    }

    #[inline]
    pub fn rotation(mut self, rotation: f32) -> Self {
        self.rotation = rotation;
        self
    }
}
