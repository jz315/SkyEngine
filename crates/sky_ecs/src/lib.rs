//! Chunk-based ECS core for Sky.
//!
//! The crate owns entities, component storage, typed queries, resources,
//! commands, scheduling, and the lightweight plugin installation protocol.

pub mod ecs;
pub mod plugin;

pub use ecs::*;
pub use plugin::{Plugin, PluginError, PluginRegistry, PluginResult};
