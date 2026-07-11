use crate::render::execution::{PreparedFrame, PreparedView};
use crate::render::gpu::GpuScene;
use crate::render::pipeline::AnyRenderFeature;
use crate::render::runtime::FrameExtension;
use crate::render::RenderSettings;

use super::{ExtractedFrame, SceneUploadFrame};

pub(crate) struct FrameAssemblyInputs<'frame> {
    pub(crate) surface_format: wgpu::TextureFormat,
    pub(crate) has_surface: bool,
    pub(crate) frame_settings: &'frame RenderSettings,
    pub(crate) history: &'frame super::super::HistoryTextureStore,
    pub(crate) gpu_scene: &'frame GpuScene,
    pub(crate) runtime_features: &'frame [Box<dyn AnyRenderFeature>],
    pub(crate) frame_extensions: &'frame [Box<dyn FrameExtension>],
}

pub(crate) fn assemble_prepared_frame<'frame>(
    assembly: FrameAssemblyInputs<'frame>,
    extracted: &'frame ExtractedFrame,
    uploads: &'frame SceneUploadFrame,
) -> PreparedFrame<'frame> {
    let mut frame = PreparedFrame::new(assembly.surface_format, assembly.has_surface);
    let _ = frame.insert_payload(assembly.frame_settings);
    let _ = frame.insert_payload(assembly.history);
    let _ = frame.insert_payload(assembly.gpu_scene);
    let _ = frame.insert_payload(&uploads.model_matrices);
    let _ = frame.insert_payload(&uploads.previous_model_matrices);
    for feature in assembly.runtime_features {
        feature.insert_frame_payloads(&mut frame);
    }
    for extension in assembly.frame_extensions {
        extension.insert_frame_payloads(&mut frame);
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
        for feature in assembly.runtime_features {
            feature.insert_view_payloads(view_index, view, &mut prepared_view);
        }
        for extension in assembly.frame_extensions {
            extension.insert_view_payloads(view_index, view, &mut prepared_view);
        }
        frame.add_view(prepared_view);
    }
    frame
}
