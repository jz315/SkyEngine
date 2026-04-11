mod access;
mod helpers;
mod nodes;
mod payload;
mod pipeline;
mod slots;
#[cfg(test)]
mod tests;

pub type TextureFormat = wgpu::TextureFormat;

pub use nodes::{
    FinalizeExecutionContext, FrameFinalizeNode, FrameSetupNode, FrameViewNode,
    SetupExecutionContext, ViewExecutionContext,
};
pub use payload::{FramePayloadStore, PreparedFrame, PreparedView, ViewPayloadStore};
pub use pipeline::{FrameExecutionStats, FramePipeline};
pub use slots::{
    CompletedViewState, FinalizePhaseState, PhaseState, ResourceSlotMap, SlotResource, TextureSlot,
};

pub(crate) use access::{FrameContextExecuteAccess, FrameContextSetupAccess};
pub(crate) use helpers::{
    pass_first_read_texture, pass_first_write_texture, pass_nth_read_texture,
    require_current_color, require_render_target,
};
