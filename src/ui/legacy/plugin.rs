use crate::ecs::World;
use crate::plugin::{Plugin, PluginResult};

use super::{legacy_capture, LegacyUiBackend, UiConfig, UiEvents, UiFontBook, UiState, UiTheme};
use crate::ui::{ensure_ui_host, try_with_ui_host_mut};

#[derive(Clone, Debug, Default)]
pub struct UiPlugin {
    pub config: UiConfig,
}

impl UiPlugin {
    pub fn new(config: UiConfig) -> Self {
        Self { config }
    }
}

impl Plugin for UiPlugin {
    fn name(&self) -> &'static str {
        "ui-legacy"
    }

    fn install(self, world: &mut World) -> PluginResult {
        install_ui_plugin(world, self.config);
        Ok(())
    }
}

/// Install native UI resources with explicit configuration.
///
/// Most applications do not need to call this directly. Use [`UiPlugin`] for
/// explicit configuration, or let [`super::update_ui`] / [`super::render_ui`]
/// lazily install default resources.
fn install_ui_plugin(world: &mut World, config: UiConfig) {
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
    try_with_ui_host_mut(world, |host, world| {
        if !host.contains(LegacyUiBackend::ID) {
            host.register(LegacyUiBackend::new());
        }
        if let Some(legacy) = host.backend_mut::<LegacyUiBackend>() {
            let capture = legacy_capture(world.get_resource::<UiState>());
            legacy.set_capture(capture);
        }
    });
}

#[cfg(test)]
mod tests {
    use crate::plugin::Plugin;

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
        .install(&mut world)
        .unwrap();

        assert!(!world.get_resource::<UiConfig>().unwrap().load_system_fonts);
        assert!(world.get_resource::<UiTheme>().is_some());
        assert!(world.get_resource::<UiState>().is_some());
        assert!(world.get_resource::<UiEvents>().is_some());
        assert!(world.get_resource::<UiFontBook>().is_some());
    }
}
