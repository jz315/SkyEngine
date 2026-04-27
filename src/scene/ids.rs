use std::fmt;
use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};

/// Stable entity ID stored in scene/prefab documents.
///
/// This is distinct from [`EntityId`](crate::ecs::EntityId), which is only
/// valid inside a running [`World`](crate::ecs::World).
#[derive(Clone, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SceneEntityId(String);

impl SceneEntityId {
    #[inline]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    #[inline]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl PartialEq for SceneEntityId {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Hash for SceneEntityId {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl fmt::Debug for SceneEntityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("SceneEntityId").field(&self.0).finish()
    }
}

impl fmt::Display for SceneEntityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for SceneEntityId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for SceneEntityId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}
