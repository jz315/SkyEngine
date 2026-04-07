//! Composable multi-view 2D rendering pipeline.

pub(crate) mod bloom_node;
pub(crate) mod color_resolve_node;
pub(crate) mod composite_node;
pub(crate) mod extractor;
pub(crate) mod light_node;
pub(crate) mod prepared;
mod render_pipeline;
mod scene_cache;
pub(crate) mod sprite_pass;
mod state;
pub(crate) mod tonemap_node;
pub(crate) mod viewport_blit_node;
pub(crate) mod vignette_node;

pub use render_pipeline::RenderPipeline;
pub use state::{FeatureExecutionContext2D, FramePayloads2D, PipelineState2D};

pub(crate) use extractor::SceneExtractor;
pub(crate) use prepared::{PreparedRenderWorld2D, PreparedView2D};
pub(crate) use scene_cache::{SceneCache2D, SceneLightItem, SceneSpriteItem, SceneView2D};
pub(crate) use state::FramePipelineState2D;

pub use bloom_node::BloomNode;
pub(crate) use color_resolve_node::ColorResolveNode;
pub use composite_node::CompositeNode;
pub use light_node::LightNode;
pub use sprite_pass::SpritePass;
pub use tonemap_node::ToneMapNode;
pub use viewport_blit_node::ViewportBlitNode;
pub use vignette_node::VignetteNode;

use crate::gpu::GpuContext;
use crate::render::ecs::RenderSettings2D;
use crate::render::graph::{CompiledPass, PhysicalResources, RenderGraph, RenderGraphError};

/// A composable rendering feature in the 2D pipeline.
pub trait RenderFeature2D: Send {
    /// Unique graph pass name used for dispatch during execution.
    fn name(&self) -> &'static str;

    /// Whether this feature is active for the current frame.
    fn is_enabled(&self, _settings: &RenderSettings2D, _has_surface: bool) -> bool {
        true
    }

    /// Declare graph resources and the pass for this feature.
    fn setup(&mut self, graph: &mut RenderGraph, state: &mut PipelineState2D);

    /// Execute GPU work for this feature.
    fn execute(
        &mut self,
        pass: &CompiledPass,
        ctx: &mut GpuContext,
        resources: &PhysicalResources<'_>,
        execution: &FeatureExecutionContext2D<'_>,
    ) -> Result<(), RenderGraphError>;

    /// Number of draw calls this feature will issue for one executed pass.
    fn draw_calls(&self, _execution: &FeatureExecutionContext2D<'_>) -> usize {
        0
    }

    /// Update internal runtime parameters from render settings.
    fn apply_settings(&mut self, _settings: &RenderSettings2D) {}

    /// Notify the feature that the surface size changed.
    fn resize(&mut self, _ctx: &GpuContext, _width: u32, _height: u32) {}

    #[cfg(test)]
    fn debug_last_light_ambient(&self) -> Option<[f32; 4]> {
        None
    }
}
