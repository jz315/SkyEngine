use std::fmt;

use crate::ecs::EntityId;

use super::SceneEntityId;

/// Errors produced while validating or spawning scene/prefab documents.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SceneError {
    EmptySceneEntityId,
    DuplicateSceneEntityId(SceneEntityId),
    DuplicateTransform(SceneEntityId),
    DuplicateSceneComponent {
        entity: SceneEntityId,
        type_name: String,
    },
    DuplicateComponentType(String),
    UnregisteredComponentType(String),
    MissingRuntimeEntity(EntityId),
    MissingSceneEntity(EntityId),
    DuplicateRuntimeEntity(EntityId),
    ComponentSerde {
        type_name: String,
        error: String,
    },
    MissingPrefabRoot,
    Json(String),
    Io(String),
}

impl fmt::Display for SceneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptySceneEntityId => f.write_str("scene entity IDs must not be empty"),
            Self::DuplicateSceneEntityId(id) => {
                write!(f, "duplicate scene entity ID '{id}'")
            }
            Self::DuplicateTransform(id) => {
                write!(
                    f,
                    "scene entity '{id}' has more than one Transform component"
                )
            }
            Self::DuplicateSceneComponent { entity, type_name } => {
                write!(
                    f,
                    "scene entity '{entity}' has more than one '{type_name}' component"
                )
            }
            Self::DuplicateComponentType(type_name) => {
                write!(
                    f,
                    "scene component type '{type_name}' is already registered"
                )
            }
            Self::UnregisteredComponentType(type_name) => {
                write!(f, "scene component type '{type_name}' is not registered")
            }
            Self::MissingRuntimeEntity(entity) => {
                write!(f, "runtime entity '{entity:?}' does not exist")
            }
            Self::MissingSceneEntity(entity) => {
                write!(f, "runtime entity '{entity:?}' has no SceneEntity ID")
            }
            Self::DuplicateRuntimeEntity(entity) => {
                write!(
                    f,
                    "runtime entity '{entity:?}' appears more than once in the captured hierarchy"
                )
            }
            Self::ComponentSerde { type_name, error } => {
                write!(f, "scene component '{type_name}' serde error: {error}")
            }
            Self::MissingPrefabRoot => f.write_str("prefab document must contain one root node"),
            Self::Json(error) => write!(f, "scene JSON error: {error}"),
            Self::Io(error) => write!(f, "scene I/O error: {error}"),
        }
    }
}

impl std::error::Error for SceneError {}

impl From<serde_json::Error> for SceneError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error.to_string())
    }
}

impl From<std::io::Error> for SceneError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.to_string())
    }
}
