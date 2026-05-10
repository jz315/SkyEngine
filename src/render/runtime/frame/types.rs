use crate::render::execution::FrameExecutionStats;
use crate::render::lighting::shadow::{DirectionalShadowSetup, ShadowDebugResources};
use crate::render::phase::{OpaquePhase, TransparentPhase};
use crate::render::view::SceneView;
use crate::render::GpuLight;

use super::super::executor::RenderExecutor;
use super::super::state::{FrameRuntimeState, RenderResourceHub, RuntimePlan, ShadowRuntime};
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
    pub(crate) shadows: &'a mut ShadowRuntime,
    pub(crate) executor: &'a mut RenderExecutor,
}

pub(crate) struct ExtractedFrame {
    pub(crate) views: Vec<SceneView>,
    pub(crate) opaque_phases: Vec<OpaquePhase>,
    pub(crate) transparent_phases: Vec<TransparentPhase>,
    pub(crate) shadow_setups: Vec<DirectionalShadowSetup>,
}

pub(crate) struct SceneUploadFrame {
    pub(crate) lights: Vec<GpuLight>,
    pub(crate) model_matrices: Vec<[f32; 16]>,
    pub(crate) previous_model_matrices: PreviousModelMatrices,
}

pub(crate) struct PreviousModelMatrices(pub(crate) Vec<[f32; 16]>);

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ShadowFrameStats {
    pub(crate) cascade_count: usize,
    pub(crate) caster_count: usize,
    pub(crate) caster_count_by_cascade:
        [usize; crate::render::component::MAX_DIRECTIONAL_SHADOW_CASCADES],
    pub(crate) atlas_size: [u32; 2],
    pub(crate) rect_count: usize,
    pub(crate) used_pixel_ratio: f32,
    pub(crate) guard_band_texels: f32,
}

pub(crate) struct ShadowFrameSummary {
    pub(crate) debug_resources: Option<ShadowDebugResources>,
    pub(crate) stats: ShadowFrameStats,
    pub(crate) draw_calls: usize,
    pub(crate) draw_calls_by_cascade:
        [usize; crate::render::component::MAX_DIRECTIONAL_SHADOW_CASCADES],
}

pub(crate) struct FrameExecutionSummary {
    pub(crate) stats: FrameExecutionStats,
    pub(crate) execute_ms: f64,
}
