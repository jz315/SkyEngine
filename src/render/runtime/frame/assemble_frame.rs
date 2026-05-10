use crate::render::execution::{PreparedFrame, PreparedView};
use crate::render::gpu::GpuScene;
use crate::render::lighting::shadow::{
    ShadowPassBindingLayout, ShadowSceneBindingLayout, ShadowViewBinding,
};
use crate::render::pipeline::AnyRenderFeature;
use crate::render::RenderSettings;

use super::{ExtractedFrame, SceneUploadFrame, ShadowFrameSummary};

pub(crate) struct FrameAssemblyInputs<'frame> {
    pub(crate) surface_format: wgpu::TextureFormat,
    pub(crate) has_surface: bool,
    pub(crate) frame_settings: &'frame RenderSettings,
    pub(crate) history: &'frame super::super::HistoryTextureStore,
    pub(crate) gpu_scene: &'frame GpuScene,
    pub(crate) gi: &'frame crate::render::gi::GiRuntime,
    pub(crate) shadow_layout: &'frame ShadowSceneBindingLayout,
    pub(crate) shadow_pass_layout: &'frame ShadowPassBindingLayout,
    pub(crate) shadow_views: &'frame [ShadowViewBinding],
    pub(crate) runtime_features: &'frame [Box<dyn AnyRenderFeature>],
}

pub(crate) fn assemble_prepared_frame<'frame>(
    assembly: FrameAssemblyInputs<'frame>,
    extracted: &'frame ExtractedFrame,
    uploads: &'frame SceneUploadFrame,
    shadows: &'frame ShadowFrameSummary,
) -> PreparedFrame<'frame> {
    let mut frame = PreparedFrame::new(assembly.surface_format, assembly.has_surface);
    let _ = frame.insert_payload(assembly.frame_settings);
    let _ = frame.insert_payload(assembly.history);
    let _ = frame.insert_payload(assembly.gpu_scene);
    let _ = frame.insert_payload(&uploads.model_matrices);
    let _ = frame.insert_payload(&uploads.previous_model_matrices);
    let _ = frame.insert_payload(assembly.gi);
    let _ = frame.insert_payload(assembly.shadow_layout);
    let _ = frame.insert_payload(assembly.shadow_pass_layout);
    if let Some(shadow_debug_resources) = shadows.debug_resources.as_ref() {
        let _ = frame.insert_payload(shadow_debug_resources);
    }
    for feature in assembly.runtime_features {
        feature.insert_frame_payloads(&mut frame);
    }

    for (view_index, view) in extracted.views.iter().enumerate() {
        let mut prepared_view = PreparedView::new(
            view.execution_order(),
            view.viewport,
            view.target_size,
            view.clear_surface,
        );
        prepared_view.set_history_key(view.history_key());
        let _ = prepared_view.insert_payload(view);
        let _ = prepared_view.insert_payload(&extracted.opaque_phases[view_index]);
        let _ = prepared_view.insert_payload(&extracted.transparent_phases[view_index]);
        if let Some(binding_index) = view.shadow_binding() {
            let _ = prepared_view.insert_payload(&assembly.shadow_views[binding_index]);
        }
        for feature in assembly.runtime_features {
            feature.insert_view_payloads(view_index, view, &mut prepared_view);
        }
        frame.add_view(prepared_view);
    }
    frame
}
