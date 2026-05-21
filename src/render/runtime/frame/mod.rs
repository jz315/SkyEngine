mod assemble_frame;
mod collect_frame_inputs;
mod execute_frame;
mod extract_frame;
mod finish_frame;
mod gi;
mod lighting;
mod prepare_frame_resources;
mod shadows;
mod types;
mod upload_scene;

pub(crate) use assemble_frame::{assemble_prepared_frame, FrameAssemblyInputs};
pub(crate) use collect_frame_inputs::begin_frame_inputs;
pub(crate) use execute_frame::execute_prepared_frame;
pub(crate) use extract_frame::extract_frame;
pub(crate) use finish_frame::{
    finish_frame_stats, finish_skipped_frame_stats, remember_previous_models,
};
pub(crate) use gi::prepare_global_illumination;
pub(crate) use lighting::collect_gpu_lights;
pub(crate) use prepare_frame_resources::{
    finish_render_assets, prepare_declared_materials, prepare_frame_assets,
};
pub(crate) use shadows::prepare_shadows;
pub(crate) use types::{
    ExtractedFrame, FrameExecutionSummary, FrameInputs, FrameRuntimeParts, PreviousModelMatrices,
    SceneUploadFrame, ShadowFrameStats, ShadowFrameSummary, IDENTITY_MODEL_MATRIX,
};
pub(crate) use upload_scene::upload_scene_data;
