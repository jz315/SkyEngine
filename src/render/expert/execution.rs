//! Prepared-frame execution, pass contexts, frame pipeline, and view data.

pub use crate::math::{Projection, Quat, Transform};
pub use crate::render::execution::{
    CompletedViewState, ComputePassExecuteContext, ComputePassSetupContext,
    FinalizeExecutionContext, FinalizePhaseState, FrameExecutionStats, FrameFinalizeNode,
    FramePayloadStore, FramePipeline, FrameSetupNode, FrameViewNode, GraphPassExecuteContext,
    GraphPassSetupContext, PhaseDrawServices, PhaseExecuteContext, PhaseSetupContext, PhaseState,
    PostFxPassExecuteContext, PostFxPassSetupContext, PreparedFrame, PreparedView,
    RenderPassExecuteContext, RenderPassSetupContext, ResourceSlotMap, SceneTexture,
    SetupExecutionContext, SlotResource, TextureFormat, TextureSlot, ViewExecutionContext,
    ViewPayloadStore,
};
pub use crate::render::extract::{
    ExtractContext, ExtractError, ExtractSchedule, ExtractSprites, Extractor,
};
pub use crate::render::runtime::{HistoryTexture, HistoryTextureRequest, HistoryTextureSize};
pub use crate::render::view::{
    Camera, Frustum, RenderView, SceneView, TemporalViewState, ViewUniform, ViewportRect,
};
