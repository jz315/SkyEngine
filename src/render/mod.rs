//! SkyEngine high-level rendering facade built around programmable scene pipelines.

pub(crate) mod composer;
pub(crate) mod core;
pub(crate) mod domains;
mod ecs;
pub mod expert;
pub(crate) mod frame_pipeline;
pub(crate) mod graph;
pub(crate) mod internal;
pub(crate) mod light;
pub(crate) mod output_chain;
pub(crate) mod passes;
pub(crate) mod pipeline;
pub(crate) mod postfx;
pub(crate) mod resources;
pub(crate) mod scene;
pub(crate) mod stats;

#[cfg(feature = "live2d")]
pub(crate) mod live2d;

pub use composer::RenderComposer;
pub use core::{camera::Camera2D, color::Color, texture::Texture, viewport::ViewportRect};
pub use domains::{GpuScene2D, PreparedView2D, RenderDomain, SpriteDomain};
pub use ecs::{
    BloomSettings, Camera, CameraViewport, MainCamera, OrderInLayer, Parent, PointLight2D,
    Quaternion, RenderLayerMask, RenderSettings, SortingLayer, SpriteRenderer, ToneMapSettings,
    Transform, VignetteSettings,
};
pub use passes::batch::Sprite;
pub use pipeline::{
    OutputChainConfig, RenderFeature, RenderFeatureExecuteContext, RenderFeatureSetupContext,
    RenderPipelineAsset, RenderPipelineBuilder,
};
pub use scene::{
    Projection, RenderInjectionPoint, RenderOutputFormat, RenderQueueDesc, RenderQueueSort,
    RenderStageKey, RenderStats, SceneView,
};
pub use stats::RenderTimingStats;

#[cfg(feature = "live2d")]
pub use domains::Live2DDomain;
#[cfg(feature = "live2d")]
pub use ecs::Live2DModelInstance;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curated_render_exports_are_available() {
        let _camera = Camera::new();
        let _projection = Projection::orthographic(16.0, 9.0);
        let _color = Color::WHITE;
        let _pipeline = RenderPipelineAsset::universal_unlit();
        let _builder = RenderPipelineAsset::builder();
        let _format_hint = RenderOutputFormat::Preserve;
        let _settings = RenderSettings::default();
        let _sprite = Sprite::new(0.0, 0.0, 1.0, 1.0);
        let _stats = RenderStats::default();
        let _timings = RenderTimingStats::default();
        let _view = CameraViewport::default();
        let _viewport = ViewportRect::default();
        let _transform = Transform::default();
        let _sprite_renderer = SpriteRenderer::new(8.0, 8.0);
        let _light = PointLight2D::new(64.0);
        let _composer = RenderComposer::from_asset(RenderPipelineAsset::overlay());
        let _domain = SpriteDomain::unlit();
        let _main_camera = MainCamera;
        let _sorting_layer = SortingLayer::default();
        let _order_in_layer = OrderInLayer::default();
        let _mask = RenderLayerMask::default();
    }

    #[test]
    fn expert_namespace_exposes_low_level_render_api() {
        let _graph = expert::RenderGraph::new();
        let _target: Option<expert::RenderTarget> = None;
        let _batch: Option<expert::SpriteBatch> = None;
        let _light: Option<expert::LightPass> = None;
        let _composite: Option<expert::CompositePass> = None;
        let _bloom: Option<expert::Bloom> = None;
        let _tonemap: Option<expert::ToneMap> = None;
        let _vignette: Option<expert::Vignette> = None;
    }
}
