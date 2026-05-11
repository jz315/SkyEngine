mod host;
mod traits;

pub use host::{
    ensure_ui_host, handle_ui_event, render_ui_overlays, try_with_ui_host_mut, ui_wants_keyboard,
    ui_wants_pointer, update_ui_backends, with_ui_backend_mut, UiHost,
};
pub use traits::{
    UiBackend, UiBackendId, UiBeginFrameContext, UiCaptureState, UiError, UiEventContext,
    UiEventResponse, UiRenderContext,
};
