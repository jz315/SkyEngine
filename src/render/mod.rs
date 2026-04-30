//! SkyEngine high-level rendering facade built around programmable scene pipelines.

pub mod animation;
pub mod assets;
pub mod backend;
pub mod component;
pub(crate) mod composite;
pub(crate) mod execution;
pub mod expert;
pub(crate) mod extract;
pub(crate) mod gi;
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
pub(crate) mod tilemap;
pub(crate) mod view;

#[cfg(feature = "live2d")]
pub(crate) mod live2d;

pub use crate::math::{Projection, Quat, Transform};
pub use animation::{animate_sprites, SpriteAnimationClip, SpriteAnimationFrame, SpriteAnimator};
pub use assets::{
    MeshAsset, MeshAssetDescriptor, MeshAssetError, MeshBoundingSphere, MeshIndexData, MeshSubMesh,
    MeshVertexAttribute, MeshVertexFormat, MeshVertexLayout, MeshVertexSemantic, RenderAssets,
    StandardMaterialAsset, TextureAddressMode, TextureFilter, TextureSamplerDesc,
};
#[cfg(feature = "kajiya-renderer")]
pub use backend::{KajiyaSceneRenderer, KajiyaSceneSyncStats};
#[cfg(feature = "renderling-renderer")]
pub use backend::{RenderlingSceneRenderer, RenderlingSceneSyncStats};
pub use backend::{
    SceneRenderer, SceneRendererError, SceneRendererInitError, SceneSpotLight, WgpuSceneRenderer,
};
pub use component::{
    BloomSettings, Camera as CameraMarker, CameraViewport, DdgiSettings, DdgiVolumeSettings,
    DirectionalLight, GiDebugMode, GlobalIlluminationMode, GlobalIlluminationSettings, MainCamera,
    MeshRenderer, Parent, PointLight, RenderDebugView, RenderLayerMask, RenderSettings,
    ShadowSamplingMode, ShadowUpdatePolicy, SharpenSettings, SortingLayer, SpotLight,
    SpriteRenderer, SsgiSettings, TemporalAntiAliasingSettings, TileAnimation, TileAnimationFrame,
    TilemapDepthSort, TilemapOrientation, TilemapRenderOrder, TilemapRenderer, TilemapStaggerAxis,
    TilemapStaggerIndex, TilesetGrid, TilesetTileRect, ToneMapSettings, VignetteSettings,
    WgpuMeshRenderer, ALL_SHADOW_CASCADE_MASK, MAX_DIRECTIONAL_SHADOW_CASCADES,
};
pub use execution::SceneTexture;
pub use gi::{SsgiComputeTextureLayout, SsgiPass, SsgiResources};
pub use gpu::{
    is_depth_format, GpuScene, GpuTable, GpuTableManager, ModelMatrixTable, Texture,
    DEFAULT_DEPTH_FORMAT,
};
pub use lighting::DirectionalShadowPhase;
pub use lighting::{GpuLight, GpuLightKind, Light2D, LightTable, SceneLightingResources};
pub use phase::{OpaquePhase, TransparentPhase};
pub use pipeline::{
    Bloom, ComputePass, ComputePassExecuteContext, ComputePassSetupContext, DdgiUpdateCompute,
    DebugView, GraphPass, GraphPassExecuteContext, GraphPassSetupContext, KajiyaDpiMode,
    KajiyaRendererSettings, PipelineStepDescriptor, PostFxPass, PostFxPassExecuteContext,
    PostFxPassSetupContext, RenderBackendKind, RenderFeature, RenderPass, RenderPassExecuteContext,
    RenderPassSetupContext, RenderPhase, RenderPhaseExecuteContext, RenderPhaseSetupContext,
    RenderPipelineAsset, RenderPipelineBuilder, RenderPipelineDescriptor, SceneMaterialPrepass,
    SceneNormalPrepass, Sharpen, SpriteFeature, TemporalAntiAliasing, TextureSpec, ToneMap,
    Vignette,
};
pub use resources::material::{
    AlphaMode, Material, MaterialBindContext, MaterialHandle, MaterialRenderState, MaterialStorage,
    SceneBindingDesc, SceneBindingKind, ShaderSource, SpriteMaterial, StandardMaterial,
    UnlitMaterial,
};
pub use runtime::RenderComposer;
pub use runtime::RenderTimingStats;
pub use runtime::{HistoryTexture, HistoryTextureRequest, HistoryTextureSize};
pub use sprite::Sprite;
pub use tilemap::{
    Tile, TileChunkBounds, TileFlags, TileId, TiledImport, TiledImportError, TiledLayer,
    TiledMapInstance, TiledMapInstanceError, TiledObject, TiledObjectLayer, TiledObjectShape,
    TiledProperty, TiledPropertyValue, TiledSpawnOptions, TiledSpawnOrigin, TiledTileObject,
    TiledTileset, Tilemap, TilemapCacheConfig, TilemapDescriptor, TilemapFeature, TilemapHandle,
    TilemapStorage,
};
#[cfg(feature = "physics")]
pub use tilemap::{TiledPhysicsError, TiledPhysicsInstance, TiledPhysicsOptions};
pub use view::{
    Camera, Color, Frustum, RenderQueueSort, RenderStats, SceneView, SceneViewKind,
    TemporalViewState, ViewportRect,
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
        let _opaque_phase = expert::OpaquePhase::new();
        let _mesh_renderer = WgpuMeshRenderer::new(
            expert::Mesh::QUAD,
            expert::MaterialHandle::new::<SpriteMaterial>(0, 0),
        );
        let _gpu_light_kind = GpuLightKind::Point;
        let _scene_lighting: Option<SceneLightingResources<'_>> = None;
        let _ssgi_layout: Option<SsgiComputeTextureLayout> = None;
        let _sprite_renderer = SpriteRenderer::new(8.0, 8.0);
        let _tile = Tile::new(TileId(0));
        let _tilemap_storage = TilemapStorage::new();
        let _tilemap_feature = TilemapFeature::unlit();
        let _tilemap_feature_with_cache =
            TilemapFeature::unlit().with_cache_config(TilemapCacheConfig::default());
        let _tilemap_renderer = TilemapRenderer::new(
            TilemapHandle::new(0, 0),
            TilesetGrid::new(
                crate::asset::Handle::new(crate::asset::AssetId::new()),
                [8, 8],
                1,
                1,
            ),
        )
        .cache_prewarm(true);
        let _tilemap_orientation = TilemapOrientation::Isometric;
        let _tilemap_render_order = TilemapRenderOrder::RightDown;
        let _tilemap_stagger_axis = TilemapStaggerAxis::Y;
        let _tilemap_stagger_index = TilemapStaggerIndex::Odd;
        let _tiled_error: Option<TiledImportError> = None;
        let _tiled_map_instance_error: Option<TiledMapInstanceError> = None;
        let _tiled_spawn_options = TiledSpawnOptions::centered();
        let _tiled_spawn_origin = TiledSpawnOrigin::Centered;
        let _light = PointLight::new(64.0);
        let _spot = SpotLight::new(32.0);
        let _composer = RenderComposer::from_asset(RenderPipelineAsset::builder().build());
        let _feature = SpriteFeature::unlit();
        let _main_camera = MainCamera;
        let _sorting_layer = SortingLayer::default();
        let _mask = RenderLayerMask::default();
    }

    #[test]
    fn expert_namespace_exposes_low_level_render_api() {
        let _graph = expert::RenderGraph::new();
        let _spec = expert::TextureSpec::r32f("expert_texture_spec").storage();
        let _target: Option<expert::RenderTarget> = None;
        let _batch: Option<expert::SpriteBatch> = None;
        let _light: Option<expert::LightPass> = None;
        let _light_kind = expert::GpuLightKind::Directional;
        let _scene_lighting: Option<expert::SceneLightingResources<'_>> = None;
        let _composite: Option<expert::CompositePass> = None;
        let _bloom: Option<expert::Bloom> = None;
        let _tonemap: Option<expert::ToneMap> = None;
        let _vignette: Option<expert::Vignette> = None;
    }
}
