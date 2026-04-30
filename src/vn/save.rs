use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::vn::preferences::VnPreferences;
use crate::vn::runtime::{VnRuntime, VnRuntimeSnapshot};

pub const VN_SAVE_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VnSaveData {
    pub version: u32,
    pub slot_id: String,
    pub script_id: String,
    pub node: String,
    pub instruction: u32,
    pub runtime: VnRuntimeSnapshot,
    #[serde(default)]
    pub preferences: VnPreferences,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<VnSaveThumbnail>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview_text: Option<String>,
    pub created_at_unix_ms: u64,
}

impl VnSaveData {
    pub fn from_runtime(
        slot_id: impl Into<String>,
        script_id: impl Into<String>,
        runtime: &VnRuntime,
        preferences: VnPreferences,
    ) -> Self {
        let snapshot = runtime.snapshot();
        Self {
            version: VN_SAVE_VERSION,
            slot_id: slot_id.into(),
            script_id: script_id.into(),
            node: snapshot.current_node.clone(),
            instruction: snapshot.instruction_index as u32,
            preview_text: snapshot
                .dialogue
                .current_line
                .as_ref()
                .map(|line| line.text.clone())
                .or_else(|| {
                    snapshot
                        .dialogue
                        .backlog
                        .last()
                        .map(|line| line.text.clone())
                }),
            runtime: snapshot,
            preferences,
            thumbnail: None,
            created_at_unix_ms: unix_time_ms(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VnSaveThumbnail {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VnSaveSlot {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<VnSaveData>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct VnSaveStore {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    slots: BTreeMap<String, VnSaveData>,
}

impl VnSaveStore {
    pub fn save(&mut self, save: VnSaveData) -> Option<VnSaveData> {
        self.slots.insert(save.slot_id.clone(), save)
    }

    pub fn save_runtime(
        &mut self,
        slot_id: impl Into<String>,
        script_id: impl Into<String>,
        runtime: &VnRuntime,
        preferences: VnPreferences,
    ) -> &VnSaveData {
        let slot_id = slot_id.into();
        let save = VnSaveData::from_runtime(slot_id.clone(), script_id, runtime, preferences);
        self.save(save);
        self.slots.get(&slot_id).expect("slot was just inserted")
    }

    pub fn get(&self, slot_id: &str) -> Option<&VnSaveData> {
        self.slots.get(slot_id)
    }

    pub fn remove(&mut self, slot_id: &str) -> Option<VnSaveData> {
        self.slots.remove(slot_id)
    }

    pub fn slots(&self) -> impl Iterator<Item = &VnSaveData> {
        self.slots.values()
    }

    pub fn to_toml_string(&self) -> Result<String, toml::ser::Error> {
        toml::to_string_pretty(self)
    }

    pub fn from_toml_str(text: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(text)
    }

    pub fn save_to_path(&self, path: impl AsRef<Path>) -> Result<(), VnSaveStoreError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| VnSaveStoreError::Io {
                path: parent.to_path_buf(),
                source: source.to_string(),
            })?;
        }
        let text = self
            .to_toml_string()
            .map_err(|source| VnSaveStoreError::Format {
                source: source.to_string(),
            })?;
        fs::write(path, text).map_err(|source| VnSaveStoreError::Io {
            path: path.to_path_buf(),
            source: source.to_string(),
        })
    }

    pub fn load_from_path(path: impl AsRef<Path>) -> Result<Self, VnSaveStoreError> {
        let path = path.as_ref();
        let text = fs::read_to_string(path).map_err(|source| VnSaveStoreError::Io {
            path: path.to_path_buf(),
            source: source.to_string(),
        })?;
        Self::from_toml_str(&text).map_err(|source| VnSaveStoreError::Format {
            source: source.to_string(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VnSaveStoreError {
    Io { path: PathBuf, source: String },
    Format { source: String },
}

impl fmt::Display for VnSaveStoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => {
                write!(f, "VN save IO error at '{}': {source}", path.display())
            }
            Self::Format { source } => write!(f, "VN save format error: {source}"),
        }
    }
}

impl Error for VnSaveStoreError {}

fn unix_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use crate::vn::{VnRuntime, VnRuntimeEvent, YarnScript};

    use super::*;

    #[test]
    fn save_store_round_trips_runtime_snapshot() {
        let script = YarnScript::parse_str(
            r#"
title: Start
---
Hello. #line:start.1
===
"#,
        )
        .unwrap();
        let mut runtime = VnRuntime::from_script(script, "Start").unwrap();
        assert!(matches!(
            runtime.advance().unwrap(),
            VnRuntimeEvent::Line(_)
        ));

        let mut store = VnSaveStore::default();
        store.save_runtime("quick", "memory", &runtime, VnPreferences::default());
        let text = store.to_toml_string().unwrap();
        let loaded = VnSaveStore::from_toml_str(&text).unwrap();
        assert_eq!(loaded.get("quick").unwrap().node, "Start");
        assert_eq!(
            loaded.get("quick").unwrap().runtime.dialogue.backlog.len(),
            1
        );
    }
}
