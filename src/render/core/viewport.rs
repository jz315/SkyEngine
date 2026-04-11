/// Pixel-space viewport rectangle on a render target or presentation surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewportRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl ViewportRect {
    #[inline]
    pub const fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    #[inline]
    pub const fn full_surface(width: u32, height: u32) -> Self {
        Self::new(0, 0, width, height)
    }

    #[inline]
    pub fn from_surface_size(surface_size: [u32; 2]) -> Self {
        Self::full_surface(surface_size[0], surface_size[1])
    }

    #[inline]
    pub fn clamp_to_surface(self, surface_size: [u32; 2]) -> Self {
        let surface_width = surface_size[0];
        let surface_height = surface_size[1];
        let x = self.x.min(surface_width);
        let y = self.y.min(surface_height);
        let width = self.width.min(surface_width.saturating_sub(x)).max(1);
        let height = self.height.min(surface_height.saturating_sub(y)).max(1);
        Self {
            x,
            y,
            width,
            height,
        }
    }

    #[inline]
    pub fn size(self) -> [u32; 2] {
        [self.width.max(1), self.height.max(1)]
    }
}

impl Default for ViewportRect {
    fn default() -> Self {
        Self::new(0, 0, 1, 1)
    }
}
