//! Backend-neutral UI host plus legacy retained UI exports.
//!
//! The legacy retained path is ECS-first and screen-space only: every panel,
//! label, button, and progress bar is a normal entity with UI components.

mod core;
#[cfg(feature = "ui-legacy")]
mod legacy;
#[cfg(feature = "ui-serein")]
pub mod serein;
#[cfg(feature = "yakui-ui")]
pub mod yakui;

pub use core::{
    ensure_ui_host, handle_ui_event, render_ui_overlays, try_with_ui_host_mut, ui_wants_keyboard,
    ui_wants_pointer, update_ui_backends, with_ui_backend_mut, UiBackend, UiBackendId,
    UiBeginFrameContext, UiCaptureState, UiError, UiEventContext, UiEventResponse, UiHost,
    UiRenderContext,
};
#[cfg(feature = "ui-legacy")]
pub use legacy::{
    render_ui, update_ui, LegacyUiBackend, UiAlign, UiAnchor, UiButton, UiConfig, UiEvent,
    UiEventKind, UiEvents, UiFontBook, UiFontSource, UiId, UiImage, UiInteraction, UiLayout,
    UiLength, UiNode, UiPanel, UiPlugin, UiProgressBar, UiRect, UiScroll, UiSlider, UiState,
    UiText, UiTheme, UiToggle,
};
#[cfg(feature = "yakui-ui")]
pub use yakui::{YakuiBackend, YakuiUiPlugin};

use crate::ecs::World;

pub(crate) fn ensure_ui_resources(world: &mut World) {
    #[cfg(feature = "ui-legacy")]
    {
        legacy::ensure_ui_resources(world);
    }
    #[cfg(not(feature = "ui-legacy"))]
    let _ = world;
}
