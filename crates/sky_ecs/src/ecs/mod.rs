//! Core ECS (Entity Component System) module.
//!
//! This module provides the primary API surface for Sky Engine:
//!
//! - [`World`] — the central container for entities, components, and resources.
//! - [`EntityId`] — a generational handle to a live entity.
//! - [`CommandBuffer`] — an owned deferred buffer for manual structural changes.
//! - [`Commands`] — a borrowed deferred writer available as a system parameter.
//! - [`Bundle`] — trait implemented for component tuples used in [`World::spawn`].
//! - [`Query`] / [`QueryMut`] — world-bound typed query facades.
//! - [`PreparedQuery`] — an explicit reusable query plan for advanced hot paths.
//! - [`With`] / [`Without`] — archetype-level query filters.
//! - [`View`], [`ParView`], [`Res`], and [`Commands`] — inferred typed system parameters.
//! - [`StageLabel`] — type-level labels used by [`World::stage`].
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
mod system;
pub(crate) mod time;
mod world;

pub use bundle::Bundle;
pub use commands::{CommandBuffer, Commands};
pub use entity::EntityId;
pub use query::{Any, PreparedQuery, Query, QueryFilter, QueryMut, With, Without};
pub use sky_ecs_derive::{QueryData, StageLabel};
pub use sky_type::{
    query_by_name as component_type_by_name, query_by_rust_type as component_type_by_rust_type,
    register as register_component_type, registered_types as registered_component_types,
    type_of as component_type, Type as ComponentType, TypeInfo as ComponentTypeInfo,
};
pub use system::{
    CommandDiagnostics, ExclusiveSystem, First, FixedOverflow, FixedStageDiagnostics, FixedStep,
    FixedUpdate, IntoSystem, Last, Local, ParView, PostUpdate, PreUpdate, Res, ResMut,
    ScheduleBuildError, ScheduleDiagnostics, ScheduleError, StageBuilder, StageDiagnostics,
    StageLabel, StageSegmentDiagnostics, SystemAccessDiagnostics, SystemDiagnostics, TickReport,
    Update, View,
};
pub use time::Time;
pub use world::World;

/// Built-in schedule labels and fixed-step configuration.
pub mod stage {
    pub use super::{
        First, FixedOverflow, FixedStep, FixedUpdate, Last, PostUpdate, PreUpdate, StageLabel,
        Update,
    };
}

/// Implementation details used by the `QueryData` derive macro.
///
/// This module is public only so macro expansions can name the unsafe query
/// contract. It is not part of the supported hand-written API surface.
#[doc(hidden)]
pub mod __private {
    pub use super::chunk::Chunk;
    pub use super::query::{QueryDescriptor, QueryParam, QuerySpec, ReadOnlyQuerySpec};
}

pub(crate) use archetype::*;
pub(crate) use chunk::*;
pub(crate) use query::*;
