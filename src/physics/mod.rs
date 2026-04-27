//! Optional 2D physics integration for SkyEngine.
//!
//! The public API is SkyEngine-owned. Rapier is used internally as the v1
//! simulation backend, but app code should interact with components and
//! resources from this module.
//!
//! Enable with `--features physics`. Rendering helpers such as debug draw and
//! Tiled physics demos require `--features "app physics"`.
//!
//! Typical setup:
//!
//! ```rust
//! # use sky_engine::ecs::World;
//! # use sky_engine::physics::{install_physics, PhysicsConfig2D};
//! let mut world = World::new();
//! install_physics(&mut world, PhysicsConfig2D::default());
//! ```
//!
//! See `docs/physics.md` for the full guide.

mod backend;
mod components;
mod config;
mod conversion;
#[cfg(feature = "app")]
mod debug;
mod events;
mod handles;
mod queries;
mod sync;
mod system;
mod world;

#[cfg(test)]
mod tests;

pub use components::{
    BodyType2D, Collider2D, ColliderShape2D, CollisionGroups2D, RigidBody2D, Velocity2D,
};
pub use config::PhysicsConfig2D;
#[cfg(feature = "app")]
pub use debug::{
    install_physics_debug_draw, sync_physics_debug_draw, PhysicsDebugDraw2D,
    PhysicsDebugDrawOptions2D,
};
pub use events::{PhysicsEvent2D, PhysicsEvents};
pub use queries::{PhysicsQueryFilter2D, RaycastHit2D};
pub use system::{install_physics, step_physics};
pub use world::PhysicsWorld2D;
