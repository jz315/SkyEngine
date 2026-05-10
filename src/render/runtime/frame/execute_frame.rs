use crate::gpu::GpuContext;

use super::{
    assemble_prepared_frame, ExtractedFrame, FrameAssemblyInputs, FrameExecutionSummary,
    FrameRuntimeParts, SceneUploadFrame, ShadowFrameSummary,
};

pub(crate) fn execute_prepared_frame(
    parts: &mut FrameRuntimeParts<'_>,
    gpu: &mut GpuContext,
    extracted: &ExtractedFrame,
    uploads: &SceneUploadFrame,
    shadows: &ShadowFrameSummary,
) -> FrameExecutionSummary {
    let assembly = FrameAssemblyInputs {
        surface_format: gpu.surface_format(),
        has_surface: gpu.has_surface(),
        frame_settings: &parts.runtime.frame_settings,
        history: &parts.runtime.history,
        gpu_scene: parts
            .runtime
            .gpu_scene
            .as_ref()
            .expect("phase runtime should initialize a GpuScene"),
        gi: parts
            .runtime
            .gi
            .as_ref()
            .expect("GI runtime should be initialized before frame build"),
        shadow_layout: parts
            .shadows
            .layout
            .as_ref()
            .expect("shadow runtime should initialize a shared shadow layout"),
        shadow_pass_layout: parts
            .shadows
            .pass_layout
            .as_ref()
            .expect("shadow runtime should initialize a shared shadow-pass layout"),
        shadow_views: &parts.shadows.views,
        runtime_features: &parts.plan.runtime_features,
    };
    let frame = assemble_prepared_frame(assembly, extracted, uploads, shadows);
    let fallback_texture = parts
        .runtime
        .fallback_texture
        .as_ref()
        .expect("phase runtime should initialize fallback texture before execution");
    parts.executor.execute_prepared_frame(
        &mut parts.plan.steps,
        parts.resources,
        fallback_texture,
        gpu,
        &frame,
    )
}
