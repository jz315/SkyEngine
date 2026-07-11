use crate::render::view::ViewportRect;

/// High-level scene camera marker/config.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Camera {
    pub enabled: bool,
}

impl Camera {
    #[inline]
    pub const fn new() -> Self {
        Self { enabled: true }
    }

    #[inline]
    pub const fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

/// ECS/manual render-view descriptor for multi-camera rendering.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CameraViewport {
    pub order: i32,
    pub viewport: ViewportRect,
    pub layer_mask: u32,
}

impl CameraViewport {
    #[inline]
    pub const fn new(viewport: ViewportRect) -> Self {
        Self {
            order: 0,
            viewport,
            layer_mask: u32::MAX,
        }
    }

    #[inline]
    pub const fn order(mut self, order: i32) -> Self {
        self.order = order;
        self
    }

    #[inline]
    pub const fn layer_mask(mut self, layer_mask: u32) -> Self {
        self.layer_mask = layer_mask;
        self
    }
}

impl Default for CameraViewport {
    fn default() -> Self {
        Self::new(ViewportRect::default())
    }
}

/// Marker component identifying the primary camera used by the high-level renderer.
#[derive(Clone, Copy, Debug, Default)]
pub struct MainCamera;
