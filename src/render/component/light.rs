use crate::render::Color;

pub const MAX_DIRECTIONAL_SHADOW_CASCADES: usize = 4;

/// Directional shadow filtering mode.
///
/// Fixed PCF is the deterministic default. Dithered PCF rotates the same
/// Vogel disk per pixel, and PCSS adds a blocker search before filtering.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ShadowSamplingMode {
    #[default]
    FixedPcf,
    DitheredPcf,
    Pcss,
}

impl ShadowSamplingMode {
    #[inline]
    pub(crate) const fn shader_code(self) -> f32 {
        match self {
            Self::FixedPcf => 0.0,
            Self::DitheredPcf => 1.0,
            Self::Pcss => 2.0,
        }
    }
}

/// Directional shadow atlas update policy.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ShadowUpdatePolicy {
    #[default]
    EveryFrame,
    StaticWhenUnchanged,
}

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

/// High-level spot light component for the programmable 3D renderer.
#[derive(Clone, Copy, Debug)]
pub struct SpotLight {
    pub radius: f32,
    pub intensity: f32,
    pub color: Color,
    pub temperature: f32,
    pub falloff: f32,
    pub direction: [f32; 3],
    pub inner_angle: f32,
    pub outer_angle: f32,
    pub visible: bool,
    pub layer_mask: u32,
    pub casts_shadows: bool,
    pub shadow_resolution: u32,
    pub shadow_bias: f32,
    pub shadow_depth_bias: i32,
    pub shadow_slope_bias: f32,
    pub shadow_normal_bias: f32,
    pub shadow_filter_radius: f32,
    pub shadow_sampling_mode: ShadowSamplingMode,
    pub shadow_update_policy: ShadowUpdatePolicy,
}

impl SpotLight {
    #[inline]
    pub const fn new(radius: f32) -> Self {
        Self {
            radius,
            intensity: 1.0,
            color: Color::WHITE,
            temperature: 6500.0,
            falloff: 2.0,
            direction: [0.0, -1.0, 0.0],
            inner_angle: 0.34906584,
            outer_angle: 0.55850536,
            visible: true,
            layer_mask: u32::MAX,
            casts_shadows: true,
            shadow_resolution: 1024,
            shadow_bias: 0.0015,
            shadow_depth_bias: 2,
            shadow_slope_bias: 2.0,
            shadow_normal_bias: 0.0,
            shadow_filter_radius: 0.025,
            shadow_sampling_mode: ShadowSamplingMode::FixedPcf,
            shadow_update_policy: ShadowUpdatePolicy::EveryFrame,
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
    pub const fn direction(mut self, direction: [f32; 3]) -> Self {
        self.direction = direction;
        self
    }

    #[inline]
    pub const fn cone_angles(mut self, inner_angle: f32, outer_angle: f32) -> Self {
        self.inner_angle = inner_angle;
        self.outer_angle = outer_angle;
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
    pub const fn shadow_resolution(mut self, shadow_resolution: u32) -> Self {
        self.shadow_resolution = if shadow_resolution == 0 {
            1
        } else {
            shadow_resolution
        };
        self
    }

    #[inline]
    pub const fn shadow_bias(mut self, shadow_bias: f32) -> Self {
        self.shadow_bias = shadow_bias;
        self
    }

    #[inline]
    pub const fn shadow_depth_bias(mut self, shadow_depth_bias: i32) -> Self {
        self.shadow_depth_bias = shadow_depth_bias;
        self
    }

    #[inline]
    pub const fn shadow_slope_bias(mut self, shadow_slope_bias: f32) -> Self {
        self.shadow_slope_bias = shadow_slope_bias;
        self
    }

    #[inline]
    pub const fn shadow_normal_bias(mut self, shadow_normal_bias: f32) -> Self {
        self.shadow_normal_bias = shadow_normal_bias;
        self
    }

    #[inline]
    pub const fn shadow_filter_radius(mut self, shadow_filter_radius: f32) -> Self {
        self.shadow_filter_radius = shadow_filter_radius;
        self
    }

    #[inline]
    pub const fn shadow_sampling_mode(mut self, shadow_sampling_mode: ShadowSamplingMode) -> Self {
        self.shadow_sampling_mode = shadow_sampling_mode;
        self
    }

    #[inline]
    pub const fn shadow_update_policy(mut self, shadow_update_policy: ShadowUpdatePolicy) -> Self {
        self.shadow_update_policy = shadow_update_policy;
        self
    }

    #[inline]
    pub fn resolved_cone_cosines(self) -> [f32; 2] {
        let outer = self.outer_angle.max(0.001);
        let inner = self.inner_angle.clamp(0.0, outer - 0.0001);
        [inner.cos(), outer.cos()]
    }
}

impl Default for SpotLight {
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
    pub radius: f32,
    pub visible: bool,
    pub layer_mask: u32,
    pub casts_shadows: bool,
    pub shadow_map_size: u32,
    pub shadow_bias: f32,
    pub shadow_depth_bias: i32,
    pub shadow_slope_bias: f32,
    pub shadow_normal_bias: f32,
    pub shadow_filter_radius: f32,
    pub cascade_count: u32,
    pub cascade_distances: [f32; MAX_DIRECTIONAL_SHADOW_CASCADES],
    pub cascade_blend: f32,
    pub shadow_resolution_per_cascade: u32,
    pub shadow_sampling_mode: ShadowSamplingMode,
    pub shadow_update_policy: ShadowUpdatePolicy,
}

impl DirectionalLight {
    #[inline]
    pub const fn new(direction: [f32; 3]) -> Self {
        Self {
            direction,
            intensity: 1.0,
            color: Color::WHITE,
            radius: 0.025,
            visible: true,
            layer_mask: u32::MAX,
            casts_shadows: true,
            shadow_map_size: 2048,
            shadow_bias: 0.0015,
            shadow_depth_bias: 2,
            shadow_slope_bias: 2.0,
            shadow_normal_bias: 0.0,
            shadow_filter_radius: 0.025,
            cascade_count: 1,
            cascade_distances: [0.0; MAX_DIRECTIONAL_SHADOW_CASCADES],
            cascade_blend: 0.0,
            shadow_resolution_per_cascade: 2048,
            shadow_sampling_mode: ShadowSamplingMode::FixedPcf,
            shadow_update_policy: ShadowUpdatePolicy::EveryFrame,
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
    pub const fn radius(mut self, radius: f32) -> Self {
        self.radius = radius;
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
        self.shadow_resolution_per_cascade = self.shadow_map_size;
        self
    }

    #[inline]
    pub const fn shadow_resolution_per_cascade(
        mut self,
        shadow_resolution_per_cascade: u32,
    ) -> Self {
        self.shadow_resolution_per_cascade = if shadow_resolution_per_cascade == 0 {
            1
        } else {
            shadow_resolution_per_cascade
        };
        self
    }

    #[inline]
    pub const fn cascade_count(mut self, cascade_count: u32) -> Self {
        self.cascade_count = if cascade_count == 0 {
            1
        } else if cascade_count > MAX_DIRECTIONAL_SHADOW_CASCADES as u32 {
            MAX_DIRECTIONAL_SHADOW_CASCADES as u32
        } else {
            cascade_count
        };
        self
    }

    #[inline]
    pub const fn cascade_distances(
        mut self,
        cascade_distances: [f32; MAX_DIRECTIONAL_SHADOW_CASCADES],
    ) -> Self {
        self.cascade_distances = cascade_distances;
        self
    }

    #[inline]
    pub const fn cascade_blend(mut self, cascade_blend: f32) -> Self {
        self.cascade_blend = cascade_blend;
        self
    }

    #[inline]
    pub const fn shadow_bias(mut self, shadow_bias: f32) -> Self {
        self.shadow_bias = shadow_bias;
        self
    }

    #[inline]
    pub const fn shadow_depth_bias(mut self, shadow_depth_bias: i32) -> Self {
        self.shadow_depth_bias = shadow_depth_bias;
        self
    }

    #[inline]
    pub const fn shadow_slope_bias(mut self, shadow_slope_bias: f32) -> Self {
        self.shadow_slope_bias = shadow_slope_bias;
        self
    }

    #[inline]
    pub const fn shadow_normal_bias(mut self, shadow_normal_bias: f32) -> Self {
        self.shadow_normal_bias = shadow_normal_bias;
        self
    }

    #[inline]
    pub const fn shadow_filter_radius(mut self, shadow_filter_radius: f32) -> Self {
        self.shadow_filter_radius = shadow_filter_radius;
        self
    }

    #[inline]
    pub const fn shadow_sampling_mode(mut self, shadow_sampling_mode: ShadowSamplingMode) -> Self {
        self.shadow_sampling_mode = shadow_sampling_mode;
        self
    }

    #[inline]
    pub const fn fixed_pcf_shadows(mut self) -> Self {
        self.shadow_sampling_mode = ShadowSamplingMode::FixedPcf;
        self
    }

    #[inline]
    pub const fn dithered_pcf_shadows(mut self) -> Self {
        self.shadow_sampling_mode = ShadowSamplingMode::DitheredPcf;
        self
    }

    #[inline]
    pub const fn pcss_shadows(mut self) -> Self {
        if self.shadow_filter_radius < 0.05 {
            self.shadow_filter_radius = 0.05;
        }
        if self.shadow_normal_bias < 0.002 {
            self.shadow_normal_bias = 0.002;
        }
        self.shadow_sampling_mode = ShadowSamplingMode::Pcss;
        self
    }

    #[inline]
    pub const fn shadow_update_policy(mut self, shadow_update_policy: ShadowUpdatePolicy) -> Self {
        self.shadow_update_policy = shadow_update_policy;
        self
    }

    #[inline]
    pub const fn update_shadows_every_frame(mut self) -> Self {
        self.shadow_update_policy = ShadowUpdatePolicy::EveryFrame;
        self
    }

    #[inline]
    pub const fn static_shadows_when_unchanged(mut self) -> Self {
        self.shadow_update_policy = ShadowUpdatePolicy::StaticWhenUnchanged;
        self
    }

    #[inline]
    pub const fn sharp_shadows(mut self) -> Self {
        self.shadow_filter_radius = 0.0;
        self.shadow_depth_bias = 2;
        self.shadow_slope_bias = 1.0;
        self.shadow_normal_bias = 0.0;
        self.shadow_sampling_mode = ShadowSamplingMode::FixedPcf;
        self
    }

    #[inline]
    pub const fn soft_shadows(mut self) -> Self {
        self.shadow_filter_radius = 0.05;
        self.shadow_depth_bias = 2;
        self.shadow_slope_bias = 2.0;
        self.shadow_normal_bias = 0.0;
        self.shadow_sampling_mode = ShadowSamplingMode::DitheredPcf;
        self
    }

    #[inline]
    pub const fn contact_safe_shadows(mut self) -> Self {
        if self.shadow_filter_radius < 0.025 {
            self.shadow_filter_radius = 0.025;
        }
        if self.shadow_bias < 0.0025 {
            self.shadow_bias = 0.0025;
        }
        if self.shadow_depth_bias < 4 {
            self.shadow_depth_bias = 4;
        }
        if self.shadow_slope_bias < 3.0 {
            self.shadow_slope_bias = 3.0;
        }
        if self.shadow_normal_bias < 0.02 {
            self.shadow_normal_bias = 0.02;
        }
        self.shadow_sampling_mode = ShadowSamplingMode::Pcss;
        self
    }
}

impl Default for DirectionalLight {
    fn default() -> Self {
        Self::new([0.0, -1.0, 0.0])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directional_shadow_bias_defaults_match_previous_pipeline_state() {
        let light = DirectionalLight::default();

        assert_eq!(light.shadow_depth_bias, 2);
        assert_eq!(light.shadow_slope_bias, 2.0);
        assert_eq!(light.shadow_normal_bias, 0.0);
        assert_eq!(light.radius, 0.025);
        assert_eq!(light.shadow_filter_radius, 0.025);
        assert_eq!(light.cascade_count, 1);
        assert_eq!(light.shadow_resolution_per_cascade, 2048);
        assert_eq!(light.shadow_sampling_mode, ShadowSamplingMode::FixedPcf);
        assert_eq!(light.shadow_update_policy, ShadowUpdatePolicy::EveryFrame);
    }

    #[test]
    fn directional_light_radius_does_not_change_shadow_filter_radius() {
        let light = DirectionalLight::default().radius(0.05);

        assert_eq!(light.radius, 0.05);
        assert_eq!(light.shadow_filter_radius, 0.025);
    }

    #[test]
    fn directional_light_cascade_count_is_clamped_to_fixed_contract() {
        let light = DirectionalLight::default()
            .cascade_count(99)
            .cascade_distances([12.0, 24.0, 48.0, 96.0])
            .cascade_blend(0.2)
            .shadow_resolution_per_cascade(1024);

        assert_eq!(light.cascade_count, MAX_DIRECTIONAL_SHADOW_CASCADES as u32);
        assert_eq!(light.cascade_distances, [12.0, 24.0, 48.0, 96.0]);
        assert_eq!(light.cascade_blend, 0.2);
        assert_eq!(light.shadow_resolution_per_cascade, 1024);
    }

    #[test]
    fn directional_shadow_sampling_presets_select_expected_modes() {
        assert_eq!(
            DirectionalLight::default()
                .sharp_shadows()
                .shadow_sampling_mode,
            ShadowSamplingMode::FixedPcf
        );
        assert_eq!(
            DirectionalLight::default()
                .soft_shadows()
                .shadow_sampling_mode,
            ShadowSamplingMode::DitheredPcf
        );
        assert_eq!(
            DirectionalLight::default()
                .contact_safe_shadows()
                .shadow_sampling_mode,
            ShadowSamplingMode::Pcss
        );
        let pcss = DirectionalLight::default().pcss_shadows();
        assert_eq!(pcss.shadow_sampling_mode, ShadowSamplingMode::Pcss);
        assert!(pcss.shadow_filter_radius >= 0.05);
        assert!(pcss.shadow_normal_bias >= 0.002);
        assert_eq!(ShadowSamplingMode::Pcss.shader_code(), 2.0);
    }

    #[test]
    fn directional_shadow_update_policy_is_explicit() {
        assert_eq!(
            DirectionalLight::default()
                .static_shadows_when_unchanged()
                .shadow_update_policy,
            ShadowUpdatePolicy::StaticWhenUnchanged
        );
        assert_eq!(
            DirectionalLight::default()
                .static_shadows_when_unchanged()
                .update_shadows_every_frame()
                .shadow_update_policy,
            ShadowUpdatePolicy::EveryFrame
        );
    }

    #[test]
    fn spot_light_defaults_are_shadow_ready() {
        let light = SpotLight::default();

        assert!(light.casts_shadows);
        assert_eq!(light.shadow_resolution, 1024);
        assert_eq!(light.shadow_sampling_mode, ShadowSamplingMode::FixedPcf);
        assert_eq!(light.shadow_update_policy, ShadowUpdatePolicy::EveryFrame);
        assert_eq!(light.resolved_cone_cosines()[0], light.inner_angle.cos());
        assert!(light.resolved_cone_cosines()[0] > light.resolved_cone_cosines()[1]);
    }
}
