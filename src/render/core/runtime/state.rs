use crate::asset::AssetEventCursor;
use crate::ecs::EntityId;
use crate::render::extract::Extractor;
use crate::render::gpu::GpuScene;
use crate::render::gpu::Texture;
use crate::render::phase::DrawFunctionRegistry;
use crate::render::pipeline::{AnyRenderFeature, MaterialRegistration, PipelineStep};
use crate::render::resources::material::MaterialRegistry;
use crate::render::resources::mesh::MeshRegistry;
use crate::render::runtime::FrameExtension;
use crate::render::view::RenderStats;
use crate::render::GpuTable;
use crate::render::RenderSettings;

use super::{HistoryTextureStore, TemporalViewTracker, WorldViewCollector};

pub(crate) struct RuntimePlan {
    pub(crate) runtime_features: Vec<Box<dyn AnyRenderFeature>>,
    pub(crate) frame_extensions: Vec<Box<dyn FrameExtension>>,
    pub(crate) steps: Vec<PipelineStep>,
    pub(crate) extractors: Vec<Box<dyn Extractor>>,
    pub(crate) gpu_tables: Vec<Box<dyn GpuTable>>,
    pub(crate) materials: Vec<MaterialRegistration>,
}

pub(crate) struct RenderResourceHub {
    pub(crate) draw_functions: DrawFunctionRegistry,
    pub(crate) material_registry: MaterialRegistry,
    pub(crate) mesh_registry: MeshRegistry,
}

pub(crate) struct FrameRuntimeState {
    pub(crate) pipeline_initialized: bool,
    pub(crate) last_stats: RenderStats,
    pub(crate) surface_size: [u32; 2],
    pub(crate) frame_settings: RenderSettings,
    pub(crate) view_collector: WorldViewCollector,
    pub(crate) temporal: TemporalViewTracker,
    pub(crate) gpu_scene: Option<GpuScene>,
    pub(crate) fallback_texture: Option<Texture>,
    pub(crate) history: HistoryTextureStore,
    pub(crate) asset_event_cursor: AssetEventCursor,
    pub(crate) previous_model_by_entity: rustc_hash::FxHashMap<EntityId, [f32; 16]>,
}
