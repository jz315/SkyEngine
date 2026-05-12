//! Native retained-mode UI for game HUDs and menus.
//!
//! The v1 UI path is ECS-first and screen-space only: every panel, label,
//! button, and progress bar is a normal entity with UI components.

mod backend;
mod backends;
#[cfg(feature = "ui-legacy")]
mod components;
#[cfg(feature = "ui-legacy")]
mod input;
#[cfg(feature = "ui-legacy")]
mod layout;
#[cfg(feature = "ui-legacy")]
mod render;
#[cfg(feature = "ui-legacy")]
mod state;
#[cfg(feature = "ui-legacy")]
mod text;

pub use backend::{
    ensure_ui_host, handle_ui_event, render_ui_overlays, try_with_ui_host_mut, ui_wants_keyboard,
    ui_wants_pointer, update_ui_backends, with_ui_backend_mut, UiBackend, UiBackendId,
    UiBeginFrameContext, UiCaptureState, UiError, UiEventContext, UiEventResponse, UiHost,
    UiRenderContext,
};
#[cfg(feature = "ui-legacy")]
pub use backends::legacy::LegacyUiBackend;
#[cfg(feature = "yakui-ui")]
pub use backends::yakui::{YakuiBackend, YakuiUiPlugin};
#[cfg(feature = "ui-legacy")]
pub use components::{
    UiAlign, UiAnchor, UiButton, UiId, UiImage, UiInteraction, UiLayout, UiLength, UiNode, UiPanel,
    UiProgressBar, UiRect, UiScroll, UiSlider, UiText, UiToggle,
};
#[cfg(feature = "ui-legacy")]
pub use input::update_ui;
#[cfg(feature = "ui-legacy")]
pub use render::render_ui;
#[cfg(feature = "ui-legacy")]
pub use state::{UiConfig, UiEvent, UiEventKind, UiEvents, UiState, UiTheme};
#[cfg(feature = "ui-legacy")]
pub use text::{UiFontBook, UiFontSource};

#[cfg(feature = "ui-legacy")]
pub(crate) use layout::{hit_test_input, rect_map, resolve_world_layout};
#[cfg(feature = "ui-legacy")]
pub(crate) use text::preferred_text_size;

use crate::ecs::World;
#[cfg(feature = "ui-legacy")]
use crate::plugin::{Plugin, PluginResult};

#[cfg(feature = "ui-legacy")]
#[derive(Clone, Debug, Default)]
pub struct UiPlugin {
    pub config: UiConfig,
}

#[cfg(feature = "ui-legacy")]
impl UiPlugin {
    pub fn new(config: UiConfig) -> Self {
        Self { config }
    }
}

#[cfg(feature = "ui-legacy")]
impl Plugin for UiPlugin {
    fn name(&self) -> &'static str {
        "ui"
    }

    fn install(self, world: &mut World) -> PluginResult {
        install_ui_plugin(world, self.config);
        Ok(())
    }
}

/// Install native UI resources with explicit configuration.
///
/// Most applications do not need to call this directly. Use [`UiPlugin`] for
/// explicit configuration, or let [`update_ui`] / [`render_ui`] lazily install
/// default resources.
#[cfg(feature = "ui-legacy")]
fn install_ui_plugin(world: &mut World, config: UiConfig) {
    world.insert_resource(config.clone());
    install_ui_resources(world, &config);
    install_legacy_backend(world);
}

pub(crate) fn ensure_ui_resources(world: &mut World) {
    #[cfg(feature = "ui-legacy")]
    {
        ensure_legacy_ui_resources(world);
        install_legacy_backend(world);
    }
    #[cfg(not(feature = "ui-legacy"))]
    let _ = world;
}

#[cfg(feature = "ui-legacy")]
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

#[cfg(feature = "ui-legacy")]
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

#[cfg(feature = "ui-legacy")]
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
    #[cfg(feature = "ui-legacy")]
    use super::*;

    #[cfg(feature = "ui-legacy")]
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

    #[cfg(feature = "ui-legacy")]
    #[test]
    fn ui_plugin_installs_configured_runtime_resources() {
        use crate::plugin::Plugin;

        let mut world = World::new();

        UiPlugin::new(UiConfig {
            load_system_fonts: false,
        })
        .install(&mut world)
        .unwrap();

        assert!(!world.get_resource::<UiConfig>().unwrap().load_system_fonts);
        assert!(world.get_resource::<UiTheme>().is_some());
        assert!(world.get_resource::<UiState>().is_some());
        assert!(world.get_resource::<UiEvents>().is_some());
        assert!(world.get_resource::<UiFontBook>().is_some());
    }
}
