mod contexts;
mod frame_pipeline;
mod helpers;
mod nodes;
mod payload;
mod slots;
mod step_nodes;
#[cfg(test)]
mod tests;

pub type TextureFormat = wgpu::TextureFormat;

pub use contexts::{
    ComputePassExecuteContext, ComputePassSetupContext, GraphPassExecuteContext,
    GraphPassSetupContext, PhaseDrawServices, PhaseExecuteContext, PhaseSetupContext,
    PostFxPassExecuteContext, PostFxPassSetupContext, RenderPassExecuteContext,
    RenderPassSetupContext,
};
pub(crate) use frame_pipeline::FramePipelineCache;
pub use frame_pipeline::{FrameExecutionStats, FramePipeline};
pub use nodes::{
    FinalizeExecutionContext, FrameFinalizeNode, FrameSetupNode, FrameViewNode,
    SetupExecutionContext, ViewExecutionContext,
};
pub use payload::{FramePayloadStore, PreparedFrame, PreparedView, ViewPayloadStore};
pub use slots::{
    CompletedViewState, FinalizePhaseState, PhaseState, ResourceSlotMap, SceneTexture,
    SlotResource, TextureSlot,
};

pub(crate) use step_nodes::{
    ComputeStepNode, GraphPassStepNode, HeadlessKeepAliveNode, PhaseStepNode, PostFxStepNode,
    RenderPassStepNode, RenderServices, RuntimeRenderServices, SceneColorSeedNode,
};

pub(crate) use helpers::{
    bind_current_as_scene_color, create_scene_texture, ensure_scene_texture,
    pass_first_read_texture, pass_first_write_texture, pass_nth_read_texture,
    pass_nth_write_texture, require_current_color, require_render_target,
};
