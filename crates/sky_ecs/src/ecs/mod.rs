//! Core ECS (Entity Component System) module.
//!
//! This module provides the primary API surface for Sky Engine:
//!
//! - [`World`] — the central container for entities, components, and resources.
//! - [`EntityId`] — a generational handle to a live entity.
//! - [`Commands`] — a deferred command buffer for structural changes.
//! - [`Bundle`] — trait implemented for component tuples used in [`World::spawn`].
//! - [`PreparedQuery`] — the typed, cached query API.
//! - [`With`] / [`Without`] — archetype-level query filters.
//! - [`System`] — trait for runnable systems scheduled via [`World::group`].
//! - [`Time`] — per-frame timing information.

mod archetype;
mod bundle;
mod chunk;
mod commands;
pub mod dynamic;
mod entity;
pub mod expert;
mod query;
mod resource;
pub(crate) mod system;
pub(crate) mod time;
mod world;

pub use bundle::Bundle;
pub use commands::Commands;
pub use entity::EntityId;
pub use query::{PreparedQuery, With, Without};
pub use sky_type::{
    query_by_name as component_type_by_name, query_by_rust_type as component_type_by_rust_type,
    register as register_component_type, registered_types as registered_component_types,
    type_of as component_type, Type as ComponentType, TypeInfo as ComponentTypeInfo,
};
pub use system::System;
pub use time::Time;
pub use world::World;

pub(crate) use archetype::*;
pub(crate) use chunk::*;
pub(crate) use query::*;
