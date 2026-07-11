//! Lighting, directional shadows, and scene composition.

mod component;
pub mod composite;
pub mod renderer;

pub use component::{
    DirectionalLight, PointLight, ShadowSamplingMode, ShadowUpdatePolicy, SpotLight,
    MAX_DIRECTIONAL_SHADOW_CASCADES,
};
pub use composite::*;
pub use renderer::*;

use crate::render::pipeline::{RenderFeature, RenderPipelineBuilder};

/// Installs feature-owned directional shadow frame preparation.
pub struct LightingFeature;

impl RenderFeature for LightingFeature {
    fn name(&self) -> &'static str {
        "lighting"
    }

    fn register(&mut self, builder: &mut RenderPipelineBuilder) {
        let current = std::mem::take(builder);
        *builder = current.add_frame_extension(renderer::shadow::ShadowFrameExtension::default());
    }
}
