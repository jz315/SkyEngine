use crate::ecs::World;
use crate::gpu::GpuContext;
use crate::render::component::RenderSettings;
use crate::render::execution::FramePipeline;
use crate::render::gpu::GpuScene;
use crate::render::gpu::Texture;
use crate::render::lighting::shadow::{
    create_shadow_compare_sampler, ShadowPassBindingLayout, ShadowSceneBindingLayout,
};
use crate::render::phase::DrawFunctionRegistry;
use crate::render::pipeline::{MaterialRegistration, RenderPipelineAsset};
use crate::render::resources::material::{
    Material, MaterialRegistry, MaterialStorage, MaterialStorageMut, PipelineCache,
};
use crate::render::resources::mesh::{Mesh, MeshHandle};
use crate::render::view::{RenderStats, ResolvedSceneTransforms, SceneView};

use super::state::{ComposerPlan, ComposerResources, ComposerRuntime, ShadowRuntime};
use super::WorldViewCollector;

pub struct RenderComposer {
    pub(crate) plan: ComposerPlan,
    pub(crate) resources: ComposerResources,
    pub(crate) runtime: ComposerRuntime,
    pub(crate) shadows: ShadowRuntime,
}

impl RenderComposer {
    pub fn from_asset(asset: RenderPipelineAsset) -> Self {
        let mut draw_functions = DrawFunctionRegistry::new();
        for draw_function in asset.draw_functions {
            let _ = draw_functions.register_boxed(draw_function);
        }

        Self {
            plan: ComposerPlan {
                runtime_features: asset.runtime_features,
                steps: asset.steps,
                extractors: asset.extractors,
                gpu_tables: asset.gpu_tables,
                materials: asset.materials,
            },
            resources: ComposerResources {
                draw_functions,
                material_registry: MaterialRegistry::default(),
                mesh_registry: crate::render::resources::mesh::MeshRegistry::default(),
            },
            runtime: ComposerRuntime {
                pipeline_initialized: false,
                last_stats: RenderStats::default(),
                surface_size: [1, 1],
                frame_settings: RenderSettings::default(),
                view_collector: WorldViewCollector::default(),
                temporal: crate::render::runtime::TemporalViewTracker::default(),
                gpu_scene: None,
                gi: None,
                fallback_texture: None,
                history: crate::render::runtime::HistoryTextureStore::new(),
                asset_event_cursor: crate::asset::AssetEventCursor::default(),
                previous_model_by_entity: rustc_hash::FxHashMap::default(),
            },
            shadows: ShadowRuntime {
                layout: None,
                pass_layout: None,
                compare_sampler: None,
                views: Vec::new(),
            },
        }
    }

    #[inline]
    pub fn stats(&self) -> RenderStats {
        self.runtime.last_stats
    }

    pub(crate) fn collect_world_views(
        &mut self,
        world: &World,
        transforms: &ResolvedSceneTransforms,
    ) -> Vec<SceneView> {
        self.runtime.view_collector.collect_world_views(
            world,
            transforms,
            self.runtime.surface_size,
        )
    }

    pub(crate) fn resolve_scene_transforms(&mut self, world: &World) -> ResolvedSceneTransforms {
        self.runtime.view_collector.resolve_transforms(world)
    }

    pub fn resize(&mut self, _gpu: &GpuContext, width: u32, height: u32) {
        self.runtime.surface_size = [width.max(1), height.max(1)];
    }

    pub fn surface_lost(&mut self) {}

    pub fn feature_mut<T: 'static>(&mut self) -> Option<&mut T> {
        self.plan
            .runtime_features
            .iter_mut()
            .find_map(|feature| feature.as_any_mut().downcast_mut::<T>())
    }

    pub fn register_material<M: Material>(&mut self, gpu: &GpuContext) {
        let _ = self
            .resources
            .material_registry
            .register_material::<M>(gpu.device());
    }

    #[inline]
    pub fn try_materials<M: Material>(&self) -> Option<MaterialStorage<'_, M>> {
        self.resources.material_registry.try_materials::<M>()
    }

    #[inline]
    pub fn materials<M: Material>(&self) -> MaterialStorage<'_, M> {
        self.resources.material_registry.materials::<M>()
    }

    #[inline]
    pub fn try_materials_mut<M: Material>(&mut self) -> Option<MaterialStorageMut<'_, M>> {
        self.resources.material_registry.try_materials_mut::<M>()
    }

    #[inline]
    pub fn materials_mut<M: Material>(&mut self) -> MaterialStorageMut<'_, M> {
        self.resources.material_registry.ensure_storage::<M>()
    }

    #[inline]
    pub fn material_pipeline_cache(&self) -> &PipelineCache {
        self.resources.material_registry.pipeline_cache()
    }

    #[inline]
    pub fn material_pipeline_cache_mut(&mut self) -> &mut PipelineCache {
        self.resources.material_registry.pipeline_cache_mut()
    }

    pub(crate) fn ensure_registered_materials(&mut self, gpu: &GpuContext) {
        for MaterialRegistration {
            type_id: _,
            type_name: _,
            register,
        } in &self.plan.materials
        {
            register(&mut self.resources.material_registry, gpu.device());
        }
    }

    /// Realize the pipeline declarations that depend on a live GPU device.
    ///
    /// After this runs, material models declared by the pipeline builder are
    /// available through `materials()` / `materials_mut()` even before the
    /// first frame is rendered. Per-frame resources stay lazy.
    pub(crate) fn initialize_for_gpu(&mut self, gpu: &GpuContext) {
        if self.runtime.pipeline_initialized {
            return;
        }
        self.ensure_registered_materials(gpu);
        self.runtime.pipeline_initialized = true;
    }

    pub(crate) fn ensure_builtin_meshes(&mut self, gpu: &GpuContext) {
        let _ = self.resources.mesh_registry.ensure_builtin_quad(gpu);
    }

    pub(crate) fn ensure_phase_runtime(&mut self, gpu: &GpuContext) {
        if self.runtime.gpu_scene.is_none() {
            let mut gpu_scene = GpuScene::new(gpu);
            for table in self.plan.gpu_tables.drain(..) {
                gpu_scene.register_boxed(table);
            }
            self.runtime.gpu_scene = Some(gpu_scene);
        }
        if self.runtime.fallback_texture.is_none() {
            self.runtime.fallback_texture = Some(Texture::white_pixel(gpu));
        }
    }

    pub(crate) fn ensure_shadow_runtime(&mut self, gpu: &GpuContext) {
        if self.shadows.layout.is_none() {
            self.shadows.layout = Some(ShadowSceneBindingLayout::new(gpu.device()));
        }
        if self.shadows.pass_layout.is_none() {
            self.shadows.pass_layout = Some(ShadowPassBindingLayout::new(gpu.device()));
        }
        if self.shadows.compare_sampler.is_none() {
            self.shadows.compare_sampler = Some(create_shadow_compare_sampler(gpu.device()));
        }
    }

    pub(crate) fn ensure_gi_runtime(&mut self, gpu: &GpuContext) {
        if self.runtime.gi.is_none() {
            self.runtime.gi = Some(crate::render::gi::GiRuntime::new(gpu));
        }
    }

    pub(crate) fn build_runtime_pipeline(&mut self, gpu: &GpuContext) -> FramePipeline {
        super::pipeline_runtime::build_runtime_pipeline(self, gpu)
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
