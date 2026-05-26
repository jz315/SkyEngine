//! SkyEngine shared math and color layer.
//!
//! SkyEngine owns this API surface even though the current math implementation
//! is backed by `glam` internally.

pub mod color;
pub mod geometry;
pub mod matrix;
pub mod projection;
pub mod quaternion;
#[cfg(feature = "reflect")]
pub mod reflect;
pub mod screen;
pub mod transform;
pub mod vector;

pub use color::Color;
pub use geometry::{Aabb2, Aabb3, Ray2, Ray3};
pub use matrix::Mat4;
pub use projection::Projection;
pub use quaternion::Quat;
pub use screen::{LogicalDelta, LogicalPoint, LogicalSize, PhysicalSize};
pub use transform::Transform;
pub use vector::{Vec2, Vec3, Vec4};
