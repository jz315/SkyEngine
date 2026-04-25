//! SkyEngine high-level rendering facade built around programmable scene pipelines.

pub(crate) mod component;
pub(crate) mod composite;
pub(crate) mod execution;
pub mod expert;
pub(crate) mod extract;
pub(crate) mod gpu;
pub(crate) mod graph;
pub(crate) mod lighting;
pub(crate) mod mesh;
pub(crate) mod phase;
pub(crate) mod pipeline;
pub(crate) mod postfx;
pub(crate) mod resources;
pub(crate) mod runtime;
pub(crate) mod sprite;
pub(crate) mod view;

#[cfg(feature = "live2d")]
pub(crate) mod live2d;

pub use crate::math::{Projection, Quat, Transform};
pub use component::{
    BloomSettings, Camera as CameraMarker, CameraViewport, DirectionalLight,
    GlobalIlluminationSettings, MainCamera, MeshRenderer, OrderInLayer, Parent, PointLight,
    ProbeVolumeGiSettings, RenderLayerMask, RenderSettings, ScreenSpaceGiSettings, SortingLayer,
    SpriteRenderer, ToneMapSettings, VignetteSettings,
};
pub use gpu::{
    is_depth_format, GpuScene, GpuTable, GpuTableManager, ModelMatrixTable, Texture,
    DEFAULT_DEPTH_FORMAT,
};
pub use lighting::DirectionalShadowPhase;
pub use lighting::{GpuLight, Light2D, LightTable};
pub use phase::{OpaquePhase, TransparentPhase};
pub use pipeline::{
    Bloom, ComputePass, ComputePassExecuteContext, ComputePassSetupContext, GlobalIllumination,
    PipelineStepDescriptor, PostFxPass, PostFxPassExecuteContext, PostFxPassSetupContext,
    RenderFeature, RenderPass, RenderPassExecuteContext, RenderPassSetupContext, RenderPhase,
    RenderPhaseExecuteContext, RenderPhaseSetupContext, RenderPipelineAsset, RenderPipelineBuilder,
    RenderPipelineDescriptor, SceneMaterialPrepass, SceneNormalPrepass, SpriteFeature, ToneMap,
    Vignette,
};
pub use resources::material::{
    AlphaMode, Material, MaterialBindContext, MaterialHandle, MaterialRenderState, MaterialStorage,
    SceneBindingDesc, SceneBindingKind, ShaderSource, SpriteMaterial, StandardMaterial,
    UnlitMaterial,
};
pub use runtime::RenderComposer;
pub use runtime::RenderTimingStats;
pub use sprite::Sprite;
pub use view::{
    Camera, Color, Frustum, RenderQueueSort, RenderStats, SceneView, SceneViewKind, ViewportRect,
};

#[cfg(feature = "live2d")]
pub use component::{Live2DAnimator, Live2DCommand, Live2DCommands, Live2DModelInstance};
#[cfg(feature = "live2d")]
pub use pipeline::Live2DFeature;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curated_render_exports_are_available() {
        let _camera = Camera::new(16.0, 9.0);
        let _camera_marker = CameraMarker::new();
        let _projection = Projection::orthographic(9.0);
        let _color = Color::WHITE;
        let _pipeline = RenderPipelineAsset::builder()
            .add_feature(SpriteFeature::unlit())
            .add_phase(TransparentPhase::new())
            .build();
        let _builder = RenderPipelineAsset::builder();
        let _settings = RenderSettings::default();
        let _sprite = Sprite::new(0.0, 0.0, 1.0, 1.0);
        let _stats = RenderStats::default();
        let _timings = RenderTimingStats::default();
        let _view = CameraViewport::default();
        let _viewport = ViewportRect::default();
        let _transform = Transform::default();
        let _material_state = MaterialRenderState::transparent();
        let _sprite_material = SpriteMaterial::default();
        let _unlit_material = UnlitMaterial::default();
        let _standard_material = StandardMaterial::default();
        let _depth_format = DEFAULT_DEPTH_FORMAT;
        let _opaque_phase = expert::OpaquePhase::new();
        let _mesh_renderer = MeshRenderer::new(
            expert::Mesh::QUAD,
            expert::MaterialHandle::new::<SpriteMaterial>(0, 0),
        );
        let _sprite_renderer = SpriteRenderer::new(8.0, 8.0);
        let _light = PointLight::new(64.0);
        let _composer = RenderComposer::from_asset(RenderPipelineAsset::builder().build());
        let _feature = SpriteFeature::unlit();
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
