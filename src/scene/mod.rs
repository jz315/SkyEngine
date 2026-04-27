//! Scene and prefab documents for organizing ECS entity trees.
//!
//! The v1 scene module is intentionally small: it stores stable document IDs,
//! names, hierarchy links, and engine-owned component data, then spawns those
//! documents into a [`World`](crate::ecs::World). It does not replace ECS; it
//! gives ECS entities a reusable asset-like structure.

mod capture;
mod components;
mod document;
mod errors;
mod ids;
mod runtime;
mod serialize;
mod spawn;
mod validation;
mod value;

#[cfg(test)]
mod tests;

pub use capture::{capture_prefab, capture_scene};
pub use components::{Children, Name, Parent, SceneEntity, SceneRoot};
pub(crate) use document::TRANSFORM_COMPONENT_TYPE;
pub use document::{PrefabDocument, PrefabSpawnOptions, SceneComponents, SceneDocument, SceneNode};
pub use errors::SceneError;
pub use ids::SceneEntityId;
pub use runtime::SceneRuntime;
pub use spawn::{
    despawn_prefab_instance, despawn_scene_instance, spawn_prefab, spawn_scene, PrefabInstance,
    SceneInstance,
};
pub use value::SceneValue;
