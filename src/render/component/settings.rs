use crate::render::Color;

/// Shared render-layer mask used by the unified scene renderer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderLayerMask(pub u32);

impl RenderLayerMask {
    #[inline]
    pub const fn all() -> Self {
        Self(u32::MAX)
    }
}

impl Default for RenderLayerMask {
    fn default() -> Self {
        Self::all()
    }
}

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

/// Debug visualization mode for DDGI.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GiDebugMode {
    #[default]
    Off,
    Probes,
    Irradiance,
    Visibility,
    RayBudget,
}

/// World-space DDGI probe volume controls.
#[derive(Clone, Copy, Debug)]
pub struct DdgiVolumeSettings {
    pub origin: [f32; 3],
    pub spacing: f32,
    pub counts: [u32; 3],
    pub scroll_with_main_camera: bool,
}

impl Default for DdgiVolumeSettings {
    fn default() -> Self {
        Self {
            origin: [-14.0, -4.0, -14.0],
            spacing: 1.85,
            counts: [16, 8, 16],
            scroll_with_main_camera: true,
        }
    }
}

/// Dynamic diffuse global illumination controls.
#[derive(Clone, Copy, Debug)]
pub struct DdgiSettings {
    pub volume: DdgiVolumeSettings,
    pub rays_per_probe: u32,
    pub probes_per_frame: u32,
    pub hysteresis: f32,
    pub normal_bias: f32,
    pub view_bias: f32,
    pub max_ray_distance: f32,
    pub irradiance_resolution: u32,
    pub visibility_resolution: u32,
    pub bounces: u32,
}

impl Default for DdgiSettings {
    fn default() -> Self {
        Self {
            volume: DdgiVolumeSettings::default(),
            rays_per_probe: 64,
            probes_per_frame: 128,
            hysteresis: 0.92,
            normal_bias: 0.08,
            view_bias: 0.20,
            max_ray_distance: 40.0,
            irradiance_resolution: 6,
            visibility_resolution: 6,
            bounces: 2,
        }
    }
}

/// Global illumination settings for the high-level 3D renderer.
#[derive(Clone, Copy, Debug)]
pub struct GlobalIlluminationSettings {
    pub enabled: bool,
    pub ddgi: DdgiSettings,
    pub debug: GiDebugMode,
}

impl Default for GlobalIlluminationSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            ddgi: DdgiSettings::default(),
            debug: GiDebugMode::Off,
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
pub struct RenderSettings {
    pub clear_color: Color,
    pub ambient_color: Color,
    pub global_illumination: GlobalIlluminationSettings,
    pub bloom: BloomSettings,
    pub tonemap: ToneMapSettings,
    pub vignette: VignetteSettings,
}

impl Default for RenderSettings {
    fn default() -> Self {
        Self {
            clear_color: Color::new(0.015, 0.016, 0.02, 1.0),
            ambient_color: Color::new(0.07, 0.075, 0.09, 1.0),
            global_illumination: GlobalIlluminationSettings::default(),
            bloom: BloomSettings::default(),
            tonemap: ToneMapSettings::default(),
            vignette: VignetteSettings::default(),
        }
    }
}
