use crate::gpu::GpuContext;
use crate::render::resources::texture_cache::{RenderAssetStats, SharedRenderAssetCache};

use super::FrameRuntimeParts;
use crate::render::runtime::state::{FrameRuntimeState, RenderResourceHub, RuntimePlan};

pub(crate) fn prepare_declared_materials(
    plan: &RuntimePlan,
    resources: &mut RenderResourceHub,
    runtime: &mut FrameRuntimeState,
    gpu: &GpuContext,
) {
    if runtime.pipeline_initialized {
        return;
    }
    for registration in &plan.materials {
        (registration.register)(&mut resources.material_registry, gpu.device());
    }
    runtime.pipeline_initialized = true;
}

pub(crate) fn prepare_frame_assets(
    parts: &mut FrameRuntimeParts<'_>,
    gpu: &mut GpuContext,
    asset_cache: Option<&SharedRenderAssetCache>,
) {
    if let Some(asset_cache) = asset_cache {
        asset_cache.borrow_mut().prepare_queued_textures(gpu);
    }
    parts
        .resources
        .material_registry
        .prepare_dirty(gpu, parts.runtime.fallback_texture.as_ref())
        .expect("material preparation should succeed before draw");
}

pub(crate) fn finish_render_assets(
    asset_cache: Option<&SharedRenderAssetCache>,
) -> RenderAssetStats {
    asset_cache.map_or(Default::default(), |asset_cache| {
        asset_cache.borrow_mut().finish_frame()
    })
}
