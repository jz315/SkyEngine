use crate::render::{Color, Texture};

/// Coarse scene ordering layer shared across renderable content.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct SortingLayer(pub i32);

/// Stable order within a [`SortingLayer`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct OrderInLayer(pub i32);

/// High-level sprite component consumed by the default scene renderer.
#[derive(Clone)]
pub struct SpriteRenderer {
    pub width: f32,
    pub height: f32,
    pub color: Color,
    pub uv: [f32; 4],
    pub texture: Option<Texture>,
    pub visible: bool,
    pub layer_mask: u32,
}

impl SpriteRenderer {
    #[inline]
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            width,
            height,
            color: Color::WHITE,
            uv: [0.0, 0.0, 1.0, 1.0],
            texture: None,
            visible: true,
            layer_mask: u32::MAX,
        }
    }

    #[inline]
    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    #[inline]
    pub fn texture(mut self, texture: Texture) -> Self {
        self.texture = Some(texture);
        self
    }

    #[inline]
    pub fn clear_texture(mut self) -> Self {
        self.texture = None;
        self
    }

    #[inline]
    pub fn uv(mut self, u_min: f32, v_min: f32, u_max: f32, v_max: f32) -> Self {
        self.uv = [u_min, v_min, u_max, v_max];
        self
    }

    #[inline]
    pub fn visible(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }

    #[inline]
    pub fn layer_mask(mut self, layer_mask: u32) -> Self {
        self.layer_mask = layer_mask;
        self
    }
}

impl Default for SpriteRenderer {
    fn default() -> Self {
        Self::new(1.0, 1.0)
    }
}

impl std::fmt::Debug for SpriteRenderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SpriteRenderer")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("color", &self.color)
            .field("uv", &self.uv)
            .field("textured", &self.texture.is_some())
            .field("visible", &self.visible)
            .field("layer_mask", &self.layer_mask)
            .finish()
    }
}
