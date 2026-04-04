//! 2D orthographic camera.

/// GPU-ready camera uniform shared by sprite and fullscreen passes.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CameraUniform {
    pub view_proj: [f32; 16],
    pub camera: [f32; 4],   // x, y, zoom, 0
    pub viewport: [f32; 4], // width, height, inv_width, inv_height
}

/// A 2D orthographic camera that produces a view-projection matrix.
///
/// The camera maps world-space coordinates to normalised device coordinates.
/// Default: origin at screen centre, +X right, +Y up.
#[derive(Debug, Clone, Copy)]
pub struct Camera2D {
    /// World-space position of the camera centre.
    pub position: [f32; 2],
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

        let left = self.position[0] - hw;
        let right = self.position[0] + hw;
        let bottom = self.position[1] - hh;
        let top = self.position[1] + hh;

        // Orthographic projection matrix (column-major)
        let sx = 2.0 / (right - left);
        let sy = 2.0 / (top - bottom);
        let tx = -(right + left) / (right - left);
        let ty = -(top + bottom) / (top - bottom);

        [
            sx, 0.0, 0.0, 0.0, 0.0, sy, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, tx, ty, 0.0, 1.0,
        ]
    }

    /// Return the packed uniform consumed by render shaders.
    pub fn uniform(&self) -> CameraUniform {
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

        CameraUniform {
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

        let world_x = self.position[0] + (screen_x / viewport_width - 0.5) * 2.0 * hw;
        // Flip Y: screen Y goes down, world Y goes up
        let world_y = self.position[1] - (screen_y / viewport_height - 0.5) * 2.0 * hh;

        [world_x, world_y]
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
