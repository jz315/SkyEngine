use crate::gpu::GpuContext;
use crate::render::phase::DrawFunctionRegistry;
use crate::render::pipeline::RenderPipelineAsset;
use crate::render::resources::material::{
    Material, MaterialHandle, MaterialRegistry, PipelineCache, TypedMaterialHandle,
};
use crate::render::resources::mesh::{Mesh, MeshHandle};
use crate::render::resources::texture_cache::SharedRenderAssetCache;
use crate::render::view::RenderStats;
use crate::render::RenderSettings;

use super::executor::RenderExecutor;
use super::frame::prepare_declared_materials;
use super::frame_coordinator::FrameCoordinator;
use super::state::{FrameRuntimeState, RenderResourceHub, RuntimePlan};
use super::WorldViewCollector;

pub struct RenderRuntime {
    pub(crate) plan: RuntimePlan,
    pub(crate) resources: RenderResourceHub,
    pub(crate) runtime: FrameRuntimeState,
    pub(crate) frame: FrameCoordinator,
    pub(crate) executor: RenderExecutor,
    pub(crate) asset_cache: SharedRenderAssetCache,
}

impl RenderRuntime {
    pub fn from_asset(asset: RenderPipelineAsset) -> Self {
        let mut draw_functions = DrawFunctionRegistry::new();
        for draw_function in asset.draw_functions {
            let _ = draw_functions.register_boxed(draw_function);
        }

        Self {
            plan: RuntimePlan {
                runtime_features: asset.runtime_features,
                frame_extensions: asset.frame_extensions,
                steps: asset.steps,
                extractors: asset.extractors,
                gpu_tables: asset.gpu_tables,
                materials: asset.materials,
            },
            resources: RenderResourceHub {
                draw_functions,
                material_registry: MaterialRegistry::default(),
                mesh_registry: crate::render::resources::mesh::MeshRegistry::default(),
            },
            runtime: FrameRuntimeState {
                pipeline_initialized: false,
                last_stats: RenderStats::default(),
                surface_size: [1, 1],
                frame_settings: RenderSettings::default(),
                view_collector: WorldViewCollector::default(),
                temporal: crate::render::runtime::TemporalViewTracker::default(),
                gpu_scene: None,
                fallback_texture: None,
                history: crate::render::runtime::HistoryTextureStore::new(),
                asset_event_cursor: crate::asset::AssetEventCursor::default(),
                previous_model_by_entity: rustc_hash::FxHashMap::default(),
            },
            frame: FrameCoordinator::new(),
            executor: RenderExecutor::new(),
            asset_cache: SharedRenderAssetCache::default(),
        }
    }

    #[inline]
    pub fn render_asset_cache(&self) -> &SharedRenderAssetCache {
        &self.asset_cache
    }

    #[inline]
    pub fn stats(&self) -> RenderStats {
        self.runtime.last_stats
    }

    pub fn resize(&mut self, _gpu: &GpuContext, width: u32, height: u32) {
        self.runtime.surface_size = [width.max(1), height.max(1)];
    }

    pub fn surface_lost(&mut self) {
        self.runtime.temporal.invalidate();
        self.runtime.history.invalidate();
    }

    pub fn feature_mut<T: 'static>(&mut self) -> Option<&mut T> {
        self.plan
            .runtime_features
            .iter_mut()
            .find_map(|feature| feature.as_any_mut().downcast_mut::<T>())
    }

    #[cfg(test)]
    pub(crate) fn frame_extension<T: 'static>(&self) -> Option<&T> {
        self.plan
            .frame_extensions
            .iter()
            .find_map(|extension| extension.as_any().downcast_ref::<T>())
    }

    pub fn register_material<M: Material>(&mut self, gpu: &GpuContext) {
        let _ = self
            .resources
            .material_registry
            .register_model::<M>(gpu.device());
    }

    #[inline]
    pub fn try_insert_material<M: Material>(
        &mut self,
        material: M::Data,
    ) -> Result<TypedMaterialHandle<M>, crate::render::resources::material::MaterialError> {
        self.resources
            .material_registry
            .insert_material::<M>(material)
    }

    #[inline]
    pub fn insert_material<M: Material>(&mut self, material: M::Data) -> TypedMaterialHandle<M> {
        self.try_insert_material::<M>(material)
            .expect("material model should be registered before insertion")
    }

    #[inline]
    pub fn material<M: Material>(
        &self,
        handle: TypedMaterialHandle<M>,
    ) -> Result<&M::Data, crate::render::resources::material::MaterialError> {
        self.resources.material_registry.get_material(handle)
    }

    #[inline]
    pub fn material_erased<M: Material>(
        &self,
        handle: MaterialHandle,
    ) -> Result<&M::Data, crate::render::resources::material::MaterialError> {
        self.resources.material_registry.get_erased::<M>(handle)
    }

    #[inline]
    pub fn set_material<M, F>(
        &mut self,
        handle: TypedMaterialHandle<M>,
        update: F,
    ) -> Result<(), crate::render::resources::material::MaterialError>
    where
        M: Material,
        F: FnOnce(&mut M::Data),
    {
        self.resources
            .material_registry
            .set_material(handle, update)
    }

    #[inline]
    pub fn remove_material<M: Material>(
        &mut self,
        handle: TypedMaterialHandle<M>,
    ) -> Result<M::Data, crate::render::resources::material::MaterialError> {
        self.resources.material_registry.remove_material(handle)
    }

    #[inline]
    pub fn remove_material_erased<M: Material>(
        &mut self,
        handle: MaterialHandle,
    ) -> Result<M::Data, crate::render::resources::material::MaterialError> {
        self.resources.material_registry.remove_erased::<M>(handle)
    }

    #[inline]
    pub fn material_pipeline_cache(&self) -> &PipelineCache {
        self.resources.material_registry.pipeline_cache()
    }

    #[inline]
    pub fn material_pipeline_cache_mut(&mut self) -> &mut PipelineCache {
        self.resources.material_registry.pipeline_cache_mut()
    }

    /// Realize the pipeline declarations that depend on a live GPU device.
    ///
    /// After this runs, material models declared by the pipeline builder are
    /// available through `insert_material` / `material` even before the
    /// first frame is rendered. Per-frame resources stay lazy.
    pub fn prepare_gpu_resources(&mut self, gpu: &GpuContext) {
        prepare_declared_materials(&self.plan, &mut self.resources, &mut self.runtime, gpu);
    }

    #[inline]
    pub fn insert_mesh(&mut self, mesh: Mesh) -> MeshHandle {
        self.resources.mesh_registry.insert(mesh)
    }

    #[inline]
    pub fn mesh(&self, handle: MeshHandle) -> Option<&Mesh> {
        self.resources.mesh_registry.get(handle)
    }

    #[inline]
    pub fn mesh_mut(&mut self, handle: MeshHandle) -> Option<&mut Mesh> {
        self.resources.mesh_registry.get_mut(handle)
    }

    #[inline]
    pub fn remove_mesh(&mut self, handle: MeshHandle) -> Option<Mesh> {
        self.resources.mesh_registry.remove(handle)
    }
}
