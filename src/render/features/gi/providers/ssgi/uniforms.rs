use super::contract::SsgiPassKind;
use super::settings::SsgiSettings;
use crate::math::Mat4;
use crate::render::view::SceneView;

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct SsgiUniform {
    pub(crate) params0: [f32; 4],
    pub(crate) params1: [f32; 4],
    pub(crate) params2: [f32; 4],
    pub(crate) inverse_projection: [f32; 16],
}

impl SsgiUniform {
    pub(crate) fn for_pass(
        settings: SsgiSettings,
        scene_view: &SceneView,
        pass_kind: SsgiPassKind,
    ) -> Self {
        let inverse_projection = Mat4::from_cols_array(scene_view.unjittered_projection_matrix)
            .inverse()
            .to_cols_array();
        let pass_settings = pass_kind.settings();
        let range = if pass_kind.is_compute() {
            pass_settings
                .range
                .max((settings.radius_pixels * 0.25).clamp(1.0, 3.0))
        } else {
            pass_settings.range
        };
        let spread = pass_settings.spread;
        let range_spread = (range * spread).max(1.0);
        let final_marker = if pass_kind.is_final() { 1.0 } else { 0.0 };

        Self {
            params0: [
                settings.intensity.max(0.0),
                range,
                spread,
                settings.depth_rejection.max(0.001).recip(),
            ],
            params1: [
                range_spread.recip() * range_spread.recip(),
                settings.normal_power.max(0.001),
                0.96,
                0.02,
            ],
            params2: [
                pass_kind.output_scale() as f32,
                pass_kind.source_scale() as f32,
                final_marker,
                0.0,
            ],
            inverse_projection,
        }
    }
}
