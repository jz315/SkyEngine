use std::any::{Any, TypeId};
use std::fmt::{Display, Formatter};
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Weak};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const ASSET_SYSTEM_VERSION: u32 = 1;
pub const DEFAULT_ASSET_IO_WORKER_THREADS: usize = 2;
pub const DEFAULT_ASSET_IO_QUEUE_CAPACITY: usize = 256;
pub const DEFAULT_ASSET_IO_PRIORITY: i32 = 0;
pub const DEFAULT_ASSET_IO_SHUTDOWN_TIMEOUT: Option<Duration> = None;
pub const DEFAULT_ASSET_INSTALL_TIME_BUDGET: Duration = Duration::from_millis(4);

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

pub(crate) trait AssetHandleProvider: Send + Sync {
    fn state_for_handle(&self, id: AssetId) -> AssetState;
    fn error_for_handle(&self, id: AssetId) -> Option<AssetError>;
    fn get_for_handle(
        &self,
        id: AssetId,
        expected_type: &'static str,
        expected_type_id: TypeId,
    ) -> Result<Arc<dyn Any + Send + Sync>, AssetError>;
}

pub(crate) struct AssetLease {
    id: AssetId,
    release_tx: Sender<AssetId>,
    provider: Option<Weak<dyn AssetHandleProvider>>,
}

impl AssetLease {
    pub(crate) fn new(
        id: AssetId,
        release_tx: Sender<AssetId>,
        provider: Weak<dyn AssetHandleProvider>,
    ) -> Self {
        Self {
            id,
            release_tx,
            provider: Some(provider),
        }
    }
}

impl Drop for AssetLease {
    fn drop(&mut self) {
        let _ = self.release_tx.send(self.id);
    }
}

/// Strong typed asset handle.
///
/// Cloning this handle keeps the same asset lease alive. Dropping the final
/// clone releases that lease back to the owning [`Assets`](crate::asset::Assets)
/// facade on the next asset update.
pub struct Handle<T: Asset> {
    id: AssetId,
    lease: Arc<AssetLease>,
    marker: PhantomData<fn() -> T>,
}

impl<T: Asset> Handle<T> {
    #[must_use]
    pub(crate) fn from_lease(id: AssetId, lease: Arc<AssetLease>) -> Self {
        Self {
            id,
            lease,
            marker: PhantomData,
        }
    }

    #[cfg(test)]
    #[allow(dead_code)]
    #[must_use]
    pub(crate) fn new(id: AssetId) -> Self {
        let (release_tx, _release_rx) = std::sync::mpsc::channel();
        Self {
            id,
            lease: Arc::new(AssetLease {
                id,
                release_tx,
                provider: None,
            }),
            marker: PhantomData,
        }
    }

    #[must_use]
    pub fn id(&self) -> AssetId {
        self.id
    }

    #[must_use]
    pub fn downgrade(&self) -> WeakHandle<T> {
        WeakHandle::new(self.id)
    }

    #[must_use]
    pub fn state(&self) -> AssetState {
        self.lease
            .provider
            .as_ref()
            .and_then(|provider| provider.upgrade())
            .map_or(AssetState::Unloaded, |provider| {
                provider.state_for_handle(self.id)
            })
    }

    #[must_use]
    pub fn status(&self) -> AssetStatus {
        self.state().into()
    }

    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.state() == AssetState::Installed
    }

    pub fn get(&self) -> Result<Arc<T>, AssetError> {
        let provider = self
            .lease
            .provider
            .as_ref()
            .and_then(|provider| provider.upgrade())
            .ok_or(AssetError::AssetNotInstalled {
                id: self.id,
                state: AssetState::Unloaded,
            })?;
        let installed = provider.get_for_handle(self.id, T::TYPE, TypeId::of::<T>())?;
        Arc::downcast::<T>(installed).map_err(|_| AssetError::AssetTypeMismatch {
            id: self.id,
            expected: T::TYPE,
            actual: "unknown".to_string(),
        })
    }

    #[must_use]
    pub fn try_get(&self) -> Option<Arc<T>> {
        self.get().ok()
    }

    #[must_use]
    pub fn error(&self) -> Option<AssetError> {
        self.lease
            .provider
            .as_ref()
            .and_then(|provider| provider.upgrade())
            .and_then(|provider| provider.error_for_handle(self.id))
    }
}

impl<T: Asset> Clone for Handle<T> {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            lease: self.lease.clone(),
            marker: PhantomData,
        }
    }
}

impl<T: Asset> std::fmt::Debug for Handle<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Handle").field("id", &self.id).finish()
    }
}

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

/// Typed asset source path.
///
/// This is an editor/serialized reference to an asset source key. It does not
/// keep runtime residency alive; resolve it through [`Assets`](crate::asset::Assets)
/// when a strong [`Handle<T>`] or weak [`WeakHandle<T>`] is needed.
pub struct AssetPath<T: Asset> {
    path: PathBuf,
    marker: PhantomData<fn() -> T>,
}

impl<T: Asset> AssetPath<T> {
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            marker: PhantomData,
        }
    }

    #[must_use]
    pub fn as_path(&self) -> &Path {
        &self.path
    }

    #[must_use]
    pub fn into_path_buf(self) -> PathBuf {
        self.path
    }
}

impl<T: Asset> Clone for AssetPath<T> {
    fn clone(&self) -> Self {
        Self::new(self.path.clone())
    }
}

impl<T: Asset> std::fmt::Debug for AssetPath<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("AssetPath").field(&self.path).finish()
    }
}

impl<T: Asset> Display for AssetPath<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.path.display())
    }
}

impl<T: Asset> PartialEq for AssetPath<T> {
    fn eq(&self, other: &Self) -> bool {
        self.path == other.path
    }
}

impl<T: Asset> Eq for AssetPath<T> {}

impl<T: Asset> std::hash::Hash for AssetPath<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.path.hash(state);
    }
}

impl<T: Asset> AsRef<Path> for AssetPath<T> {
    fn as_ref(&self) -> &Path {
        self.as_path()
    }
}

impl<T: Asset> From<PathBuf> for AssetPath<T> {
    fn from(path: PathBuf) -> Self {
        Self::new(path)
    }
}

impl<T: Asset> From<&Path> for AssetPath<T> {
    fn from(path: &Path) -> Self {
        Self::new(path)
    }
}

impl<T: Asset> From<&str> for AssetPath<T> {
    fn from(path: &str) -> Self {
        Self::new(path)
    }
}

impl<T: Asset> Serialize for AssetPath<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.path.serialize(serializer)
    }
}

impl<'de, T: Asset> Deserialize<'de> for AssetPath<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        PathBuf::deserialize(deserializer).map(Self::new)
    }
}

/// Weak typed asset identity.
///
/// This does not keep the asset loaded. Use it for serialized data, editor
/// references, and places that need identity without residency.
#[derive(Debug)]
pub struct WeakHandle<T: Asset> {
    id: AssetId,
    marker: PhantomData<fn() -> T>,
}

impl<T: Asset> WeakHandle<T> {
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

impl<T: Asset> Clone for WeakHandle<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: Asset> Copy for WeakHandle<T> {}

impl<T: Asset> PartialEq for WeakHandle<T> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<T: Asset> Eq for WeakHandle<T> {}

impl<T: Asset> std::hash::Hash for WeakHandle<T> {
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssetStatus {
    NotRequested,
    Loading,
    Ready,
    Failed,
}

impl From<AssetState> for AssetStatus {
    fn from(value: AssetState) -> Self {
        match value {
            AssetState::Unloaded => Self::NotRequested,
            AssetState::Failed => Self::Failed,
            AssetState::Installed => Self::Ready,
            AssetState::Loading
            | AssetState::Loaded
            | AssetState::WaitingDependencies
            | AssetState::Installing
            | AssetState::Uninstalling
            | AssetState::Unloading => Self::Loading,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AssetStateCounts {
    pub unloaded: usize,
    pub loading: usize,
    pub loaded: usize,
    pub waiting_dependencies: usize,
    pub installing: usize,
    pub installed: usize,
    pub uninstalling: usize,
    pub unloading: usize,
    pub failed: usize,
}

impl AssetStateCounts {
    pub(crate) fn record(&mut self, state: AssetState) {
        match state {
            AssetState::Unloaded => self.unloaded += 1,
            AssetState::Loading => self.loading += 1,
            AssetState::Loaded => self.loaded += 1,
            AssetState::WaitingDependencies => self.waiting_dependencies += 1,
            AssetState::Installing => self.installing += 1,
            AssetState::Installed => self.installed += 1,
            AssetState::Uninstalling => self.uninstalling += 1,
            AssetState::Unloading => self.unloading += 1,
            AssetState::Failed => self.failed += 1,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AssetRequestPhaseTimingStats {
    pub samples: usize,
    pub average: Option<Duration>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AssetRequestTimingStats {
    pub completed_requests: usize,
    pub average_queue_wait: Option<Duration>,
    pub average_canceled_queue_wait: Option<Duration>,
    pub average_active_time: Option<Duration>,
    pub average_total_time: Option<Duration>,
    pub loading: AssetRequestPhaseTimingStats,
    pub decoding: AssetRequestPhaseTimingStats,
    pub waiting_dependencies: AssetRequestPhaseTimingStats,
    pub ready_to_install: AssetRequestPhaseTimingStats,
    pub installing: AssetRequestPhaseTimingStats,
    pub unloading: AssetRequestPhaseTimingStats,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AssetLoadTimingStats {
    pub completed_source_loads: usize,
    pub failed_source_loads: usize,
    pub average_read_time: Option<Duration>,
    pub average_decode_time: Option<Duration>,
    pub average_total_time: Option<Duration>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AssetSourceLoadPhaseCounts {
    pub queued: usize,
    pub reading: usize,
    pub decoding: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AssetProviderStats {
    pub package_roots: usize,
    pub package_files: usize,
    pub cached_bundle_indexes: usize,
    pub cached_bundle_index_entries: usize,
    pub resolved_raw_sources: usize,
    pub resolved_cooked_sources: usize,
    pub resolved_package_sources: usize,
    pub resolved_bundle_sources: usize,
    pub resolve_errors: usize,
    pub cache_invalidations: usize,
    pub full_cache_invalidations: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AssetActiveStateAgeStats {
    pub loading: Option<Duration>,
    pub loaded: Option<Duration>,
    pub waiting_dependencies: Option<Duration>,
    pub installing: Option<Duration>,
    pub uninstalling: Option<Duration>,
    pub unloading: Option<Duration>,
}

impl AssetActiveStateAgeStats {
    pub(crate) fn record(&mut self, state: AssetState, age: Duration) {
        match state {
            AssetState::Loading => record_oldest(&mut self.loading, age),
            AssetState::Loaded => record_oldest(&mut self.loaded, age),
            AssetState::WaitingDependencies => record_oldest(&mut self.waiting_dependencies, age),
            AssetState::Installing => record_oldest(&mut self.installing, age),
            AssetState::Uninstalling => record_oldest(&mut self.uninstalling, age),
            AssetState::Unloading => record_oldest(&mut self.unloading, age),
            AssetState::Unloaded | AssetState::Installed | AssetState::Failed => {}
        }
    }
}

fn record_oldest(slot: &mut Option<Duration>, age: Duration) {
    if slot.map_or(true, |current| age > current) {
        *slot = Some(age);
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AssetStats {
    pub records: usize,
    pub queued_requests: usize,
    pub active_requests: usize,
    pub submitted_requests: usize,
    pub activated_requests: usize,
    pub canceled_requests: usize,
    pub failed_requests: usize,
    pub oldest_queued_request_age: Option<Duration>,
    pub oldest_active_request_age: Option<Duration>,
    pub request_timings: AssetRequestTimingStats,
    pub load_timings: AssetLoadTimingStats,
    pub inflight_loads: usize,
    pub load_worker_threads: usize,
    pub running_load_jobs: usize,
    pub queued_load_jobs: usize,
    pub load_queue_capacity: usize,
    pub oldest_queued_load_job_age: Option<Duration>,
    pub source_load_phases: AssetSourceLoadPhaseCounts,
    pub deferred_load_submissions: usize,
    pub provider: AssetProviderStats,
    pub retained_events: usize,
    pub strong_references: usize,
    pub dependency_references: usize,
    pub states: AssetStateCounts,
    pub active_state_ages: AssetActiveStateAgeStats,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssetRequestStatus {
    Queued,
    Loading,
    Decoding,
    WaitingDependencies,
    ReadyToInstall,
    Installing,
    Installed,
    Unloading,
    Unloaded,
    Failed,
    Canceled,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetRequestProgress {
    pub completed_steps: u8,
    pub total_steps: u8,
    pub label: String,
}

impl AssetRequestProgress {
    #[must_use]
    pub fn new(completed_steps: u8, total_steps: u8, label: impl Into<String>) -> Self {
        Self {
            completed_steps,
            total_steps,
            label: label.into(),
        }
    }

    #[must_use]
    pub fn percent(&self) -> u8 {
        if self.total_steps == 0 {
            return 0;
        }
        let percent =
            u16::from(self.completed_steps).saturating_mul(100) / u16::from(self.total_steps);
        u8::try_from(percent.min(100)).unwrap_or(100)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetRequestSnapshot {
    pub request_id: u64,
    pub asset_id: AssetId,
    pub generation: u64,
    pub priority: i32,
    pub status: AssetRequestStatus,
    pub failure_phase: Option<AssetFailurePhase>,
    pub progress: AssetRequestProgress,
    pub queued_age: Duration,
    pub active_age: Option<Duration>,
    pub phase_age: Duration,
    pub dependency_blockers: Vec<AssetId>,
    pub dependency_blocker_details: Vec<AssetDependencyBlocker>,
    pub dependency_cycle: Vec<AssetId>,
    pub last_error: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssetDependencyBlockerReason {
    Missing,
    Failed,
    Waiting,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetDependencyBlocker {
    pub asset_id: AssetId,
    pub reason: AssetDependencyBlockerReason,
    pub state: Option<AssetState>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetFailureSnapshot {
    pub asset_id: AssetId,
    pub asset_type: String,
    pub state: AssetState,
    pub generation: u64,
    pub phase: Option<AssetFailurePhase>,
    pub error: AssetError,
    pub reload_pending: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AssetDiagnosticsSnapshot {
    pub stats: AssetStats,
    pub queued_requests: Vec<AssetRequestSnapshot>,
    pub active_requests: Vec<AssetRequestSnapshot>,
    pub canceled_requests: Vec<AssetRequestSnapshot>,
    pub failed_requests: Vec<AssetRequestSnapshot>,
    pub failures: Vec<AssetFailureSnapshot>,
    pub reload_status: AssetReloadStatus,
    pub last_reload_report: AssetReloadReport,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AssetReloadReport {
    pub changed_roots: Vec<AssetId>,
    pub impacted: Vec<AssetId>,
    pub skipped: Vec<AssetReloadSkipped>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetReloadSkipped {
    pub asset_id: AssetId,
    pub reason: AssetReloadSkipReason,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssetReloadSkipReason {
    UntrackedRecord,
    MissingManifestEntry,
    Unchanged,
}

impl AssetReloadReport {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.changed_roots.is_empty() && self.impacted.is_empty() && self.skipped.is_empty()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AssetReloadStatus {
    pub auto_reload_enabled: bool,
    pub file_watcher_enabled: bool,
    pub auto_reload_frozen: bool,
    pub pending_roots: Vec<AssetId>,
    pub pending_age: Option<Duration>,
    pub last_report: AssetReloadReport,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssetEventKind {
    ReloadQueued,
    Loaded,
    Installed,
    Reloaded,
    Unloaded,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetEvent {
    pub sequence: u64,
    pub id: AssetId,
    pub kind: AssetEventKind,
    pub state: AssetState,
    pub generation: u64,
    pub asset_type: String,
    pub failure_phase: Option<AssetFailurePhase>,
    pub manifest_fingerprint: Option<String>,
    pub content_hash: Option<String>,
    pub dependencies: Vec<AssetId>,
    pub reload_pending: bool,
}

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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AssetEventCursor {
    next_sequence: u64,
}

impl AssetEventCursor {
    #[must_use]
    pub(crate) fn new(next_sequence: u64) -> Self {
        Self { next_sequence }
    }

    #[must_use]
    pub(crate) fn next_sequence(self) -> u64 {
        self.next_sequence
    }

    pub(crate) fn set_next_sequence(&mut self, next_sequence: u64) {
        self.next_sequence = next_sequence;
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

#[derive(Clone, Debug)]
pub struct AssetConfig {
    pub asset_root: PathBuf,
    pub target: String,
    pub profile: String,
    pub background_loading: bool,
    pub install_budget_per_update: Option<usize>,
    pub install_time_budget: Option<Duration>,
    pub auto_reload: bool,
    pub auto_reload_interval: Duration,
    pub auto_reload_debounce: Duration,
    pub file_watcher: bool,
    pub package_roots: Vec<PathBuf>,
    pub package_files: Vec<PathBuf>,
    pub io_worker_threads: usize,
    pub io_queue_capacity: usize,
    pub io_default_priority: i32,
    pub io_shutdown_timeout: Option<Duration>,
}

impl AssetConfig {
    #[must_use]
    pub fn new(asset_root: impl Into<PathBuf>, target: impl Into<String>) -> Self {
        Self {
            asset_root: asset_root.into(),
            target: target.into(),
            profile: "default".to_string(),
            background_loading: false,
            install_budget_per_update: None,
            install_time_budget: None,
            auto_reload: false,
            auto_reload_interval: Duration::from_millis(250),
            auto_reload_debounce: Duration::ZERO,
            file_watcher: false,
            package_roots: Vec::new(),
            package_files: Vec::new(),
            io_worker_threads: DEFAULT_ASSET_IO_WORKER_THREADS,
            io_queue_capacity: DEFAULT_ASSET_IO_QUEUE_CAPACITY,
            io_default_priority: DEFAULT_ASSET_IO_PRIORITY,
            io_shutdown_timeout: DEFAULT_ASSET_IO_SHUTDOWN_TIMEOUT,
        }
    }

    #[must_use]
    pub fn with_background_loading(mut self, enabled: bool) -> Self {
        self.background_loading = enabled;
        self
    }

    #[must_use]
    pub fn with_profile(mut self, profile: impl Into<String>) -> Self {
        self.profile = profile.into();
        self
    }

    #[must_use]
    pub fn with_install_budget_per_update(mut self, budget: usize) -> Self {
        self.install_budget_per_update = Some(budget);
        self
    }

    #[must_use]
    pub fn with_install_time_budget(mut self, budget: Duration) -> Self {
        self.install_time_budget = Some(budget);
        self
    }

    #[must_use]
    pub fn without_install_time_budget(mut self) -> Self {
        self.install_time_budget = None;
        self
    }

    #[must_use]
    pub fn with_auto_reload(mut self, enabled: bool) -> Self {
        self.auto_reload = enabled;
        self
    }

    #[must_use]
    pub fn with_auto_reload_interval(mut self, interval: Duration) -> Self {
        self.auto_reload_interval = interval;
        self
    }

    #[must_use]
    pub fn with_auto_reload_debounce(mut self, debounce: Duration) -> Self {
        self.auto_reload_debounce = debounce;
        self
    }

    #[must_use]
    pub fn with_file_watcher(mut self, enabled: bool) -> Self {
        self.file_watcher = enabled;
        self
    }

    #[must_use]
    pub fn with_package_root(mut self, root: impl Into<PathBuf>) -> Self {
        self.package_roots.push(root.into());
        self
    }

    #[must_use]
    pub fn with_package_roots<I, P>(mut self, roots: I) -> Self
    where
        I: IntoIterator<Item = P>,
        P: Into<PathBuf>,
    {
        self.package_roots.extend(roots.into_iter().map(Into::into));
        self
    }

    #[must_use]
    pub fn without_package_roots(mut self) -> Self {
        self.package_roots.clear();
        self
    }

    #[must_use]
    pub fn with_package_file(mut self, file: impl Into<PathBuf>) -> Self {
        self.package_files.push(file.into());
        self
    }

    #[must_use]
    pub fn with_package_files<I, P>(mut self, files: I) -> Self
    where
        I: IntoIterator<Item = P>,
        P: Into<PathBuf>,
    {
        self.package_files.extend(files.into_iter().map(Into::into));
        self
    }

    #[must_use]
    pub fn without_package_files(mut self) -> Self {
        self.package_files.clear();
        self
    }

    #[must_use]
    pub fn with_io_worker_threads(mut self, worker_threads: usize) -> Self {
        self.io_worker_threads = worker_threads.max(1);
        self
    }

    #[must_use]
    pub fn with_io_queue_capacity(mut self, queue_capacity: usize) -> Self {
        self.io_queue_capacity = queue_capacity.max(1);
        self
    }

    #[must_use]
    pub fn with_io_default_priority(mut self, priority: i32) -> Self {
        self.io_default_priority = priority;
        self
    }

    #[must_use]
    pub fn with_io_shutdown_timeout(mut self, timeout: Duration) -> Self {
        self.io_shutdown_timeout = Some(timeout);
        self
    }

    #[must_use]
    pub fn without_io_shutdown_timeout(mut self) -> Self {
        self.io_shutdown_timeout = None;
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
    pub fn package_roots(&self) -> Vec<PathBuf> {
        self.package_roots
            .iter()
            .map(|root| {
                if root.is_absolute() {
                    root.clone()
                } else {
                    self.asset_root.join(root)
                }
            })
            .collect()
    }

    #[must_use]
    pub fn package_files(&self) -> Vec<PathBuf> {
        self.package_files
            .iter()
            .map(|file| {
                if file.is_absolute() {
                    file.clone()
                } else {
                    self.asset_root.join(file)
                }
            })
            .collect()
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
