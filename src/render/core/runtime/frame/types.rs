use crate::render::execution::FrameExecutionStats;
use crate::render::phase::{OpaquePhase, TransparentPhase};
use crate::render::resources::GpuLight;
use crate::render::view::SceneView;

use super::super::executor::RenderExecutor;
use super::super::state::{FrameRuntimeState, RenderResourceHub, RuntimePlan};
use super::super::stats::TimingStart;

pub(crate) const IDENTITY_MODEL_MATRIX: [f32; 16] = [
    1.0, 0.0, 0.0, 0.0, //
    0.0, 1.0, 0.0, 0.0, //
    0.0, 0.0, 1.0, 0.0, //
    0.0, 0.0, 0.0, 1.0,
];

pub(crate) struct FrameInputs {
    pub(crate) frame_start: TimingStart,
    pub(crate) resolved_transforms: crate::render::view::ResolvedSceneTransforms,
}

pub(crate) struct FrameRuntimeParts<'a> {
    pub(crate) plan: &'a mut RuntimePlan,
    pub(crate) resources: &'a mut RenderResourceHub,
    pub(crate) runtime: &'a mut FrameRuntimeState,
    pub(crate) executor: &'a mut RenderExecutor,
}

#[derive(Default)]
pub(crate) struct ExtractedFrame {
    pub(crate) views: Vec<SceneView>,
    pub(crate) opaque_phases: Vec<OpaquePhase>,
    pub(crate) transparent_phases: Vec<TransparentPhase>,
}

#[derive(Default)]
pub(crate) struct SceneUploadFrame {
    pub(crate) lights: Vec<GpuLight>,
    pub(crate) model_matrices: Vec<[f32; 16]>,
    pub(crate) previous_model_matrices: PreviousModelMatrices,
    pub(crate) entity_to_model_slot: rustc_hash::FxHashMap<crate::ecs::EntityId, u32>,
}

#[derive(Default)]
pub(crate) struct PreviousModelMatrices(pub(crate) Vec<[f32; 16]>);

pub(crate) struct FrameExecutionSummary {
    pub(crate) stats: FrameExecutionStats,
    pub(crate) execute_ms: f64,
    pub(crate) succeeded: bool,
}
