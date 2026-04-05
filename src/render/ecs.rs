//! ECS-facing 2D rendering components and settings.

use crate::render::{Color, Texture};

/// A 2D transform used by the high-level renderer.
#[derive(Clone, Copy, Debug)]
pub struct Transform2D {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub scale_x: f32,
    pub scale_y: f32,
    pub rotation: f32,
}

impl Transform2D {
    #[inline]
    pub const fn new(x: f32, y: f32) -> Self {
        Self {
            x,
            y,
            z: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            rotation: 0.0,
        }
    }

    #[inline]
    pub const fn from_xyz(x: f32, y: f32, z: f32) -> Self {
        Self {
            x,
            y,
            z,
            scale_x: 1.0,
            scale_y: 1.0,
            rotation: 0.0,
        }
    }

    #[inline]
    pub const fn with_z(mut self, z: f32) -> Self {
        self.z = z;
        self
    }

    #[inline]
    pub const fn with_scale(mut self, scale_x: f32, scale_y: f32) -> Self {
        self.scale_x = scale_x;
        self.scale_y = scale_y;
        self
    }

    #[inline]
    pub const fn with_rotation(mut self, rotation: f32) -> Self {
        self.rotation = rotation;
        self
    }
}

impl Default for Transform2D {
    fn default() -> Self {
        Self::new(0.0, 0.0)
    }
}

/// High-level sprite component consumed by [`crate::render::Renderer2D`].
#[derive(Clone)]
pub struct Sprite2D {
    pub width: f32,
    pub height: f32,
    pub color: Color,
    pub uv: [f32; 4],
    pub texture: Option<Texture>,
    pub visible: bool,
}

impl Sprite2D {
    #[inline]
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            width,
            height,
            color: Color::WHITE,
            uv: [0.0, 0.0, 1.0, 1.0],
            texture: None,
            visible: true,
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
}

impl Default for Sprite2D {
    fn default() -> Self {
        Self::new(1.0, 1.0)
    }
}

impl std::fmt::Debug for Sprite2D {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sprite2D")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("color", &self.color)
            .field("uv", &self.uv)
            .field("textured", &self.texture.is_some())
            .field("visible", &self.visible)
            .finish()
    }
}

/// High-level 2D point light component.
#[derive(Clone, Copy, Debug)]
pub struct PointLight2D {
    pub radius: f32,
    pub intensity: f32,
    pub color: Color,
    pub temperature: f32,
    pub falloff: f32,
    pub visible: bool,
}

impl PointLight2D {
    #[inline]
    pub const fn new(radius: f32) -> Self {
        Self {
            radius,
            intensity: 1.0,
            color: Color::WHITE,
            temperature: 6500.0,
            falloff: 2.0,
            visible: true,
        }
    }

    #[inline]
    pub const fn intensity(mut self, intensity: f32) -> Self {
        self.intensity = intensity;
        self
    }

    #[inline]
    pub const fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    #[inline]
    pub const fn temperature(mut self, temperature: f32) -> Self {
        self.temperature = temperature;
        self
    }

    #[inline]
    pub const fn falloff(mut self, falloff: f32) -> Self {
        self.falloff = falloff;
        self
    }

    #[inline]
    pub const fn visible(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }
}

impl Default for PointLight2D {
    fn default() -> Self {
        Self::new(128.0)
    }
}

/// Marker component identifying the primary camera used by the high-level renderer.
#[derive(Clone, Copy, Debug, Default)]
pub struct PrimaryCamera2D;

/// Bloom controls for the high-level renderer.
#[derive(Clone, Copy, Debug)]
pub struct BloomSettings {
    pub enabled: bool,
    pub threshold: f32,
    pub intensity: f32,
    pub radius: f32,
}

impl Default for BloomSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            threshold: 0.55,
            intensity: 0.45,
            radius: 1.15,
        }
    }
}

/// Tonemap controls for the high-level renderer.
#[derive(Clone, Copy, Debug)]
pub struct ToneMapSettings {
    pub enabled: bool,
    pub exposure: f32,
    pub gamma: f32,
}

impl Default for ToneMapSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            exposure: 1.35,
            gamma: 2.2,
        }
    }
}

/// Vignette controls for the high-level renderer.
#[derive(Clone, Copy, Debug)]
pub struct VignetteSettings {
    pub enabled: bool,
    pub intensity: f32,
    pub smoothness: f32,
}

impl Default for VignetteSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            intensity: 0.35,
            smoothness: 0.28,
        }
    }
}

/// Per-frame render settings consumed from the [`crate::ecs::World`].
#[derive(Clone, Copy, Debug)]
pub struct RenderSettings2D {
    pub clear_color: Color,
    pub ambient_color: Color,
    pub bloom: BloomSettings,
    pub tonemap: ToneMapSettings,
    pub vignette: VignetteSettings,
}

impl Default for RenderSettings2D {
    fn default() -> Self {
        Self {
            clear_color: Color::new(0.015, 0.016, 0.02, 1.0),
            ambient_color: Color::new(0.07, 0.075, 0.09, 1.0),
            bloom: BloomSettings::default(),
            tonemap: ToneMapSettings::default(),
            vignette: VignetteSettings::default(),
        }
    }
}
