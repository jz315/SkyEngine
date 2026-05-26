use std::fmt;

use crate::ecs::EntityId;

use super::PersistId;

/// Errors produced while validating, saving, or loading persistence documents.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PersistError {
    EmptyPersistId,
    DuplicatePersistId(PersistId),
    DuplicateTransform(PersistId),
    DuplicatePersistComponent {
        entity: PersistId,
        type_name: String,
    },
    DuplicateComponentType(String),
    UnregisteredComponentType(String),
    MissingRuntimeEntity(EntityId),
    MissingPersistEntity(EntityId),
    DuplicateRuntimeEntity(EntityId),
    ComponentSerde {
        type_name: String,
        error: String,
    },
    MissingPrefabRoot,
    Json(String),
    Io(String),
}

impl fmt::Display for PersistError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyPersistId => f.write_str("persist IDs must not be empty"),
            Self::DuplicatePersistId(id) => {
                write!(f, "duplicate persist ID '{id}'")
            }
            Self::DuplicateTransform(id) => {
                write!(
                    f,
                    "persist entity '{id}' has more than one Transform component"
                )
            }
            Self::DuplicatePersistComponent { entity, type_name } => {
                write!(
                    f,
                    "persist entity '{entity}' has more than one '{type_name}' component"
                )
            }
            Self::DuplicateComponentType(type_name) => {
                write!(
                    f,
                    "persist component type '{type_name}' is already registered"
                )
            }
            Self::UnregisteredComponentType(type_name) => {
                write!(f, "persist component type '{type_name}' is not registered")
            }
            Self::MissingRuntimeEntity(entity) => {
                write!(f, "runtime entity '{entity:?}' does not exist")
            }
            Self::MissingPersistEntity(entity) => {
                write!(f, "runtime entity '{entity:?}' has no PersistEntity ID")
            }
            Self::DuplicateRuntimeEntity(entity) => {
                write!(
                    f,
                    "runtime entity '{entity:?}' appears more than once in the captured hierarchy"
                )
            }
            Self::ComponentSerde { type_name, error } => {
                write!(f, "persist component '{type_name}' serde error: {error}")
            }
            Self::MissingPrefabRoot => f.write_str("prefab document must contain one root node"),
            Self::Json(error) => write!(f, "persistence JSON error: {error}"),
            Self::Io(error) => write!(f, "persistence I/O error: {error}"),
        }
    }
}

impl std::error::Error for PersistError {}

impl From<serde_json::Error> for PersistError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error.to_string())
    }
}

impl From<std::io::Error> for PersistError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.to_string())
    }
}
