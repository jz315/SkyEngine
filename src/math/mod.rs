//! SkyEngine public math layer.
//!
//! The implementation now lives in the internal `sky_math` crate. This module
//! preserves the historical `sky_engine::math` paths.

pub use sky_math::*;

pub mod angle {
    pub use sky_math::angle::*;
}

pub mod color {
    pub use sky_math::color::*;
}

pub mod geometry {
    pub use sky_math::geometry::*;
}

pub mod matrix {
    pub use sky_math::matrix::*;
}

pub mod projection {
    pub use sky_math::projection::*;
}

pub mod quaternion {
    pub use sky_math::quaternion::*;
}

pub mod screen {
    pub use sky_math::screen::*;
}

pub mod transform {
    pub use sky_math::transform::*;
}

pub mod vector {
    pub use sky_math::vector::*;
}
