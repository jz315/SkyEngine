use crate::ecs::World;
use crate::plugin::{Plugin, PluginResult};
use crate::ui::{ensure_ui_host, try_with_ui_host_mut};

use super::backend::NeoUiBackend;
use super::config::NeoUiConfig;

/// Plugin that installs the EUI-NEO-style backend into `UiHost`.
#[derive(Debug, Clone, Default)]
pub struct NeoUiPlugin {
    pub config: NeoUiConfig,
}

impl NeoUiPlugin {
    pub fn new(config: NeoUiConfig) -> Self {
        Self { config }
    }

    pub fn install(world: &mut World) {
        install_neo_ui_backend(world, NeoUiConfig::default());
    }
}

impl Plugin for NeoUiPlugin {
    fn name(&self) -> &'static str {
        "ui-neo"
    }

    fn install(self, world: &mut World) -> PluginResult {
        install_neo_ui_backend(world, self.config);
        Ok(())
    }
}

/// Install the neo backend if it is not already registered.
pub fn install_neo_ui_backend(world: &mut World, config: NeoUiConfig) {
    ensure_ui_host(world);
    try_with_ui_host_mut(world, |host, _world| {
        if !host.contains(NeoUiBackend::ID) {
            host.register(NeoUiBackend::new(config));
        }
    });
}
