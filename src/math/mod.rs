//! SkyEngine public math layer.
//!
//! The engine owns this API surface even though the current implementation is
//! backed by `glam` internally.

pub mod matrix;
pub mod projection;
pub mod quaternion;
pub mod screen;
pub mod transform;
pub mod vector;

pub use matrix::Mat4;
pub use projection::Projection;
pub use quaternion::Quat;
pub use screen::{LogicalDelta, LogicalPoint, LogicalSize, PhysicalSize};
pub use transform::Transform;
pub use vector::{Vec2, Vec3, Vec4};
