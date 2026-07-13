//! SkyEngine shared math and color layer.
//!
//! SkyEngine owns this API surface even though the current math implementation
//! is backed by `glam` internally.
//!
//! Coordinates are right-handed, cameras look along negative Z, matrices are
//! column-major, projection depth is 0.0..=1.0, and [`Color`] stores linear RGBA.

pub mod angle;
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

pub use angle::{
    degrees_to_radians, lerp_angle_radians, normalize_angle_radians, radians_to_degrees,
};
pub use color::{Color, Srgba};
pub use geometry::{
    Aabb2, Aabb3, Circle, Frustum, Plane, Ray2, Ray3, RayTriangleHit, Sphere, Triangle2, Triangle3,
};
pub use matrix::{Affine2, Affine3, Mat3, Mat4};
pub use projection::{Projection, ProjectionError};
pub use quaternion::Quat;
pub use screen::{LogicalDelta, LogicalPoint, LogicalSize, PhysicalSize};
pub use transform::Transform;
pub use vector::{IVec2, IVec3, UVec2, UVec3, Vec2, Vec3, Vec4};
