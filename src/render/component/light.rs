use crate::render::Color;

/// High-level point light component for the programmable renderer.
#[derive(Clone, Copy, Debug)]
pub struct PointLight {
    pub radius: f32,
    pub intensity: f32,
    pub color: Color,
    pub temperature: f32,
    pub falloff: f32,
    pub visible: bool,
    pub layer_mask: u32,
}

impl PointLight {
    #[inline]
    pub const fn new(radius: f32) -> Self {
        Self {
            radius,
            intensity: 1.0,
            color: Color::WHITE,
            temperature: 6500.0,
            falloff: 2.0,
            visible: true,
            layer_mask: u32::MAX,
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

    #[inline]
    pub const fn layer_mask(mut self, layer_mask: u32) -> Self {
        self.layer_mask = layer_mask;
        self
    }
}

impl Default for PointLight {
    fn default() -> Self {
        Self::new(128.0)
    }
}

/// High-level directional light component for future 3D/light-view workflows.
#[derive(Clone, Copy, Debug)]
pub struct DirectionalLight {
    pub direction: [f32; 3],
    pub intensity: f32,
    pub color: Color,
    pub visible: bool,
    pub layer_mask: u32,
    pub casts_shadows: bool,
    pub shadow_map_size: u32,
    pub shadow_bias: f32,
}

impl DirectionalLight {
    #[inline]
    pub const fn new(direction: [f32; 3]) -> Self {
        Self {
            direction,
            intensity: 1.0,
            color: Color::WHITE,
            visible: true,
            layer_mask: u32::MAX,
            casts_shadows: true,
            shadow_map_size: 1024,
            shadow_bias: 0.0015,
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
    pub const fn visible(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }

    #[inline]
    pub const fn layer_mask(mut self, layer_mask: u32) -> Self {
        self.layer_mask = layer_mask;
        self
    }

    #[inline]
    pub const fn casts_shadows(mut self, casts_shadows: bool) -> Self {
        self.casts_shadows = casts_shadows;
        self
    }

    #[inline]
    pub const fn shadow_map_size(mut self, shadow_map_size: u32) -> Self {
        self.shadow_map_size = if shadow_map_size == 0 {
            1
        } else {
            shadow_map_size
        };
        self
    }

    #[inline]
    pub const fn shadow_bias(mut self, shadow_bias: f32) -> Self {
        self.shadow_bias = shadow_bias;
        self
    }
}

impl Default for DirectionalLight {
    fn default() -> Self {
        Self::new([0.0, -1.0, 0.0])
    }
}
