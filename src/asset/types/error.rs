use std::fmt::{Display, Formatter};
use std::path::PathBuf;

use super::{AssetId, AssetState};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssetFailurePhase {
    Lookup,
    Read,
    Decode,
    Dependency,
    Install,
    Uninstall,
    Runtime,
    Verification,
}

impl AssetFailurePhase {
    #[must_use]
    pub fn from_error(error: &AssetError) -> Self {
        match error {
            AssetError::ManifestMissing { .. }
            | AssetError::AssetNotFound { .. }
            | AssetError::AssetPathNotFound { .. }
            | AssetError::AssetTypeMismatch { .. }
            | AssetError::FactoryNotRegistered { .. } => Self::Lookup,
            AssetError::MissingCookedArtifact { .. } | AssetError::Io { .. } => Self::Read,
            AssetError::Json { .. }
            | AssetError::InvalidCookedAsset { .. }
            | AssetError::CookedSchemaMismatch { .. }
            | AssetError::VersionMismatch { .. } => Self::Decode,
            AssetError::MissingDependency { .. }
            | AssetError::DependencyFailed { .. }
            | AssetError::DependencyCycle { .. } => Self::Dependency,
            AssetError::VerificationFailed { .. } => Self::Verification,
            AssetError::InvalidAssetId { .. }
            | AssetError::InvalidConfig { .. }
            | AssetError::AssetNotInstalled { .. }
            | AssetError::InvalidState { .. }
            | AssetError::Unsupported { .. }
            | AssetError::Internal { .. } => Self::Runtime,
        }
    }
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
    CookedSchemaMismatch {
        id: AssetId,
        expected_cooker: String,
        expected_version: u32,
        actual_cooker: String,
        actual_version: u32,
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
            Self::CookedSchemaMismatch {
                id,
                expected_cooker,
                expected_version,
                actual_cooker,
                actual_version,
            } => write!(
                f,
                "Cooked schema mismatch for asset `{id}`: manifest uses `{actual_cooker}` v{actual_version}, runtime factory expects `{expected_cooker}` v{expected_version}"
            ),
            Self::VerificationFailed { issues } => {
                write!(f, "Asset verification failed: {}", issues.join("; "))
            }
            Self::Unsupported { message } => write!(f, "Unsupported asset operation: {message}"),
            Self::Internal { message } => write!(f, "Internal asset error: {message}"),
        }
    }
}

impl std::error::Error for AssetError {}
