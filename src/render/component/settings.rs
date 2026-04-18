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

/// Screen-space diffuse GI controls for the high-level 3D renderer.
#[derive(Clone, Copy, Debug)]
pub struct ScreenSpaceGiSettings {
    pub enabled: bool,
    pub intensity: f32,
    pub radius_px: f32,
    pub depth_reject: f32,
    pub normal_reject: f32,
    pub falloff: f32,
}

impl Default for ScreenSpaceGiSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            intensity: 0.22,
            radius_px: 18.0,
            depth_reject: 3.0,
            normal_reject: 8.0,
            falloff: 0.55,
        }
    }
}

/// World-space probe volume controls for hybrid GI.
#[derive(Clone, Copy, Debug)]
pub struct ProbeVolumeGiSettings {
    pub counts: [u32; 3],
    pub spacing: f32,
    pub proxy_radius_scale: f32,
    pub bounce_strength: f32,
    pub emissive_strength: f32,
    pub light_injection: f32,
}

impl Default for ProbeVolumeGiSettings {
    fn default() -> Self {
        Self {
            counts: [10, 6, 10],
            spacing: 4.0,
            proxy_radius_scale: 1.4,
            bounce_strength: 0.65,
            emissive_strength: 1.5,
            light_injection: 0.9,
        }
    }
}

/// Unified GI controls for the high-level 3D renderer.
#[derive(Clone, Copy, Debug)]
pub struct GlobalIlluminationSettings {
    pub enabled: bool,
    pub intensity: f32,
    pub probe_strength: f32,
    pub detail_strength: f32,
    pub occlusion_strength: f32,
    pub sky_boost: f32,
    pub probe_volume: ProbeVolumeGiSettings,
    pub detail: ScreenSpaceGiSettings,
}

impl Default for GlobalIlluminationSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            intensity: 0.55,
            probe_strength: 0.9,
            detail_strength: 0.3,
            occlusion_strength: 0.24,
            sky_boost: 1.1,
            probe_volume: ProbeVolumeGiSettings::default(),
            detail: ScreenSpaceGiSettings {
                enabled: true,
                intensity: 0.25,
                radius_px: 18.0,
                depth_reject: 4.0,
                normal_reject: 12.0,
                falloff: 0.6,
            },
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
