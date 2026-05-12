//! Non-invasive module installation protocol.

use std::error::Error;
use std::fmt;

use crate::ecs::World;

/// Result type returned by engine module plugins.
pub type PluginResult = Result<(), PluginError>;

/// Error returned when a plugin cannot install itself into a [`World`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginError {
    pub plugin: &'static str,
    pub message: String,
}

impl PluginError {
    pub fn new(plugin: &'static str, message: impl Into<String>) -> Self {
        Self {
            plugin,
            message: message.into(),
        }
    }
}

impl fmt::Display for PluginError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} plugin failed: {}", self.plugin, self.message)
    }
}

impl Error for PluginError {}

/// A non-invasive installer for an engine module.
///
/// Plugins receive a mutable [`World`] and may insert resources, register
/// systems, or install module-local backends. The ECS [`World`] itself does
/// not expose plugin-specific methods.
pub trait Plugin {
    fn name(&self) -> &'static str;

    fn install(self, world: &mut World) -> PluginResult
    where
        Self: Sized;
}
