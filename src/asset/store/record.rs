use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::AssetRecord;
use crate::asset::types::{
    AssetError, AssetFailurePhase, AssetId, AssetRequestProgress, AssetState,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AssetReloadScanRecord {
    Tracked {
        id: AssetId,
        loaded_entry_fingerprint: String,
        loaded_cooked_hash: String,
    },
    Untracked {
        id: AssetId,
    },
}

impl AssetReloadScanRecord {
    pub(crate) fn id(&self) -> AssetId {
        match self {
            Self::Tracked { id, .. } | Self::Untracked { id } => *id,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AssetRecordDiagnostic {
    pub(crate) asset_id: AssetId,
    pub(crate) asset_type: String,
    pub(crate) state: AssetState,
    pub(crate) load_generation: u64,
    pub(crate) strong_ref_count: usize,
    pub(crate) dependency_ref_count: usize,
    pub(crate) dependencies: Vec<AssetId>,
    pub(crate) error: Option<AssetError>,
    pub(crate) failure_phase: Option<AssetFailurePhase>,
    pub(crate) reload_pending: bool,
    pub(crate) install_progress: Option<AssetRequestProgress>,
    pub(crate) state_age: Duration,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct AssetEventRecordContext {
    pub(crate) generation: u64,
    pub(crate) asset_type: String,
    pub(crate) manifest_fingerprint: Option<String>,
    pub(crate) content_hash: Option<String>,
    pub(crate) dependencies: Vec<AssetId>,
    pub(crate) reload_pending: bool,
    pub(crate) failure_phase: Option<AssetFailurePhase>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AssetRecordLoadActivation {
    pub(crate) state: AssetState,
    pub(crate) load_generation: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AssetRecordInstallCompletion {
    pub(crate) dependencies: Vec<AssetId>,
    pub(crate) reloaded: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AssetRawSourceRecord {
    pub(crate) asset_type: String,
    pub(crate) path: PathBuf,
}

impl AssetRecordDiagnostic {
    pub(super) fn from_record(asset_id: AssetId, record: &AssetRecord, now: Instant) -> Self {
        Self {
            asset_id,
            asset_type: record.asset_type.clone(),
            state: record.state,
            load_generation: record.load_generation,
            strong_ref_count: record.strong_ref_count,
            dependency_ref_count: record.dependency_ref_count,
            dependencies: record.dependencies.clone(),
            error: record.error.clone(),
            failure_phase: record.failure_phase,
            reload_pending: record.reload_pending,
            install_progress: record.install_progress(),
            state_age: record.state_age(now),
        }
    }
}

impl AssetEventRecordContext {
    pub(super) fn from_record(record: &AssetRecord) -> Self {
        Self {
            generation: record.load_generation,
            asset_type: record.asset_type.clone(),
            manifest_fingerprint: record.loaded_entry_fingerprint.clone(),
            content_hash: record.loaded_cooked_hash.clone(),
            dependencies: record.dependencies.clone(),
            reload_pending: record.reload_pending,
            failure_phase: record.failure_phase,
        }
    }
}
