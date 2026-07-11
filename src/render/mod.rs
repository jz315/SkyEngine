//! SkyEngine high-level rendering facade built around programmable scene pipelines.
//!
//! This module is the default entry point for application and gameplay code.
//! Its exports are intentionally grouped by audience:
//!
//! - stable gameplay API: cameras, colors, render components, sprites,
//!   tilemaps, lights, renderer backends, pipeline assets, and runtime stats;
//! - advanced extension API: feature, phase, pass, post-fx, material, and shader
//!   registration traits used by custom renderer families;
//! - compatibility exports for lower-level execution types while the render
//!   stack is still settling.
//!
//! New code that needs direct access to frame execution, render graph resources,
//! draw functions, GPU tables, low-level meshes, or renderer-owned caches should
//! prefer [`expert`]. The top-level facade should answer "how do I render my
//! scene?", while [`expert`] answers "how do I participate in the renderer
//! internals?".

pub(crate) mod core;
pub mod expert;
pub mod features;
pub(crate) mod integration;

// Transitional crate-internal names keep existing implementation modules
// compiling while callers are migrated to the new core/features/integration
// topology. They are deliberately not public API.
pub(crate) use core::draw as phase;
pub(crate) use core::execution;
pub(crate) use core::extraction as extract;
pub(crate) use core::gpu;
pub(crate) use core::graph;
pub(crate) use core::pipeline;
pub(crate) use core::resources;
pub(crate) use core::runtime;
pub(crate) use core::scene::color;
pub(crate) use core::view;
pub(crate) use features::gi;
pub(crate) use features::lighting;
#[cfg(feature = "live2d")]
pub(crate) use features::live2d;
pub(crate) use features::postfx;
pub(crate) use features::sprite;
pub(crate) use features::sprite::animation;
pub(crate) use integration::assets as asset;
pub(crate) use integration::backend;

pub use crate::math::{Projection, Quat, Transform};
pub use animation::{animate_sprites, SpriteAnimationClip, SpriteAnimationFrame, SpriteAnimator};
pub use asset::{
    register_render_asset_factories, register_render_cookers, render_cook_registry, MeshAsset,
    MeshAssetDescriptor, MeshAssetError, MeshBoundingSphere, MeshIndexData, MeshSubMesh,
    MeshVertexAttribute, MeshVertexFormat, MeshVertexLayout, MeshVertexSemantic, RenderAssets,
    StandardMaterialAsset, TextureAddressMode, TextureFilter, TextureSamplerDesc,
};
#[cfg(feature = "kajiya-renderer")]
pub use backend::{KajiyaSceneRenderer, KajiyaSceneSyncStats};
#[cfg(feature = "renderling-renderer")]
pub use backend::{RenderlingSceneRenderer, RenderlingSceneSyncStats};
pub use backend::{
    SceneCamera, SceneDirectionalLight, SceneFrame, SceneFrameClearReason, SceneFrameSkipReason,
    SceneMeshInstance, ScenePointLight, SceneRenderOutcome, SceneRenderer, SceneRendererError,
    SceneRendererInitError, SceneSnapshot, SceneSnapshotExtractor, SceneSnapshotStats,
    SceneSpotLight, WgpuSceneRenderer,
};
pub use color::Color;
pub use core::scene::{
    BloomSettings, Camera as CameraMarker, CameraViewport, ContactShadowsSettings,
    GlobalIllumination, MainCamera, Parent, RenderDebugView, RenderLayerMask, RenderSettings,
    SharpenSettings, TemporalAntiAliasingSettings, ToneMapSettings, VignetteSettings,
};
pub use execution::SceneTexture;
pub use execution::{
    ComputePassExecuteContext, ComputePassSetupContext, GraphPassExecuteContext,
    GraphPassSetupContext, PhaseDrawServices, PhaseExecuteContext, PhaseSetupContext,
    PostFxPassExecuteContext, PostFxPassSetupContext, RenderPassExecuteContext,
    RenderPassSetupContext,
};
pub use features::lighting::{
    DirectionalLight, PointLight, ShadowSamplingMode, ShadowUpdatePolicy, SpotLight,
};
pub use features::mesh::{MeshRenderer, WgpuMeshRenderer, ALL_SHADOW_CASCADE_MASK};
pub use features::sprite::{SortingLayer, SpriteRenderer};
pub use gpu::{
    is_depth_format, GpuScene, GpuTable, GpuTableManager, ModelMatrixTable, Texture,
    DEFAULT_DEPTH_FORMAT,
};
pub use lighting::DirectionalShadowPhase;
pub use lighting::{GpuLight, GpuLightKind, Light2D, LightTable, SceneLightingResources};
pub use phase::{OpaquePhase, TransparentPhase};
pub use pipeline::{
    ComputePass, GraphPass, KajiyaDpiMode, KajiyaRendererSettings, PipelineStepDescriptor,
    PostFxPass, RenderBackendKind, RenderFeature, RenderPass, RenderPhase, RenderPipelineAsset,
    RenderPipelineBuilder, RenderPipelineDescriptor, TextureSpec,
};
pub use resources::material::{
    AlphaMode, MainPassMode, Material, MaterialBinding, MaterialBindingLayout, MaterialError,
    MaterialHandle, MaterialInterface, MaterialPassSet, MaterialPrepareContext,
    MaterialPrepassMode, MaterialRenderState, MaterialShaderSet, PreparedMaterial,
    SceneBindingDesc, SceneBindingKind, SceneResourceKind, SceneResourceRequirements, ShaderSource,
    ShaderVariantKey, ShaderVariantPolicy, ShadowPassMode, SpriteMaterial, StandardMaterial,
    UnlitMaterial,
};
pub use resources::texture_cache::{SharedRenderAssetCache, TextureReadiness};
pub use resources::MAX_DIRECTIONAL_SHADOW_CASCADES;
pub use runtime::RenderRuntime;
pub use runtime::RenderTimingStats;
pub use runtime::{FrameRenderOutcome, FrameSkipReason};
pub use runtime::{HistoryTexture, HistoryTextureRequest, HistoryTextureSize};
pub use sprite::{Sprite, SpriteFeature};
pub use view::{
    Camera, Frustum, RenderQueueSort, RenderStats, SceneView, SceneViewKind, TemporalViewState,
    ViewportRect,
};

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
        let _texture_readiness = TextureReadiness::CpuLoading;
        let _sprite = Sprite::new(0.0, 0.0, 1.0, 1.0);
        let _sprite_animation_clip =
            SpriteAnimationClip::new([SpriteAnimationFrame::new([0.0, 0.0, 1.0, 1.0], 100)]);
        let _sprite_animator =
            SpriteAnimator::new(crate::asset::Handle::new(crate::asset::AssetId::new()));
        let _animate_sprites: fn(&mut crate::ecs::World) = animate_sprites;
        let _stats = RenderStats::default();
        let _timings = RenderTimingStats::default();
        let _texture_spec = TextureSpec::rgba16f("curated_texture_spec").half_res();
        let _view = CameraViewport::default();
        let _viewport = ViewportRect::default();
        let _transform = Transform::default();
        let _material_state = MaterialRenderState::transparent();
        let _sprite_material = SpriteMaterial::default();
        let _unlit_material = UnlitMaterial::default();
        let _standard_material = StandardMaterial::default();
        let _depth_format = DEFAULT_DEPTH_FORMAT;
        let _opaque_phase = expert::draw::OpaquePhase::new();
        let _mesh_renderer = WgpuMeshRenderer::new(
            expert::resources::Mesh::QUAD,
            expert::resources::MaterialHandle::new::<SpriteMaterial>(0, 0),
        );
        let _gpu_light_kind = GpuLightKind::Point;
        let _scene_lighting: Option<SceneLightingResources<'_>> = None;
        let _sprite_renderer = SpriteRenderer::new(8.0, 8.0);
        let _tile = features::tilemap::Tile::new(features::tilemap::TileId(0));
        let _tilemap_storage = features::tilemap::TilemapStorage::new();
        let _tilemap_feature = features::tilemap::TilemapFeature::unlit();
        let _tilemap_feature_with_cache = features::tilemap::TilemapFeature::unlit()
            .with_cache_config(features::tilemap::TilemapCacheConfig::default());
        let _tilemap_renderer = features::tilemap::TilemapRenderer::new(
            features::tilemap::TilemapHandle::new(0, 0),
            features::tilemap::TilesetGrid::new(
                crate::asset::Handle::new(crate::asset::AssetId::new()),
                [8, 8],
                1,
                1,
            ),
        )
        .cache_prewarm(true);
        let _tilemap_orientation = features::tilemap::TilemapOrientation::Isometric;
        let _tilemap_render_order = features::tilemap::TilemapRenderOrder::RightDown;
        let _tilemap_stagger_axis = features::tilemap::TilemapStaggerAxis::Y;
        let _tilemap_stagger_index = features::tilemap::TilemapStaggerIndex::Odd;
        let _tiled_error: Option<features::tilemap::TiledImportError> = None;
        let _tiled_map_instance_error: Option<features::tilemap::TiledMapInstanceError> = None;
        let _tiled_spawn_options = features::tilemap::TiledSpawnOptions::centered();
        let _tiled_spawn_origin = features::tilemap::TiledSpawnOrigin::Centered;
        let _light = PointLight::new(64.0);
        let _spot = SpotLight::new(32.0);
        let _composer = RenderRuntime::from_asset(RenderPipelineAsset::builder().build());
        let _feature = SpriteFeature::unlit();
        let _main_camera = MainCamera;
        let _sorting_layer = SortingLayer::default();
        let _mask = RenderLayerMask::default();
    }

    #[test]
    fn expert_namespace_exposes_low_level_render_api() {
        let _graph = expert::graph::RenderGraph::new();
        let _spec = expert::draw::TextureSpec::r32f("expert_texture_spec").storage();
        let _target: Option<expert::gpu::RenderTarget> = None;
        let _batch: Option<expert::draw::SpriteBatch> = None;
        let _light: Option<expert::draw::LightPass> = None;
        let _light_kind = expert::draw::GpuLightKind::Directional;
        let _scene_lighting: Option<expert::draw::SceneLightingResources<'_>> = None;
        let _bloom: Option<expert::draw::Bloom> = None;
        let _tonemap: Option<expert::draw::ToneMap> = None;
        let _vignette: Option<expert::draw::Vignette> = None;
    }

    #[test]
    fn feature_facades_expose_family_specific_api() {
        let _gi_feature = features::gi::GiFeature;
        let _gi_update = features::gi::GiUpdateCompute;
        let _gi_composite = features::gi::GiCompositePass;
        let _bloom = features::postfx::Bloom::default();
        let _tonemap = features::postfx::ToneMap::default();
        let _tilemap = features::tilemap::TilemapFeature::unlit()
            .with_cache_config(features::tilemap::TilemapCacheConfig::default());
        let _tiled_spawn = features::tilemap::TiledSpawnOptions::centered();
    }

    #[cfg(feature = "live2d")]
    #[test]
    fn live2d_feature_facade_exposes_runtime_feature() {
        let _feature = features::live2d::Live2DFeature::new();
        let _commands = features::live2d::Live2DCommands::default();
    }
}
