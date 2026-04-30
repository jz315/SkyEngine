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

/// Runtime GI algorithm selected by the high-level 3D renderer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GlobalIlluminationMode {
    #[default]
    Off,
    Ssgi,
    Ddgi,
}

/// Screen-space GI controls for the Wicked-inspired modern 3D pipeline.
#[derive(Clone, Copy, Debug)]
pub struct SsgiSettings {
    /// Final composite strength. Wicked applies SSGI as a separate indirect term;
    /// `1.0` preserves that energy in SkyEngine's current post-composite path.
    pub intensity: f32,
    /// Public radius hint. The current WGSL pass maps `8.0` to Wicked's narrow
    /// `range = 2, spread = 2` SSGI sampling pass.
    pub radius_pixels: f32,
    /// Wicked's SSGI depth rejection distance; the shader uses its reciprocal.
    pub depth_rejection: f32,
    /// Wicked's bilateral normal threshold for SSGI upsample-style rejection.
    pub normal_power: f32,
}

impl Default for SsgiSettings {
    fn default() -> Self {
        Self {
            intensity: 1.0,
            radius_pixels: 8.0,
            depth_rejection: 8.0,
            normal_power: 64.0,
        }
    }
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
    pub mode: GlobalIlluminationMode,
    pub ssgi: SsgiSettings,
    pub ddgi: DdgiSettings,
    pub debug: GiDebugMode,
}

impl Default for GlobalIlluminationSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: GlobalIlluminationMode::Off,
            ssgi: SsgiSettings::default(),
            ddgi: DdgiSettings::default(),
            debug: GiDebugMode::Off,
        }
    }
}

impl GlobalIlluminationSettings {
    #[inline]
    pub fn effective_mode(self) -> GlobalIlluminationMode {
        if !self.enabled {
            return GlobalIlluminationMode::Off;
        }

        match self.mode {
            // Compatibility: older callers only toggled `enabled`; keep that
            // path using the existing DDGI implementation.
            GlobalIlluminationMode::Off => GlobalIlluminationMode::Ddgi,
            mode => mode,
        }
    }

    #[inline]
    pub fn uses_ssgi(self) -> bool {
        self.effective_mode() == GlobalIlluminationMode::Ssgi
    }

    #[inline]
    pub fn uses_ddgi(self) -> bool {
        self.effective_mode() == GlobalIlluminationMode::Ddgi
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

/// Image sharpening controls for the high-level renderer.
#[derive(Clone, Copy, Debug)]
pub struct SharpenSettings {
    pub enabled: bool,
    pub strength: f32,
    pub clamp: f32,
}

impl Default for SharpenSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            strength: 0.25,
            clamp: 0.08,
        }
    }
}

/// Temporal anti-aliasing controls for the Wicked-inspired modern 3D pipeline.
#[derive(Clone, Copy, Debug)]
pub struct TemporalAntiAliasingSettings {
    pub enabled: bool,
    pub feedback: f32,
    pub jitter_scale: f32,
    pub history_clamp: f32,
    pub sharpen_amount: f32,
}

impl Default for TemporalAntiAliasingSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            feedback: 0.05,
            jitter_scale: 1.0,
            history_clamp: 0.0,
            sharpen_amount: 0.0,
        }
    }
}

/// Renderer intermediate-buffer debug view.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RenderDebugView {
    #[default]
    None,
    SceneColor,
    SceneDepth,
    SceneNormal,
    Albedo,
    Roughness,
    Metallic,
    Emissive,
    Velocity,
    Light,
    IndirectDiffuse,
    DirectionalShadowMap,
    DirectionalShadowCascade(u32),
    DirectionalShadowCoverage,
    SsgiDiffuseMip(u32),
    SsgiAtlasLayer {
        mip: u32,
        layer: u32,
    },
}

impl RenderDebugView {
    #[inline]
    pub const fn is_enabled(self) -> bool {
        !matches!(self, Self::None)
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
    pub temporal_aa: TemporalAntiAliasingSettings,
    pub sharpen: SharpenSettings,
    pub debug_view: RenderDebugView,
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
            temporal_aa: TemporalAntiAliasingSettings::default(),
            sharpen: SharpenSettings::default(),
            debug_view: RenderDebugView::default(),
            vignette: VignetteSettings::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ssgi_defaults_match_wicked_depth_rejection_path() {
        let settings = SsgiSettings::default();

        assert_eq!(settings.intensity, 1.0);
        assert_eq!(settings.radius_pixels, 8.0);
        assert_eq!(settings.depth_rejection, 8.0);
        assert_eq!(settings.normal_power, 64.0);
    }

    #[test]
    fn taa_settings_default_disabled() {
        let settings = TemporalAntiAliasingSettings::default();

        assert!(!settings.enabled);
        assert_eq!(settings.feedback, 0.05);
        assert_eq!(settings.jitter_scale, 1.0);
        assert_eq!(settings.history_clamp, 0.0);
        assert_eq!(settings.sharpen_amount, 0.0);
    }

    #[test]
    fn render_debug_view_default_disabled() {
        let settings = RenderSettings::default();

        assert_eq!(settings.debug_view, RenderDebugView::None);
        assert!(!settings.debug_view.is_enabled());
        assert!(RenderDebugView::SceneDepth.is_enabled());
        assert!(RenderDebugView::DirectionalShadowMap.is_enabled());
        assert!(RenderDebugView::DirectionalShadowCascade(1).is_enabled());
        assert!(RenderDebugView::DirectionalShadowCoverage.is_enabled());
    }
}
