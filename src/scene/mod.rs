//! Persistence and prefab documents for ECS entity trees.
//!
//! The main API is [`Persistence`]: users keep spawning ordinary ECS entities,
//! mark saveable components with `#[persist(component)]`, and then save a World
//! or prefab subtree. The module owns stable document IDs, hierarchy metadata,
//! serde-backed component payloads, and the in-memory [`PersistDocument`] layer.

mod components;
mod document;
mod errors;
mod ids;
mod persistence;
mod serialize;
mod validation;
mod value;

#[cfg(test)]
mod tests;

pub use components::{Children, Name, Parent};
pub(crate) use components::{PersistEntity, PersistRoot};
pub(crate) use document::TRANSFORM_COMPONENT_TYPE;
pub(crate) use document::{PersistComponents, PersistDocumentData, PersistNode};
pub use errors::PersistError;
pub use ids::PersistId;
pub use persistence::{
    Persist, PersistDocument, PersistPrefabInstance, PersistRegistration, PersistWorldInstance,
    Persistence,
};
pub use sky_engine_reflect_derive::persist;
pub(crate) use value::PersistValue;

#[doc(hidden)]
pub mod __private {
    pub use inventory;
}
