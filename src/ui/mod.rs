//! Native retained-mode UI for game HUDs and menus.
//!
//! The v1 UI path is ECS-first and screen-space only: every panel, label,
//! button, and progress bar is a normal entity with UI components.

mod components;
mod input;
mod layout;
mod render;
mod state;
mod text;

pub use components::{
    UiAlign, UiAnchor, UiButton, UiId, UiInteraction, UiLayout, UiLength, UiNode, UiPanel,
    UiProgressBar, UiRect, UiScroll, UiSlider, UiText, UiToggle,
};
pub use input::update_ui;
pub use render::render_ui;
pub use state::{UiConfig, UiEvent, UiEventKind, UiEvents, UiState, UiTheme};
pub use text::{UiFontBook, UiFontSource};

pub(crate) use layout::{hit_test, rect_map, resolve_world_layout};
pub(crate) use text::preferred_text_size;

use crate::ecs::World;

/// Install native UI resources with explicit configuration.
///
/// Most applications do not need to call this. In an [`App`](crate::app::App),
/// configure UI through [`AppConfig::with_ui_config`](crate::app::AppConfig::with_ui_config).
/// [`update_ui`] and [`render_ui`] lazily install resources from the `UiConfig`
/// resource, falling back to defaults for lower-level/manual worlds.
pub fn install_ui(world: &mut World, config: UiConfig) {
    world.insert_resource(config.clone());
    install_ui_resources(world, &config);
}

pub(crate) fn ensure_ui_resources(world: &mut World) {
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
}
