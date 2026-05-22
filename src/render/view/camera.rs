//! View and camera helpers.

use crate::math::{LogicalPoint, LogicalSize, Projection, Transform, Vec2};

/// GPU-ready view uniform shared by 2D and 3D render paths.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ViewUniform {
    pub view_proj: [f32; 16],
    pub camera: [f32; 4],
    pub viewport: [f32; 4], // width, height, inv_width, inv_height
    pub view: [f32; 16],
    pub projection: [f32; 16],
    pub inverse_view: [f32; 16],
    pub camera_position: [f32; 4],
    pub near_far_time_delta: [f32; 4],
}

/// Any renderable view that can provide a packed GPU uniform.
pub trait RenderView {
    fn view_uniform(&self) -> ViewUniform;

    #[inline]
    fn viewport_size(&self) -> [f32; 2] {
        let uniform = self.view_uniform();
        [uniform.viewport[0], uniform.viewport[1]]
    }
}

/// Unified camera backed by `Transform` + `Projection`.
///
/// 2D is a subset of 3D: use `Transform::from_xy(x, y)` with
/// `Projection::orthographic(height)` for orthographic 2D views,
/// or a full 3D transform with `Projection::perspective(...)` for 3D.
#[derive(Debug, Clone, Copy)]
pub struct Camera {
    /// The camera's spatial transform (position, rotation, scale).
    pub transform: Transform,
    /// The projection mode (orthographic or perspective).
    pub projection: Projection,
    viewport_size: [u32; 2],
}

impl Camera {
    /// Create a default orthographic 2D camera with the given viewport.
    #[inline]
    pub fn new(viewport_width: f32, viewport_height: f32) -> Self {
        Self {
            transform: Transform::default(),
            projection: Projection::orthographic(viewport_height.max(1.0)),
            viewport_size: [
                viewport_width.max(1.0) as u32,
                viewport_height.max(1.0) as u32,
            ],
        }
    }

    /// Create a camera with explicit transform and projection.
    #[inline]
    pub fn with(transform: Transform, projection: Projection, viewport: [u32; 2]) -> Self {
        Self {
            transform,
            projection,
            viewport_size: viewport,
        }
    }

    /// Update the viewport dimensions.
    #[inline]
    pub fn set_viewport(&mut self, width: f32, height: f32) {
        self.viewport_size = [width.max(1.0) as u32, height.max(1.0) as u32];
    }

    /// Convert a logical-pixel screen point to world coordinates.
    #[inline]
    pub fn screen_to_world_logical(&self, point: LogicalPoint) -> Vec2 {
        let viewport = LogicalSize::new(self.viewport_size[0] as f32, self.viewport_size[1] as f32);
        self.projection
            .screen_to_world_logical(self.transform, viewport, point)
    }

    /// Return the packed uniform consumed by render shaders.
    #[inline]
    pub fn uniform(&self) -> ViewUniform {
        self.projection
            .view_uniform(self.transform, self.viewport_size)
    }

    /// Whether this camera's view is constrained to the XY plane (pure 2D).
    #[inline]
    pub fn is_planar_2d(&self) -> bool {
        self.transform.is_planar_2d()
    }
}

impl RenderView for Camera {
    #[inline]
    fn view_uniform(&self) -> ViewUniform {
        self.uniform()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_viewport_and_zoom_stay_finite() {
        let mut camera = Camera::new(0.0, 0.0);
        // Set zoom to 0 on the projection
        if let Projection::Orthographic { zoom, .. } = &mut camera.projection {
            *zoom = 0.0;
        }

        assert!(camera
            .uniform()
            .view_proj
            .iter()
            .all(|value| value.is_finite()));
    }
}
