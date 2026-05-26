//! Pixel-space coordinate types.
//!
//! Logical pixels are window/UI/input coordinates after OS DPI scaling.
//! Physical pixels are framebuffer/render-target coordinates.

use super::Vec2;

/// A point in window logical pixels.
///
/// This is the coordinate space reported by winit cursor input after DPI
/// scaling.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct LogicalPoint {
    pub x: f32,
    pub y: f32,
}

impl LogicalPoint {
    #[inline]
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    #[inline]
    pub const fn from_array(value: [f32; 2]) -> Self {
        Self {
            x: value[0],
            y: value[1],
        }
    }

    #[inline]
    pub const fn to_array(self) -> [f32; 2] {
        [self.x, self.y]
    }

    #[inline]
    pub fn to_vec2(self) -> Vec2 {
        Vec2::new(self.x, self.y)
    }
}

impl From<[f32; 2]> for LogicalPoint {
    #[inline]
    fn from(value: [f32; 2]) -> Self {
        Self::from_array(value)
    }
}

impl From<LogicalPoint> for [f32; 2] {
    #[inline]
    fn from(value: LogicalPoint) -> Self {
        value.to_array()
    }
}

/// A movement delta in window logical pixels.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct LogicalDelta {
    pub dx: f32,
    pub dy: f32,
}

impl LogicalDelta {
    #[inline]
    pub const fn new(dx: f32, dy: f32) -> Self {
        Self { dx, dy }
    }

    #[inline]
    pub const fn from_array(value: [f32; 2]) -> Self {
        Self {
            dx: value[0],
            dy: value[1],
        }
    }

    #[inline]
    pub const fn to_array(self) -> [f32; 2] {
        [self.dx, self.dy]
    }

    #[inline]
    pub fn to_vec2(self) -> Vec2 {
        Vec2::new(self.dx, self.dy)
    }
}

impl From<[f32; 2]> for LogicalDelta {
    #[inline]
    fn from(value: [f32; 2]) -> Self {
        Self::from_array(value)
    }
}

impl From<LogicalDelta> for [f32; 2] {
    #[inline]
    fn from(value: LogicalDelta) -> Self {
        value.to_array()
    }
}

/// A window or UI size in logical pixels.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct LogicalSize {
    pub width: f32,
    pub height: f32,
}

impl LogicalSize {
    #[inline]
    pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }

    #[inline]
    pub const fn from_array(value: [f32; 2]) -> Self {
        Self {
            width: value[0],
            height: value[1],
        }
    }

    #[inline]
    pub const fn to_array(self) -> [f32; 2] {
        [self.width, self.height]
    }

    #[inline]
    pub fn to_vec2(self) -> Vec2 {
        Vec2::new(self.width, self.height)
    }

    #[inline]
    pub fn contains(self, point: LogicalPoint) -> bool {
        point.x >= 0.0 && point.y >= 0.0 && point.x < self.width && point.y < self.height
    }

    #[inline]
    pub fn to_rounded_u32(self) -> [u32; 2] {
        [
            self.width.max(1.0).round() as u32,
            self.height.max(1.0).round() as u32,
        ]
    }

    #[inline]
    pub fn to_physical(self, scale_factor: f32) -> PhysicalSize {
        let scale = scale_factor.max(0.0001);
        PhysicalSize::new(
            (self.width.max(1.0) * scale).round() as u32,
            (self.height.max(1.0) * scale).round() as u32,
        )
    }
}

impl From<[f32; 2]> for LogicalSize {
    #[inline]
    fn from(value: [f32; 2]) -> Self {
        Self::from_array(value)
    }
}

impl From<LogicalSize> for [f32; 2] {
    #[inline]
    fn from(value: LogicalSize) -> Self {
        value.to_array()
    }
}

/// A framebuffer or render-target size in physical pixels.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PhysicalSize {
    pub width: u32,
    pub height: u32,
}

impl PhysicalSize {
    #[inline]
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    #[inline]
    pub const fn from_array(value: [u32; 2]) -> Self {
        Self {
            width: value[0],
            height: value[1],
        }
    }

    #[inline]
    pub const fn to_array(self) -> [u32; 2] {
        [self.width, self.height]
    }

    #[inline]
    pub fn to_logical(self, scale_factor: f32) -> LogicalSize {
        let scale = scale_factor.max(0.0001);
        LogicalSize::new(self.width as f32 / scale, self.height as f32 / scale)
    }
}

impl From<[u32; 2]> for PhysicalSize {
    #[inline]
    fn from(value: [u32; 2]) -> Self {
        Self::from_array(value)
    }
}

impl From<PhysicalSize> for [u32; 2] {
    #[inline]
    fn from(value: PhysicalSize) -> Self {
        value.to_array()
    }
}

#[cfg(test)]
mod tests {
    use super::{LogicalDelta, LogicalPoint, LogicalSize, PhysicalSize, Vec2};

    #[test]
    fn physical_and_logical_sizes_convert_with_dpi_scale() {
        let physical = PhysicalSize::new(3840, 2160);
        let logical = physical.to_logical(3.0);
        assert_eq!(logical, LogicalSize::new(1280.0, 720.0));
        assert_eq!(logical.to_physical(3.0), physical);
    }

    #[test]
    fn logical_size_contains_logical_points_only() {
        let size = LogicalSize::new(1280.0, 720.0);
        assert!(size.contains(LogicalPoint::new(1279.0, 719.0)));
        assert!(!size.contains(LogicalPoint::new(1280.0, 719.0)));
    }

    #[test]
    fn logical_delta_is_not_a_point_or_size() {
        let delta = LogicalDelta::new(-3.0, 4.0);
        assert_eq!(delta.to_array(), [-3.0, 4.0]);
        assert_eq!(delta.to_vec2(), Vec2::new(-3.0, 4.0));
    }
}
