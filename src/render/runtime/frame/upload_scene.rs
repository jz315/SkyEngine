use rustc_hash::FxHashMap;

use crate::ecs::World;
use crate::gpu::GpuContext;
use crate::render::{LightTable, ModelMatrixTable};

use super::{
    collect_gpu_lights, ExtractedFrame, FrameInputs, FrameRuntimeParts, PreviousModelMatrices,
    SceneUploadFrame,
};

pub(crate) fn upload_scene_data(
    parts: &mut FrameRuntimeParts<'_>,
    gpu: &mut GpuContext,
    world: &World,
    inputs: &FrameInputs,
    extracted: &mut ExtractedFrame,
) -> SceneUploadFrame {
    let mut entity_to_model_slot = FxHashMap::default();
    let mut model_matrices = vec![super::IDENTITY_MODEL_MATRIX];
    for phase in &mut extracted.opaque_phases {
        parts.resources.draw_functions.assign_model_matrices(
            phase.items_mut(),
            &inputs.resolved_transforms,
            &mut entity_to_model_slot,
            &mut model_matrices,
        );
    }
    for phase in &mut extracted.transparent_phases {
        parts.resources.draw_functions.assign_model_matrices(
            phase.items_mut(),
            &inputs.resolved_transforms,
            &mut entity_to_model_slot,
            &mut model_matrices,
        );
    }

    let mut previous_model_matrices = model_matrices.clone();
    for (entity, slot) in &entity_to_model_slot {
        if let Some(previous) = parts.runtime.previous_model_by_entity.get(entity) {
            previous_model_matrices[*slot as usize] = *previous;
        }
    }
    let previous_model_matrices = PreviousModelMatrices(previous_model_matrices);

    let lights = collect_gpu_lights(world, &inputs.resolved_transforms);
    {
        let gpu_scene = parts
            .runtime
            .gpu_scene
            .as_mut()
            .expect("phase runtime should initialize a GpuScene");
        gpu_scene
            .table_mut::<ModelMatrixTable>()
            .set_all(gpu, &model_matrices);
        gpu_scene.table_mut::<LightTable>().set_all(gpu, &lights);
        gpu_scene.upload_all(gpu.queue());
    }

    SceneUploadFrame {
        lights,
        model_matrices,
        previous_model_matrices,
    }
}
