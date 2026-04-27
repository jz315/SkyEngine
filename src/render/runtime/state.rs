use crate::asset::AssetEventCursor;
use crate::ecs::EntityId;
use crate::render::component::RenderSettings;
use crate::render::extract::Extractor;
use crate::render::gi::DdgiRuntime;
use crate::render::gpu::GpuScene;
use crate::render::gpu::Texture;
use crate::render::lighting::shadow::{
    ShadowPassBindingLayout, ShadowSceneBindingLayout, ShadowViewBinding,
};
use crate::render::phase::DrawFunctionRegistry;
use crate::render::pipeline::{AnyRenderFeature, MaterialRegistration, PipelineStep};
use crate::render::resources::assets::RenderAssetCache;
use crate::render::resources::material::MaterialRegistry;
use crate::render::resources::mesh::MeshRegistry;
use crate::render::view::RenderStats;
use crate::render::GpuTable;

use super::WorldViewCollector;

pub(crate) struct ComposerPlan {
    pub(crate) runtime_features: Vec<Box<dyn AnyRenderFeature>>,
    pub(crate) steps: Vec<PipelineStep>,
    pub(crate) extractors: Vec<Box<dyn Extractor>>,
    pub(crate) gpu_tables: Vec<Box<dyn GpuTable>>,
    pub(crate) materials: Vec<MaterialRegistration>,
}

pub(crate) struct ComposerResources {
    pub(crate) draw_functions: DrawFunctionRegistry,
    pub(crate) material_registry: MaterialRegistry,
    pub(crate) mesh_registry: MeshRegistry,
}

pub(crate) struct ComposerRuntime {
    pub(crate) last_stats: RenderStats,
    pub(crate) surface_size: [u32; 2],
    pub(crate) frame_settings: RenderSettings,
    pub(crate) view_collector: WorldViewCollector,
    pub(crate) gpu_scene: Option<GpuScene>,
    pub(crate) ddgi: Option<DdgiRuntime>,
    pub(crate) fallback_texture: Option<Texture>,
    pub(crate) render_assets: RenderAssetCache,
    pub(crate) asset_event_cursor: AssetEventCursor,
    pub(crate) previous_model_by_entity: rustc_hash::FxHashMap<EntityId, [f32; 16]>,
}

pub(crate) struct ShadowRuntime {
    pub(crate) layout: Option<ShadowSceneBindingLayout>,
    pub(crate) pass_layout: Option<ShadowPassBindingLayout>,
    pub(crate) compare_sampler: Option<wgpu::Sampler>,
    pub(crate) views: Vec<ShadowViewBinding>,
}
