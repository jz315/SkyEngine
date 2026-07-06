use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::{AssetConfig, AssetId, ASSET_SYSTEM_VERSION};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AssetMeta {
    pub asset_id: AssetId,
    pub asset_type: String,
    pub importer: String,
    pub cooker: String,
    pub version: u32,
    pub source_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cooked_hash: Option<String>,
    #[serde(default)]
    pub dependencies: Vec<AssetId>,
    #[serde(default)]
    pub import_settings: serde_json::Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AssetManifestEntry {
    pub asset_id: AssetId,
    pub asset_type: String,
    pub importer: String,
    pub cooker: String,
    pub version: u32,
    pub source_path: String,
    pub cooked_path: String,
    #[serde(default)]
    pub dependencies: Vec<AssetId>,
    #[serde(default)]
    pub import_settings: serde_json::Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AssetMetadata {
    pub asset_id: AssetId,
    pub asset_type: String,
    pub importer: String,
    pub cooker: String,
    pub version: u32,
    pub source_path: String,
    pub cooked_path: String,
    pub dependencies: Vec<AssetId>,
    pub import_settings: serde_json::Value,
}

impl From<&AssetManifestEntry> for AssetMetadata {
    fn from(entry: &AssetManifestEntry) -> Self {
        Self {
            asset_id: entry.asset_id,
            asset_type: entry.asset_type.clone(),
            importer: entry.importer.clone(),
            cooker: entry.cooker.clone(),
            version: entry.version,
            source_path: entry.source_path.clone(),
            cooked_path: entry.cooked_path.clone(),
            dependencies: entry.dependencies.clone(),
            import_settings: entry.import_settings.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetWatchPaths {
    pub source_path: PathBuf,
    pub cooked_path: PathBuf,
    pub package_paths: Vec<PathBuf>,
    pub package_files: Vec<PathBuf>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AssetRegistryManifest {
    pub version: u32,
    pub target: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provenance: Vec<AssetManifestProvenance>,
    pub assets: Vec<AssetManifestEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetManifestProvenance {
    pub asset_id: AssetId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cooked_hash: Option<String>,
    pub dependency_hash: String,
    pub platform: String,
    pub profile: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AssetCookedSchema {
    pub cooker: &'static str,
    pub version: u32,
    pub dependency_schema: Option<&'static str>,
}

impl AssetCookedSchema {
    #[must_use]
    pub const fn new(cooker: &'static str, version: u32) -> Self {
        Self {
            cooker,
            version,
            dependency_schema: None,
        }
    }

    #[must_use]
    pub const fn with_dependency_schema(mut self, dependency_schema: &'static str) -> Self {
        self.dependency_schema = Some(dependency_schema);
        self
    }
}

impl Default for AssetRegistryManifest {
    fn default() -> Self {
        Self {
            version: ASSET_SYSTEM_VERSION,
            target: AssetConfig::default_target(),
            provenance: Vec::new(),
            assets: Vec::new(),
        }
    }
}
