use crate::gpu::GpuContext;
use crate::render::resources::texture_cache::{RenderAssetStats, SharedRenderAssetCache};
use crate::render::runtime::{FrameExtensionError, FrameExtensionPrepareContext};

use super::{ExtractedFrame, FrameRuntimeParts, SceneUploadFrame};
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
) -> bool {
    if let Some(asset_cache) = asset_cache {
        asset_cache.borrow_mut().prepare_queued_textures(gpu);
    }
    if let Err(error) = parts
        .resources
        .material_registry
        .prepare_dirty(gpu, parts.runtime.fallback_texture.as_ref())
    {
        eprintln!("[SkyEngine] Render material preparation failed; skipping frame draw: {error}");
        return false;
    }
    true
}

pub(crate) fn prepare_frame_extensions(
    parts: &mut FrameRuntimeParts<'_>,
    gpu: &mut GpuContext,
    extracted: &ExtractedFrame,
    uploads: &SceneUploadFrame,
) -> Result<(), FrameExtensionError> {
    let plan = &mut *parts.plan;
    let resources = &*parts.resources;
    let runtime = &*parts.runtime;
    for extension in &mut plan.frame_extensions {
        extension.prepare(FrameExtensionPrepareContext {
            gpu,
            resources,
            runtime,
            extracted,
            uploads,
        })?;
    }
    Ok(())
}

pub(crate) fn finish_render_assets(
    asset_cache: Option<&SharedRenderAssetCache>,
) -> RenderAssetStats {
    asset_cache.map_or(Default::default(), |asset_cache| {
        asset_cache.borrow_mut().finish_frame()
    })
}
