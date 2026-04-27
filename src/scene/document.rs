use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::math::Transform;

use super::serialize::transform_to_scene_value;
use super::validation::{validate_prefab_document, validate_scene_document};
use super::{SceneEntityId, SceneError, SceneValue};

pub(crate) const TRANSFORM_COMPONENT_TYPE: &str = "sky.Transform";

/// Ordered, AI-friendly component payload map for a scene node.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SceneComponents {
    entries: BTreeMap<String, SceneValue>,
    duplicate_type_names: Vec<String>,
}

impl SceneComponents {
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[inline]
    pub fn contains(&self, type_name: &str) -> bool {
        self.entries.contains_key(type_name)
    }

    #[inline]
    pub fn get(&self, type_name: &str) -> Option<&SceneValue> {
        self.entries.get(type_name)
    }

    pub fn insert(
        &mut self,
        type_name: impl Into<String>,
        value: impl Into<SceneValue>,
    ) -> Option<SceneValue> {
        self.entries.insert(type_name.into(), value.into())
    }

    pub fn insert_transform(&mut self, transform: Transform) -> Option<SceneValue> {
        self.insert(
            TRANSFORM_COMPONENT_TYPE,
            transform_to_scene_value(transform),
        )
    }

    pub(crate) fn push_raw(&mut self, type_name: String, value: SceneValue) {
        if self.entries.contains_key(&type_name) {
            self.duplicate_type_names.push(type_name.clone());
        }
        self.entries.insert(type_name, value);
    }

    pub(crate) fn duplicate_type_names(&self) -> &[String] {
        &self.duplicate_type_names
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &SceneValue)> {
        self.entries
            .iter()
            .map(|(type_name, value)| (type_name.as_str(), value))
    }
}

/// One entity node in a scene or prefab document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SceneNode {
    pub id: SceneEntityId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "SceneComponents::is_empty")]
    pub components: SceneComponents,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<SceneNode>,
}

impl SceneNode {
    pub fn new(id: impl Into<SceneEntityId>) -> Self {
        Self {
            id: id.into(),
            name: None,
            components: SceneComponents::new(),
            children: Vec::new(),
        }
    }

    pub fn named(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn with_component(
        mut self,
        type_name: impl Into<String>,
        value: impl Into<SceneValue>,
    ) -> Self {
        self.components.insert(type_name, value);
        self
    }

    pub fn with_transform(mut self, transform: Transform) -> Self {
        self.components.insert_transform(transform);
        self
    }

    pub fn with_child(mut self, child: SceneNode) -> Self {
        self.children.push(child);
        self
    }
}

/// A scene document containing one or more root entity trees.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SceneDocument {
    #[serde(default = "default_scene_version")]
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub roots: Vec<SceneNode>,
}

impl Default for SceneDocument {
    fn default() -> Self {
        Self {
            version: default_scene_version(),
            name: None,
            roots: Vec::new(),
        }
    }
}

impl SceneDocument {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn named(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn with_root(mut self, root: SceneNode) -> Self {
        self.roots.push(root);
        self
    }

    pub fn from_json_str(input: &str) -> Result<Self, SceneError> {
        let scene = serde_json::from_str::<Self>(input)?;
        validate_scene_document(&scene)?;
        Ok(scene)
    }

    pub fn from_json_file(path: impl AsRef<Path>) -> Result<Self, SceneError> {
        Self::from_json_str(&fs::read_to_string(path)?)
    }

    pub fn to_json_string(&self) -> Result<String, SceneError> {
        validate_scene_document(self)?;
        Ok(serde_json::to_string(self)?)
    }

    pub fn to_json_string_pretty(&self) -> Result<String, SceneError> {
        validate_scene_document(self)?;
        Ok(serde_json::to_string_pretty(self)?)
    }

    pub fn write_json_file(&self, path: impl AsRef<Path>) -> Result<(), SceneError> {
        fs::write(path, self.to_json_string_pretty()?)?;
        Ok(())
    }
}

fn default_scene_version() -> u32 {
    1
}

/// A reusable entity tree document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PrefabDocument {
    pub root: SceneNode,
}

impl PrefabDocument {
    #[inline]
    pub fn new(root: SceneNode) -> Self {
        Self { root }
    }

    pub fn from_json_str(input: &str) -> Result<Self, SceneError> {
        let prefab = serde_json::from_str::<Self>(input)?;
        validate_prefab_document(&prefab)?;
        Ok(prefab)
    }

    pub fn from_json_file(path: impl AsRef<Path>) -> Result<Self, SceneError> {
        Self::from_json_str(&fs::read_to_string(path)?)
    }

    pub fn to_json_string(&self) -> Result<String, SceneError> {
        validate_prefab_document(self)?;
        Ok(serde_json::to_string(self)?)
    }

    pub fn to_json_string_pretty(&self) -> Result<String, SceneError> {
        validate_prefab_document(self)?;
        Ok(serde_json::to_string_pretty(self)?)
    }

    pub fn write_json_file(&self, path: impl AsRef<Path>) -> Result<(), SceneError> {
        fs::write(path, self.to_json_string_pretty()?)?;
        Ok(())
    }
}

/// Options for spawning a prefab instance.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PrefabSpawnOptions {
    pub root_transform: Option<Transform>,
}

impl PrefabSpawnOptions {
    #[inline]
    pub const fn new() -> Self {
        Self {
            root_transform: None,
        }
    }

    #[inline]
    pub const fn with_root_transform(mut self, transform: Transform) -> Self {
        self.root_transform = Some(transform);
        self
    }
}
