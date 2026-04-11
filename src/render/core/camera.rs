//! View and camera helpers.

/// GPU-ready view uniform shared by 2D and future 3D render paths.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ViewUniform {
    pub view_proj: [f32; 16],
    pub camera: [f32; 4],
    pub viewport: [f32; 4], // width, height, inv_width, inv_height
}

/// Backwards-compatible alias for the existing 2D camera uniform name.
pub type CameraUniform = ViewUniform;

/// Any renderable view that can provide a packed GPU uniform.
pub trait RenderView {
    fn view_uniform(&self) -> ViewUniform;

    #[inline]
    fn viewport_size(&self) -> [f32; 2] {
        let uniform = self.view_uniform();
        [uniform.viewport[0], uniform.viewport[1]]
    }
}

/// A 2D orthographic camera that produces a view-projection matrix.
///
/// The camera maps world-space coordinates to normalised device coordinates.
/// Default: origin at screen centre, +X right, +Y up.
#[derive(Debug, Clone, Copy)]
pub struct Camera2D {
    /// World-space position of the camera centre.
    pub position: [f32; 2],
    /// Rotation around the Z axis in radians.
    pub rotation: f32,
    /// Zoom factor (1.0 = no zoom, 2.0 = 2× zoom in).
    pub zoom: f32,
    /// Viewport width in pixels (updated on resize).
    pub(crate) viewport_width: f32,
    /// Viewport height in pixels.
    pub(crate) viewport_height: f32,
}

impl Camera2D {
    #[inline]
    fn sanitized_viewport(&self) -> (f32, f32) {
        (
            self.viewport_width.max(f32::EPSILON),
            self.viewport_height.max(f32::EPSILON),
        )
    }

    #[inline]
    fn sanitized_zoom(&self) -> f32 {
        self.zoom.max(f32::EPSILON)
    }

    /// Create a new camera with the given viewport size.
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            position: [0.0, 0.0],
            rotation: 0.0,
            zoom: 1.0,
            viewport_width: width,
            viewport_height: height,
        }
    }

    /// Update viewport dimensions (call on window resize).
    pub fn set_viewport(&mut self, width: f32, height: f32) {
        self.viewport_width = width;
        self.viewport_height = height;
    }

    /// Compute the 4×4 view-projection matrix (column-major).
    ///
    /// Maps world coordinates to clip space [-1, 1]:
    /// - Camera position is centred in the viewport
    /// - Zoom scales the visible area
    /// - +Y points up (standard math convention)
    pub fn view_projection(&self) -> [f32; 16] {
        let (viewport_width, viewport_height) = self.sanitized_viewport();
        let zoom = self.sanitized_zoom();
        let hw = viewport_width * 0.5 / zoom;
        let hh = viewport_height * 0.5 / zoom;

        let projection = [
            hw.recip(),
            0.0,
            0.0,
            0.0,
            0.0,
            hh.recip(),
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
        ];
        let (sin_r, cos_r) = self.rotation.sin_cos();
        let rotation = [
            cos_r, -sin_r, 0.0, 0.0, sin_r, cos_r, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ];
        let translation = [
            1.0,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
            0.0,
            -self.position[0],
            -self.position[1],
            0.0,
            1.0,
        ];
        let view = mul_mat4(rotation, translation);
        mul_mat4(projection, view)
    }

    fn build_uniform(&self) -> ViewUniform {
        let inv_width = if self.viewport_width > 0.0 {
            self.viewport_width.recip()
        } else {
            0.0
        };
        let inv_height = if self.viewport_height > 0.0 {
            self.viewport_height.recip()
        } else {
            0.0
        };

        ViewUniform {
            view_proj: self.view_projection(),
            camera: [self.position[0], self.position[1], self.zoom, 0.0],
            viewport: [
                self.viewport_width,
                self.viewport_height,
                inv_width,
                inv_height,
            ],
        }
    }

    /// Return the packed uniform consumed by render shaders.
    pub fn uniform(&self) -> CameraUniform {
        self.build_uniform()
    }

    /// Viewport width in pixels.
    #[inline]
    pub fn viewport_width(&self) -> f32 {
        self.viewport_width
    }

    /// Viewport height in pixels.
    #[inline]
    pub fn viewport_height(&self) -> f32 {
        self.viewport_height
    }

    /// Convert screen coordinates to world coordinates.
    pub fn screen_to_world(&self, screen_x: f32, screen_y: f32) -> [f32; 2] {
        let (viewport_width, viewport_height) = self.sanitized_viewport();
        let zoom = self.sanitized_zoom();
        let hw = viewport_width * 0.5 / zoom;
        let hh = viewport_height * 0.5 / zoom;
        let local_x = (screen_x / viewport_width - 0.5) * 2.0 * hw;
        let local_y = -(screen_y / viewport_height - 0.5) * 2.0 * hh;
        let (sin_r, cos_r) = self.rotation.sin_cos();
        let world_x = self.position[0] + cos_r * local_x - sin_r * local_y;
        let world_y = self.position[1] + sin_r * local_x + cos_r * local_y;

        [world_x, world_y]
    }
}

fn mul_mat4(lhs: [f32; 16], rhs: [f32; 16]) -> [f32; 16] {
    let mut out = [0.0; 16];
    for row in 0..4 {
        for col in 0..4 {
            let mut value = 0.0;
            for k in 0..4 {
                value += lhs[k * 4 + row] * rhs[col * 4 + k];
            }
            out[col * 4 + row] = value;
        }
    }
    out
}

impl RenderView for Camera2D {
    #[inline]
    fn view_uniform(&self) -> ViewUniform {
        self.build_uniform()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_viewport_and_zoom_stay_finite() {
        let mut camera = Camera2D::new(0.0, 0.0);
        camera.zoom = 0.0;

        assert!(camera
            .view_projection()
            .iter()
            .all(|value| value.is_finite()));
        assert!(camera
            .screen_to_world(0.0, 0.0)
            .iter()
            .all(|value| value.is_finite()));
    }
}
