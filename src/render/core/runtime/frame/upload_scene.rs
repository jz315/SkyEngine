use crate::ecs::World;
use crate::gpu::GpuContext;
use crate::render::gpu::ModelMatrixTable;
use crate::render::resources::LightTable;
use crate::render::runtime::FrameExtensionUploadContext;

use super::{ExtractedFrame, FrameInputs, FrameRuntimeParts, SceneUploadFrame};

pub(crate) fn upload_scene_data(
    parts: &mut FrameRuntimeParts<'_>,
    gpu: &mut GpuContext,
    world: &World,
    inputs: &FrameInputs,
    extracted: &mut ExtractedFrame,
    mut upload: SceneUploadFrame,
) -> SceneUploadFrame {
    upload.entity_to_model_slot.clear();
    upload.model_matrices.clear();
    upload.model_matrices.push(super::IDENTITY_MODEL_MATRIX);
    for phase in &mut extracted.opaque_phases {
        parts.resources.draw_functions.assign_model_matrices(
            phase.items_mut(),
            &inputs.resolved_transforms,
            &mut upload.entity_to_model_slot,
            &mut upload.model_matrices,
        );
    }
    for phase in &mut extracted.transparent_phases {
        parts.resources.draw_functions.assign_model_matrices(
            phase.items_mut(),
            &inputs.resolved_transforms,
            &mut upload.entity_to_model_slot,
            &mut upload.model_matrices,
        );
    }

    upload.previous_model_matrices.0.clear();
    upload
        .previous_model_matrices
        .0
        .extend_from_slice(&upload.model_matrices);
    for (entity, slot) in &upload.entity_to_model_slot {
        if let Some(previous) = parts.runtime.previous_model_by_entity.get(entity) {
            upload.previous_model_matrices.0[*slot as usize] = *previous;
        }
    }

    upload.lights.clear();
    for extension in &mut parts.plan.frame_extensions {
        extension.upload_scene(FrameExtensionUploadContext {
            world,
            transforms: &inputs.resolved_transforms,
            upload: &mut upload,
        });
    }
    {
        let gpu_scene = parts
            .runtime
            .gpu_scene
            .as_mut()
            .expect("phase runtime should initialize a GpuScene");
        gpu_scene
            .table_mut::<ModelMatrixTable>()
            .set_all(gpu, &upload.model_matrices);
        gpu_scene
            .table_mut::<LightTable>()
            .set_all(gpu, &upload.lights);
        gpu_scene.upload_all(gpu.queue());
    }

    upload
}
