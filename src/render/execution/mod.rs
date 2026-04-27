mod frame_pipeline;
mod helpers;
mod nodes;
mod payload;
mod slots;
#[cfg(test)]
mod tests;

pub type TextureFormat = wgpu::TextureFormat;

pub use frame_pipeline::{FrameExecutionStats, FramePipeline};
pub use nodes::{
    FinalizeExecutionContext, FrameFinalizeNode, FrameSetupNode, FrameViewNode,
    SetupExecutionContext, ViewExecutionContext,
};
pub use payload::{FramePayloadStore, PreparedFrame, PreparedView, ViewPayloadStore};
pub use slots::{
    CompletedViewState, FinalizePhaseState, PhaseState, ResourceSlotMap, SlotResource, TextureSlot,
};

pub(crate) use helpers::{
    bind_current_as_scene_color, create_scene_texture, ensure_scene_texture,
    pass_first_read_texture, pass_first_write_texture, pass_nth_write_texture,
    require_current_color, require_render_target, SceneTextureKind,
};
