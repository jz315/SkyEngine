use std::fmt::{Display, Formatter};
use std::marker::PhantomData;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const ASSET_SYSTEM_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AssetId(Uuid);

impl AssetId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn parse_str(value: &str) -> Result<Self, AssetError> {
        let uuid = Uuid::parse_str(value).map_err(|error| AssetError::InvalidAssetId {
            value: value.to_string(),
            message: error.to_string(),
        })?;
        Ok(Self(uuid))
    }

    #[must_use]
    pub fn as_uuid(self) -> Uuid {
        self.0
    }
}

impl Default for AssetId {
    fn default() -> Self {
        Self::new()
    }
}

impl Display for AssetId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

pub trait Asset: Send + Sync + 'static {
    const TYPE: &'static str;
}

#[derive(Debug)]
pub struct Handle<T: Asset> {
    id: AssetId,
    marker: PhantomData<fn() -> T>,
}

impl<T: Asset> Handle<T> {
    #[must_use]
    pub fn new(id: AssetId) -> Self {
        Self {
            id,
            marker: PhantomData,
        }
    }

    #[must_use]
    pub fn id(self) -> AssetId {
        self.id
    }
}

impl<T: Asset> Clone for Handle<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: Asset> Copy for Handle<T> {}

impl<T: Asset> PartialEq for Handle<T> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<T: Asset> Eq for Handle<T> {}

impl<T: Asset> std::hash::Hash for Handle<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssetState {
    Unloaded,
    Loading,
    Loaded,
    WaitingDependencies,
    Installing,
    Installed,
    Uninstalling,
    Unloading,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AssetError {
    InvalidAssetId {
        value: String,
        message: String,
    },
    InvalidConfig {
        message: String,
    },
    ManifestMissing {
        path: PathBuf,
    },
    AssetNotFound {
        id: AssetId,
    },
    AssetPathNotFound {
        path: PathBuf,
    },
    AssetTypeMismatch {
        id: AssetId,
        expected: &'static str,
        actual: String,
    },
    FactoryNotRegistered {
        asset_type: String,
    },
    AssetNotInstalled {
        id: AssetId,
        state: AssetState,
    },
    InvalidState {
        id: AssetId,
        state: AssetState,
        message: String,
    },
    MissingCookedArtifact {
        id: AssetId,
        path: PathBuf,
    },
    MissingDependency {
        id: AssetId,
        dependency: AssetId,
    },
    DependencyFailed {
        id: AssetId,
        dependency: AssetId,
    },
    DependencyCycle {
        cycle: Vec<AssetId>,
    },
    VersionMismatch {
        path: PathBuf,
        expected: u32,
        actual: u32,
    },
    Io {
        path: PathBuf,
        message: String,
    },
    Json {
        path: PathBuf,
        message: String,
    },
    InvalidCookedAsset {
        id: Option<AssetId>,
        message: String,
    },
    VerificationFailed {
        issues: Vec<String>,
    },
    Unsupported {
        message: String,
    },
    Internal {
        message: String,
    },
}

impl Display for AssetError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidAssetId { value, message } => {
                write!(f, "Invalid asset id `{value}`: {message}")
            }
            Self::InvalidConfig { message } => write!(f, "Invalid asset config: {message}"),
            Self::ManifestMissing { path } => write!(f, "Asset manifest missing at {:?}", path),
            Self::AssetNotFound { id } => write!(f, "Asset `{id}` not found in manifest"),
            Self::AssetPathNotFound { path } => {
                write!(f, "Asset path {:?} not found in manifest", path)
            }
            Self::AssetTypeMismatch {
                id,
                expected,
                actual,
            } => write!(
                f,
                "Asset `{id}` type mismatch: expected `{expected}`, got `{actual}`"
            ),
            Self::FactoryNotRegistered { asset_type } => {
                write!(f, "No runtime factory registered for `{asset_type}`")
            }
            Self::AssetNotInstalled { id, state } => {
                write!(f, "Asset `{id}` is not installed (state: {state:?})")
            }
            Self::InvalidState { id, state, message } => {
                write!(f, "Invalid state for asset `{id}` ({state:?}): {message}")
            }
            Self::MissingCookedArtifact { id, path } => {
                write!(f, "Cooked artifact for asset `{id}` missing at {:?}", path)
            }
            Self::MissingDependency { id, dependency } => {
                write!(f, "Asset `{id}` is missing dependency `{dependency}`")
            }
            Self::DependencyFailed { id, dependency } => {
                write!(f, "Asset `{id}` dependency `{dependency}` failed")
            }
            Self::DependencyCycle { cycle } => {
                let path = cycle
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(" -> ");
                write!(f, "Asset dependency cycle detected: {path}")
            }
            Self::VersionMismatch {
                path,
                expected,
                actual,
            } => write!(
                f,
                "Version mismatch in {:?}: expected {expected}, got {actual}",
                path
            ),
            Self::Io { path, message } => write!(f, "I/O error at {:?}: {message}", path),
            Self::Json { path, message } => write!(f, "JSON error at {:?}: {message}", path),
            Self::InvalidCookedAsset { id, message } => match id {
                Some(id) => write!(f, "Invalid cooked asset `{id}`: {message}"),
                None => write!(f, "Invalid cooked asset: {message}"),
            },
            Self::VerificationFailed { issues } => {
                write!(f, "Asset verification failed: {}", issues.join("; "))
            }
            Self::Unsupported { message } => write!(f, "Unsupported asset operation: {message}"),
            Self::Internal { message } => write!(f, "Internal asset error: {message}"),
        }
    }
}

impl std::error::Error for AssetError {}

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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AssetRegistryManifest {
    pub version: u32,
    pub target: String,
    pub assets: Vec<AssetManifestEntry>,
}

impl Default for AssetRegistryManifest {
    fn default() -> Self {
        Self {
            version: ASSET_SYSTEM_VERSION,
            target: AssetConfig::default_target(),
            assets: Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct AssetConfig {
    pub asset_root: PathBuf,
    pub target: String,
    pub background_loading: bool,
    pub install_budget_per_update: Option<usize>,
}

impl AssetConfig {
    #[must_use]
    pub fn new(asset_root: impl Into<PathBuf>, target: impl Into<String>) -> Self {
        Self {
            asset_root: asset_root.into(),
            target: target.into(),
            background_loading: false,
            install_budget_per_update: None,
        }
    }

    #[must_use]
    pub fn with_background_loading(mut self, enabled: bool) -> Self {
        self.background_loading = enabled;
        self
    }

    #[must_use]
    pub fn with_install_budget_per_update(mut self, budget: usize) -> Self {
        self.install_budget_per_update = Some(budget);
        self
    }

    #[must_use]
    pub fn default_target() -> String {
        if cfg!(target_arch = "wasm32") {
            "web".to_string()
        } else {
            "native".to_string()
        }
    }

    #[must_use]
    pub fn cooked_root(&self) -> PathBuf {
        self.asset_root
            .join(".sky")
            .join("cooked")
            .join(&self.target)
    }

    #[must_use]
    pub fn manifest_path(&self) -> PathBuf {
        self.cooked_root().join("manifest.json")
    }

    #[must_use]
    pub fn source_key(&self, path: &Path) -> String {
        let relative = if path.is_absolute() {
            path.strip_prefix(&self.asset_root).unwrap_or(path)
        } else {
            path
        };
        normalize_source_key(&relative.to_string_lossy())
    }
}

impl Default for AssetConfig {
    fn default() -> Self {
        Self::new("assets", Self::default_target())
    }
}

#[must_use]
pub fn normalize_source_key(value: &str) -> String {
    let normalized = value.replace('\\', "/");
    if cfg!(windows) {
        normalized.to_ascii_lowercase()
    } else {
        normalized
    }
}

pub struct AssetLoadContext<'a> {
    pub asset_id: AssetId,
    pub entry: &'a AssetManifestEntry,
    pub bytes: &'a [u8],
    pub asset_root: &'a Path,
    pub cooked_root: &'a Path,
}

pub struct AssetInstallContext<'a> {
    pub asset_id: AssetId,
    pub entry: &'a AssetManifestEntry,
}

pub struct LoadedAsset<T> {
    pub loaded: T,
    pub dependencies: Vec<AssetId>,
}

impl<T> LoadedAsset<T> {
    #[must_use]
    pub fn new(loaded: T) -> Self {
        Self {
            loaded,
            dependencies: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_dependencies(mut self, dependencies: impl Into<Vec<AssetId>>) -> Self {
        self.dependencies = dependencies.into();
        self
    }
}
