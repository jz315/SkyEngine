use std::time::Duration;

use super::{AssetError, AssetFailurePhase, AssetId};

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
    if slot.is_none_or(|current| age > current) {
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
