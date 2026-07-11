/// Result of executing the high-level wgpu render runtime for one acquired
/// frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameRenderOutcome {
    Rendered,
    Skipped(FrameSkipReason),
}

impl FrameRenderOutcome {
    #[inline]
    pub fn is_rendered(self) -> bool {
        matches!(self, Self::Rendered)
    }
}

/// Reason a runtime frame did not draw after frame inputs were collected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameSkipReason {
    ResourcePreparationFailed,
    RenderGraphFailed,
}
