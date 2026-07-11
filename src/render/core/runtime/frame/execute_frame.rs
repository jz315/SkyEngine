use crate::gpu::GpuContext;

use super::{
    assemble_prepared_frame, ExtractedFrame, FrameAssemblyInputs, FrameExecutionSummary,
    FrameRuntimeParts, SceneUploadFrame,
};

pub(crate) fn execute_prepared_frame(
    parts: &mut FrameRuntimeParts<'_>,
    gpu: &mut GpuContext,
    extracted: &ExtractedFrame,
    uploads: &SceneUploadFrame,
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
        runtime_features: &parts.plan.runtime_features,
        frame_extensions: &parts.plan.frame_extensions,
    };
    let frame = assemble_prepared_frame(assembly, extracted, uploads);
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
