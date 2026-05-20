//! Non-invasive module installation protocol.

use std::any::TypeId;
use std::error::Error;
use std::fmt;

use rustc_hash::FxHashMap;

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

/// Internal record of which plugin types have been installed into a world.
///
/// This is intentionally only an installation fact registry.  It is not a
/// configuration manifest and it does not decide which app capabilities are
/// required.  Plugins use it to reject duplicate installation and to check
/// their own local dependencies.
#[derive(Default)]
pub struct PluginRegistry {
    installed: FxHashMap<TypeId, &'static str>,
}

impl PluginRegistry {
    #[inline]
    pub fn contains<P: 'static>(&self) -> bool {
        self.installed.contains_key(&TypeId::of::<P>())
    }

    #[inline]
    pub fn get<P: 'static>(&self) -> Option<&'static str> {
        self.installed.get(&TypeId::of::<P>()).copied()
    }

    pub fn insert<P: 'static>(&mut self, name: &'static str) -> Result<(), &'static str> {
        let key = TypeId::of::<P>();
        if let Some(existing) = self.installed.get(&key).copied() {
            Err(existing)
        } else {
            self.installed.insert(key, name);
            Ok(())
        }
    }
}

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
