use crate::ecs::World;
use crate::plugin::{Plugin, PluginResult};
use crate::ui::{ensure_ui_host, try_with_ui_host_mut};

use super::backend::SereinUiBackend;
use super::config::SereinUiConfig;

/// Plugin that installs the Serein backend into `UiHost`.
#[derive(Debug, Clone, Default)]
pub struct SereinUiPlugin {
    pub config: SereinUiConfig,
}

impl SereinUiPlugin {
    pub fn new(config: SereinUiConfig) -> Self {
        Self { config }
    }

    pub fn install(world: &mut World) {
        install_serein_ui_backend(world, SereinUiConfig::default());
    }
}

impl Plugin for SereinUiPlugin {
    fn name(&self) -> &'static str {
        "ui-serein"
    }

    fn install(self, world: &mut World) -> PluginResult {
        install_serein_ui_backend(world, self.config);
        Ok(())
    }
}

/// Install the serein backend if it is not already registered.
pub fn install_serein_ui_backend(world: &mut World, config: SereinUiConfig) {
    ensure_ui_host(world);
    try_with_ui_host_mut(world, |host, _world| {
        if !host.contains(SereinUiBackend::ID) {
            host.register(SereinUiBackend::new(config));
        }
    });
}
