use std::any::Any;

use super::super::{
    render_ui, update_ui, UiBackend, UiBackendId, UiBeginFrameContext, UiCaptureState, UiError,
    UiRenderContext, UiState,
};

/// Adapter that exposes the existing ECS retained UI as a pluggable backend.
#[derive(Debug, Default)]
pub struct LegacyUiBackend {
    capture: UiCaptureState,
}

impl LegacyUiBackend {
    pub const ID: UiBackendId = UiBackendId::new("legacy");

    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn set_capture(&mut self, capture: UiCaptureState) {
        self.capture = capture;
    }
}

impl UiBackend for LegacyUiBackend {
    fn id(&self) -> UiBackendId {
        Self::ID
    }

    fn name(&self) -> &'static str {
        "legacy"
    }

    fn begin_frame(&mut self, ctx: UiBeginFrameContext<'_>) {
        update_ui(ctx.world, ctx.input, ctx.surface_size);
        self.capture = legacy_capture(ctx.world.get_resource::<UiState>());
    }

    fn render_overlay(&mut self, ctx: UiRenderContext<'_>) -> Result<(), UiError> {
        render_ui(ctx.world, ctx.gpu);
        Ok(())
    }

    fn capture(&self) -> UiCaptureState {
        self.capture
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

pub(crate) fn legacy_capture(state: Option<&UiState>) -> UiCaptureState {
    UiCaptureState::new(state.is_some_and(UiState::wants_pointer), false)
}
