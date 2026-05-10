use crate::ecs::World;
use crate::gpu::GpuContext;
use crate::render::component::RenderSettings;
use crate::render::gpu::{GpuScene, Texture};
use crate::render::lighting::shadow::{
    create_shadow_compare_sampler, ShadowPassBindingLayout, ShadowSceneBindingLayout,
};
use crate::render::resources::material::SpriteMaterial;
use crate::render::resources::texture_cache::SharedRenderAssetCache;

use super::{prepare_declared_materials, FrameInputs, FrameRuntimeParts};
use crate::render::runtime::timing_start;

pub(crate) fn begin_frame_inputs(
    parts: &mut FrameRuntimeParts<'_>,
    gpu: &mut GpuContext,
    world: &World,
    asset_cache: Option<&SharedRenderAssetCache>,
) -> FrameInputs {
    ensure_frame_runtime(parts, gpu);
    {
        let pipeline_cache = parts.resources.material_registry.pipeline_cache_mut();
        pipeline_cache.new_frame();
        pipeline_cache.garbage_collect();
    }
    parts.runtime.surface_size = gpu.surface_size();
    parts.runtime.history.begin_frame(gpu);
    parts.runtime.frame_settings = world
        .get_resource::<RenderSettings>()
        .cloned()
        .unwrap_or_default();
    if let Some(asset_cache) = asset_cache {
        asset_cache.borrow_mut().begin_frame();
    }
    if parts
        .resources
        .material_registry
        .is_registered::<SpriteMaterial>()
    {
        parts
            .resources
            .material_registry
            .clear_model_instances::<SpriteMaterial>();
    }

    FrameInputs {
        frame_start: timing_start(),
        resolved_transforms: parts.runtime.view_collector.resolve_transforms(world),
    }
}

fn ensure_frame_runtime(parts: &mut FrameRuntimeParts<'_>, gpu: &GpuContext) {
    prepare_declared_materials(parts.plan, parts.resources, parts.runtime, gpu);
    let _ = parts.resources.mesh_registry.ensure_builtin_quad(gpu);
    if parts.runtime.gpu_scene.is_none() {
        let mut gpu_scene = GpuScene::new(gpu);
        for table in parts.plan.gpu_tables.drain(..) {
            gpu_scene.register_boxed(table);
        }
        parts.runtime.gpu_scene = Some(gpu_scene);
    }
    if parts.runtime.fallback_texture.is_none() {
        parts.runtime.fallback_texture = Some(Texture::white_pixel(gpu));
    }
    if parts.shadows.layout.is_none() {
        parts.shadows.layout = Some(ShadowSceneBindingLayout::new(gpu.device()));
    }
    if parts.shadows.pass_layout.is_none() {
        parts.shadows.pass_layout = Some(ShadowPassBindingLayout::new(gpu.device()));
    }
    if parts.shadows.compare_sampler.is_none() {
        parts.shadows.compare_sampler = Some(create_shadow_compare_sampler(gpu.device()));
    }
    if parts.runtime.gi.is_none() {
        parts.runtime.gi = Some(crate::render::gi::GiRuntime::new(gpu));
    }
}
