//! Native retained-mode UI for game HUDs and menus.
//!
//! The v1 UI path is ECS-first and screen-space only: every panel, label,
//! button, and progress bar is a normal entity with UI components.

mod backend;
mod backends;
mod components;
mod input;
mod layout;
mod render;
mod state;
mod text;

pub use backend::{
    ensure_ui_host, render_ui_overlays, try_with_ui_host_mut, ui_wants_keyboard, ui_wants_pointer,
    update_ui_backends, with_ui_backend_mut, UiBackend, UiBackendId, UiBeginFrameContext,
    UiCaptureState, UiError, UiHost, UiRenderContext,
};
pub use backends::legacy::LegacyUiBackend;
pub use components::{
    UiAlign, UiAnchor, UiButton, UiId, UiImage, UiInteraction, UiLayout, UiLength, UiNode, UiPanel,
    UiProgressBar, UiRect, UiScroll, UiSlider, UiText, UiToggle,
};
pub use input::update_ui;
pub use render::render_ui;
pub use state::{UiConfig, UiEvent, UiEventKind, UiEvents, UiState, UiTheme};
pub use text::{UiFontBook, UiFontSource};

pub(crate) use layout::{hit_test_input, rect_map, resolve_world_layout};
pub(crate) use text::preferred_text_size;

use crate::ecs::World;

#[derive(Clone, Debug, Default)]
pub struct UiPlugin {
    pub config: UiConfig,
}

impl UiPlugin {
    pub fn new(config: UiConfig) -> Self {
        Self { config }
    }

    pub fn install(self, world: &mut World) {
        install_ui(world, self.config);
    }
}

/// Install native UI resources with explicit configuration.
///
/// Most applications do not need to call this directly. Use [`UiPlugin`] for
/// explicit configuration, or let [`update_ui`] / [`render_ui`] lazily install
/// default resources.
pub fn install_ui(world: &mut World, config: UiConfig) {
    world.insert_resource(config.clone());
    install_ui_resources(world, &config);
    install_legacy_backend(world);
}

pub(crate) fn ensure_ui_resources(world: &mut World) {
    ensure_legacy_ui_resources(world);
    install_legacy_backend(world);
}

pub(crate) fn ensure_legacy_ui_resources(world: &mut World) {
    let config = world
        .get_resource::<UiConfig>()
        .cloned()
        .unwrap_or_default();
    if world.get_resource::<UiConfig>().is_none() {
        world.insert_resource(config.clone());
    }
    install_ui_resources(world, &config);
}

fn install_ui_resources(world: &mut World, config: &UiConfig) {
    if world.get_resource::<UiTheme>().is_none() {
        world.insert_resource(UiTheme::default());
    }
    if world.get_resource::<UiState>().is_none() {
        world.insert_resource(UiState::default());
    }
    if world.get_resource::<UiEvents>().is_none() {
        world.insert_resource(UiEvents::default());
    }
    if world.get_resource::<UiFontBook>().is_none() {
        let fonts = if config.load_system_fonts {
            UiFontBook::new()
        } else {
            UiFontBook::empty()
        };
        world.insert_resource(fonts);
    }
}

fn install_legacy_backend(world: &mut World) {
    ensure_ui_host(world);
    backend::try_with_ui_host_mut(world, |host, world| {
        if !host.contains(LegacyUiBackend::ID) {
            host.register(LegacyUiBackend::new());
        }
        if let Some(legacy) = host.backend_mut::<LegacyUiBackend>() {
            let capture = backends::legacy::legacy_capture(world.get_resource::<UiState>());
            legacy.set_capture(capture);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_ui_resources_installs_default_runtime_resources() {
        let mut world = World::new();

        ensure_ui_resources(&mut world);

        assert!(world.get_resource::<UiConfig>().is_some());
        assert!(world.get_resource::<UiTheme>().is_some());
        assert!(world.get_resource::<UiState>().is_some());
        assert!(world.get_resource::<UiEvents>().is_some());
        assert!(world.get_resource::<UiFontBook>().is_some());
    }

    #[test]
    fn ui_plugin_installs_configured_runtime_resources() {
        let mut world = World::new();

        UiPlugin::new(UiConfig {
            load_system_fonts: false,
        })
        .install(&mut world);

        assert!(!world.get_resource::<UiConfig>().unwrap().load_system_fonts);
        assert!(world.get_resource::<UiTheme>().is_some());
        assert!(world.get_resource::<UiState>().is_some());
        assert!(world.get_resource::<UiEvents>().is_some());
        assert!(world.get_resource::<UiFontBook>().is_some());
    }
}
