mod backend;
mod components;
mod input;
mod layout;
mod plugin;
mod render;
mod state;
mod text;

pub use backend::LegacyUiBackend;
pub use components::{
    UiAlign, UiAnchor, UiButton, UiId, UiImage, UiInteraction, UiLayout, UiLength, UiNode, UiPanel,
    UiProgressBar, UiRect, UiScroll, UiSlider, UiText, UiToggle,
};
pub use input::update_ui;
pub use plugin::UiPlugin;
pub use render::render_ui;
pub use state::{UiConfig, UiEvent, UiEventKind, UiEvents, UiState, UiTheme};
pub use text::{UiFontBook, UiFontSource};

pub(crate) use backend::legacy_capture;
pub(crate) use layout::{hit_test_input, rect_map, resolve_world_layout};
pub(crate) use plugin::{ensure_legacy_ui_resources, ensure_ui_resources};
pub(crate) use text::preferred_text_size;
