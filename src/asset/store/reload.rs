use std::any::Any;
use std::sync::Arc;

use super::AssetRecord;
use crate::asset::lease::AssetDependencyLeases;
use crate::asset::types::AssetId;

pub(crate) struct AssetReloadBackup {
    pub(crate) loaded: Option<Arc<dyn Any + Send + Sync>>,
    pub(crate) installed: Option<Arc<dyn Any + Send + Sync>>,
    pub(crate) dependencies: Vec<AssetId>,
    pub(crate) held_dependencies: AssetDependencyLeases,
    pub(crate) loaded_entry_fingerprint: Option<String>,
    pub(crate) loaded_cooked_hash: Option<String>,
}

impl AssetReloadBackup {
    pub(crate) fn from_record(record: &AssetRecord) -> Self {
        Self {
            loaded: record.loaded.clone(),
            installed: record.installed.clone(),
            dependencies: record.dependencies.clone(),
            held_dependencies: record.held_dependencies.clone(),
            loaded_entry_fingerprint: record.loaded_entry_fingerprint.clone(),
            loaded_cooked_hash: record.loaded_cooked_hash.clone(),
        }
    }
}
