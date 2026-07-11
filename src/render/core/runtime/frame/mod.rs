mod assemble_frame;
mod collect_frame_inputs;
mod execute_frame;
mod extract_frame;
mod finish_frame;
mod prepare_frame_resources;
mod types;
mod upload_scene;

pub(crate) use assemble_frame::{assemble_prepared_frame, FrameAssemblyInputs};
pub(crate) use collect_frame_inputs::begin_frame_inputs;
pub(crate) use execute_frame::execute_prepared_frame;
pub(crate) use extract_frame::extract_frame;
pub(crate) use finish_frame::{
    finish_frame_stats, finish_skipped_frame_stats, remember_previous_models,
};
pub(crate) use prepare_frame_resources::{
    finish_render_assets, prepare_declared_materials, prepare_frame_assets,
    prepare_frame_extensions,
};
pub(crate) use types::{
    ExtractedFrame, FrameExecutionSummary, FrameInputs, FrameRuntimeParts, PreviousModelMatrices,
    SceneUploadFrame, IDENTITY_MODEL_MATRIX,
};
pub(crate) use upload_scene::upload_scene_data;
