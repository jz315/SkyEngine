use crate::render::gi::GiProviderConfig;
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
    pub intensity: f32,
    pub spread: f32,
}

impl Default for BloomSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            intensity: 1.0,
            spread: 1.0,
        }
    }
}

/// Global illumination provider selected by the high-level renderer.
#[derive(Clone, Debug, Default)]
pub enum GlobalIllumination {
    #[default]
    Off,
    Provider(GiProviderConfig),
}

impl GlobalIllumination {
    #[inline]
    pub fn provider(config: GiProviderConfig) -> Self {
        Self::Provider(config)
    }

    #[inline]
    pub const fn is_off(&self) -> bool {
        matches!(self, Self::Off)
    }
}

/// Photon-style screen-space contact shadow and horizon AO controls.
#[derive(Clone, Copy, Debug)]
pub struct ContactShadowsSettings {
    pub enabled: bool,
    pub intensity: f32,
    pub max_distance: f32,
    pub thickness: f32,
    pub ray_steps: u32,
    pub ao_intensity: f32,
    pub ao_radius_pixels: f32,
    pub ao_steps: u32,
}

impl Default for ContactShadowsSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            intensity: 0.72,
            max_distance: 2.6,
            thickness: 0.18,
            ray_steps: 10,
            ao_intensity: 0.46,
            ao_radius_pixels: 18.0,
            ao_steps: 3,
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
    DirectionalShadowSplitCoverage,
    DirectionalShadowFade,
    DirectionalShadowCompareDelta,
    DirectionalShadowBias,
    DirectionalShadowPcss,
    DirectLighting,
    IndirectLighting,
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
#[derive(Clone, Debug)]
pub struct RenderSettings {
    pub clear_color: Color,
    pub ambient_color: Color,
    pub global_illumination: GlobalIllumination,
    pub contact_shadows: ContactShadowsSettings,
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
            global_illumination: GlobalIllumination::default(),
            contact_shadows: ContactShadowsSettings::default(),
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
    fn contact_shadow_defaults_match_photon_style_ssrt_path() {
        let settings = ContactShadowsSettings::default();

        assert!(settings.enabled);
        assert_eq!(settings.ray_steps, 10);
        assert_eq!(settings.ao_steps, 3);
        assert_eq!(settings.max_distance, 2.6);
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
        assert!(RenderDebugView::DirectionalShadowSplitCoverage.is_enabled());
        assert!(RenderDebugView::DirectionalShadowFade.is_enabled());
        assert!(RenderDebugView::DirectionalShadowCompareDelta.is_enabled());
        assert!(RenderDebugView::DirectionalShadowBias.is_enabled());
        assert!(RenderDebugView::DirectionalShadowPcss.is_enabled());
        assert!(RenderDebugView::DirectLighting.is_enabled());
        assert!(RenderDebugView::IndirectLighting.is_enabled());
    }
}
