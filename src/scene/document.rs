use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::math::Transform;

use super::serialize::transform_to_persist_value;
use super::validation::validate_persist_document;
use super::{PersistError, PersistId, PersistValue};

pub(crate) const TRANSFORM_COMPONENT_TYPE: &str = "sky.Transform";

/// Ordered, AI-friendly component payload map for a persisted entity node.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PersistComponents {
    entries: BTreeMap<String, PersistValue>,
    duplicate_type_names: Vec<String>,
}

impl PersistComponents {
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
    pub fn get(&self, type_name: &str) -> Option<&PersistValue> {
        self.entries.get(type_name)
    }

    pub fn insert(
        &mut self,
        type_name: impl Into<String>,
        value: impl Into<PersistValue>,
    ) -> Option<PersistValue> {
        self.entries.insert(type_name.into(), value.into())
    }

    pub fn insert_transform(&mut self, transform: Transform) -> Option<PersistValue> {
        self.insert(
            TRANSFORM_COMPONENT_TYPE,
            transform_to_persist_value(transform),
        )
    }

    pub(crate) fn push_raw(&mut self, type_name: String, value: PersistValue) {
        if self.entries.contains_key(&type_name) {
            self.duplicate_type_names.push(type_name.clone());
        }
        self.entries.insert(type_name, value);
    }

    pub(crate) fn duplicate_type_names(&self) -> &[String] {
        &self.duplicate_type_names
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &PersistValue)> {
        self.entries
            .iter()
            .map(|(type_name, value)| (type_name.as_str(), value))
    }
}

/// One entity node in a persistence or prefab document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PersistNode {
    pub id: PersistId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "PersistComponents::is_empty")]
    pub components: PersistComponents,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<PersistNode>,
}

impl PersistNode {
    pub fn new(id: impl Into<PersistId>) -> Self {
        Self {
            id: id.into(),
            name: None,
            components: PersistComponents::new(),
            children: Vec::new(),
        }
    }
}

/// Internal persistence document data containing one or more root entity trees.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PersistDocumentData {
    #[serde(default = "default_document_version")]
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub roots: Vec<PersistNode>,
}

impl Default for PersistDocumentData {
    fn default() -> Self {
        Self {
            version: default_document_version(),
            name: None,
            roots: Vec::new(),
        }
    }
}

impl PersistDocumentData {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_json_str(input: &str) -> Result<Self, PersistError> {
        let data = serde_json::from_str::<Self>(input)?;
        validate_persist_document(&data)?;
        Ok(data)
    }

    pub fn to_json_string(&self) -> Result<String, PersistError> {
        validate_persist_document(self)?;
        Ok(serde_json::to_string(self)?)
    }

    pub fn to_json_string_pretty(&self) -> Result<String, PersistError> {
        validate_persist_document(self)?;
        Ok(serde_json::to_string_pretty(self)?)
    }

    pub fn write_json_file(&self, path: impl AsRef<Path>) -> Result<(), PersistError> {
        fs::write(path, self.to_json_string_pretty()?)?;
        Ok(())
    }
}

fn default_document_version() -> u32 {
    1
}
