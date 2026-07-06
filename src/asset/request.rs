use std::any::TypeId;
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

use super::load::{AssetLoadQueue, AssetSourceLoadPhase};
use super::store::AssetStore;
use super::types::{
    AssetError, AssetFailurePhase, AssetId, AssetRequestPhaseTimingStats, AssetRequestProgress,
    AssetRequestSnapshot, AssetRequestStatus, AssetRequestTimingStats, AssetState,
};

const REQUEST_SNAPSHOT_LIMIT: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct AssetRequestId(u64);

impl AssetRequestId {
    pub(crate) fn new(value: u64) -> Self {
        Self(value)
    }

    pub(crate) fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AssetRequestPhase {
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

impl AssetRequestPhase {
    pub(crate) fn from_state(state: AssetState) -> Self {
        match state {
            AssetState::Unloaded => Self::Unloaded,
            AssetState::Loading => Self::Loading,
            AssetState::Loaded => Self::ReadyToInstall,
            AssetState::WaitingDependencies => Self::WaitingDependencies,
            AssetState::Installing => Self::Installing,
            AssetState::Installed => Self::Installed,
            AssetState::Uninstalling | AssetState::Unloading => Self::Unloading,
            AssetState::Failed => Self::Failed,
        }
    }

    pub(crate) fn status(self) -> AssetRequestStatus {
        match self {
            Self::Queued => AssetRequestStatus::Queued,
            Self::Loading => AssetRequestStatus::Loading,
            Self::Decoding => AssetRequestStatus::Decoding,
            Self::WaitingDependencies => AssetRequestStatus::WaitingDependencies,
            Self::ReadyToInstall => AssetRequestStatus::ReadyToInstall,
            Self::Installing => AssetRequestStatus::Installing,
            Self::Installed => AssetRequestStatus::Installed,
            Self::Unloading => AssetRequestStatus::Unloading,
            Self::Unloaded => AssetRequestStatus::Unloaded,
            Self::Failed => AssetRequestStatus::Failed,
            Self::Canceled => AssetRequestStatus::Canceled,
        }
    }

    pub(crate) fn progress(self) -> AssetRequestProgress {
        match self {
            Self::Queued => AssetRequestProgress::new(0, 6, "queued"),
            Self::Loading => AssetRequestProgress::new(1, 6, "loading source"),
            Self::Decoding => AssetRequestProgress::new(2, 6, "decoding source"),
            Self::WaitingDependencies => AssetRequestProgress::new(3, 6, "waiting dependencies"),
            Self::ReadyToInstall => AssetRequestProgress::new(4, 6, "ready to install"),
            Self::Installing => AssetRequestProgress::new(5, 6, "installing"),
            Self::Installed => AssetRequestProgress::new(6, 6, "installed"),
            Self::Unloading => AssetRequestProgress::new(5, 6, "unloading"),
            Self::Unloaded => AssetRequestProgress::new(6, 6, "unloaded"),
            Self::Failed => AssetRequestProgress::new(0, 6, "failed"),
            Self::Canceled => AssetRequestProgress::new(0, 6, "canceled"),
        }
    }

    pub(crate) fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Installed | Self::Unloaded | Self::Failed | Self::Canceled
        )
    }
}

#[derive(Clone, Debug)]
pub(crate) struct AssetRequest {
    pub(crate) request_id: AssetRequestId,
    pub(crate) id: AssetId,
    pub(crate) generation: u64,
    pub(crate) requested_type: Option<TypeId>,
    pub(crate) priority: i32,
    pub(crate) queued_at: Instant,
    pub(crate) started_at: Option<Instant>,
    pub(crate) completed_at: Option<Instant>,
    pub(crate) phase: AssetRequestPhase,
    pub(crate) phase_started_at: Instant,
    failed_counted: bool,
}

impl AssetRequest {
    pub(crate) fn new(
        request_id: AssetRequestId,
        id: AssetId,
        generation: u64,
        requested_type: Option<TypeId>,
        priority: i32,
        queued_at: Instant,
    ) -> Self {
        Self {
            request_id,
            id,
            generation,
            requested_type,
            priority,
            queued_at,
            started_at: None,
            completed_at: None,
            phase: AssetRequestPhase::Queued,
            phase_started_at: queued_at,
            failed_counted: false,
        }
    }

    pub(crate) fn queued_duration(&self, now: Instant) -> Duration {
        now.saturating_duration_since(self.queued_at)
    }

    pub(crate) fn active_duration(&self, now: Instant) -> Option<Duration> {
        self.started_at
            .map(|started_at| now.saturating_duration_since(started_at))
    }

    pub(crate) fn phase_duration(&self, now: Instant) -> Duration {
        now.saturating_duration_since(self.phase_started_at)
    }

    pub(crate) fn snapshot(&self, now: Instant) -> AssetRequestSnapshot {
        AssetRequestSnapshot {
            request_id: self.request_id.get(),
            asset_id: self.id,
            generation: self.generation,
            priority: self.priority,
            status: self.phase.status(),
            failure_phase: None,
            progress: self.phase.progress(),
            queued_age: self.queued_duration(now),
            active_age: self.active_duration(now),
            phase_age: self.phase_duration(now),
            dependency_blockers: Vec::new(),
            dependency_blocker_details: Vec::new(),
            dependency_cycle: Vec::new(),
            last_error: None,
        }
    }

    pub(crate) fn activate(
        &mut self,
        phase: AssetRequestPhase,
        generation: u64,
        started_at: Instant,
    ) {
        self.phase = phase;
        self.generation = generation;
        self.started_at = Some(started_at);
        self.completed_at = None;
        self.phase_started_at = started_at;
    }

    pub(crate) fn refresh_phase(&mut self, phase: AssetRequestPhase, phase_started_at: Instant) {
        self.phase = phase;
        self.phase_started_at = phase_started_at;
    }

    pub(crate) fn complete(&mut self, phase: AssetRequestPhase, completed_at: Instant) {
        self.phase = phase;
        self.completed_at = Some(completed_at);
    }

    pub(crate) fn cancel(&mut self, canceled_at: Instant) {
        self.phase = AssetRequestPhase::Canceled;
        self.completed_at = Some(canceled_at);
    }

    fn mark_failed_counted(&mut self) -> bool {
        if self.failed_counted {
            return false;
        }
        self.failed_counted = true;
        true
    }
}

#[derive(Default)]
pub(crate) struct AssetRequests {
    queued: VecDeque<AssetRequest>,
    active: HashMap<AssetId, AssetRequest>,
    failed: VecDeque<AssetRequestSnapshot>,
    recent_canceled: VecDeque<AssetRequestSnapshot>,
    next_request_id: u64,
    submitted: usize,
    activated: usize,
    canceled: usize,
    failed_count: usize,
    completed: usize,
    total_queue_wait: Duration,
    total_canceled_queue_wait: Duration,
    total_active_time: Duration,
    total_total_time: Duration,
    phase_timings: PhaseTimingTotals,
}

impl AssetRequests {
    pub(crate) fn enqueue(
        &mut self,
        id: AssetId,
        generation: u64,
        requested_type: Option<TypeId>,
        priority: i32,
        queued_at: Instant,
    ) {
        let request_id = AssetRequestId::new(self.next_request_id);
        self.next_request_id = self.next_request_id.wrapping_add(1);
        self.submitted = self.submitted.saturating_add(1);
        self.queued.push_back(AssetRequest::new(
            request_id,
            id,
            generation,
            requested_type,
            priority,
            queued_at,
        ));
    }

    pub(crate) fn pop_queued(&mut self) -> Option<AssetRequest> {
        self.queued.pop_front()
    }

    pub(crate) fn cancel_queued(&mut self, mut request: AssetRequest, canceled_at: Instant) {
        self.total_canceled_queue_wait = self
            .total_canceled_queue_wait
            .saturating_add(request.queued_duration(canceled_at));
        request.cancel(canceled_at);
        debug_assert_eq!(request.phase, AssetRequestPhase::Canceled);
        self.canceled = self.canceled.saturating_add(1);
        self.record_canceled_request(request.snapshot(canceled_at));
    }

    pub(crate) fn activate(
        &mut self,
        mut request: AssetRequest,
        phase: AssetRequestPhase,
        generation: u64,
        started_at: Instant,
    ) {
        request.activate(phase, generation, started_at);
        self.activated = self.activated.saturating_add(1);
        if phase.is_terminal() {
            self.record_completed_request(&request, started_at);
            if phase == AssetRequestPhase::Failed {
                if request.mark_failed_counted() {
                    self.failed_count = self.failed_count.saturating_add(1);
                }
                self.record_failed_request(request.snapshot(started_at));
            }
        } else {
            self.active.entry(request.id).or_insert(request);
        }
    }

    #[cfg(test)]
    pub(crate) fn refresh_active<F>(&mut self, now: Instant, mut state_for: F)
    where
        F: FnMut(AssetId) -> Option<(u64, AssetState)>,
    {
        self.refresh_active_phase(now, |id| {
            state_for(id)
                .map(|(generation, state)| (generation, AssetRequestPhase::from_state(state)))
        });
    }

    pub(crate) fn refresh_active_phase<F>(&mut self, now: Instant, mut phase_for: F)
    where
        F: FnMut(AssetId) -> Option<(u64, AssetRequestPhase)>,
    {
        let mut completed = Vec::new();
        let mut failed = Vec::new();
        for (id, request) in &mut self.active {
            let (generation, phase) =
                phase_for(*id).unwrap_or((request.generation, AssetRequestPhase::Unloaded));
            request.generation = generation;
            if phase.is_terminal() {
                self.phase_timings.record_request_phase(request, now);
                request.complete(phase, now);
                self.completed = self.completed.saturating_add(1);
                let queue_wait = request
                    .started_at
                    .map(|started_at| started_at.saturating_duration_since(request.queued_at))
                    .unwrap_or_default();
                let active_time = request.active_duration(now).unwrap_or_default();
                self.total_queue_wait = self.total_queue_wait.saturating_add(queue_wait);
                self.total_active_time = self.total_active_time.saturating_add(active_time);
                self.total_total_time = self
                    .total_total_time
                    .saturating_add(now.saturating_duration_since(request.queued_at));
                if phase == AssetRequestPhase::Failed {
                    if request.mark_failed_counted() {
                        self.failed_count = self.failed_count.saturating_add(1);
                    }
                    failed.push(request.snapshot(now));
                }
                completed.push(*id);
            } else if phase != request.phase {
                self.phase_timings.record_request_phase(request, now);
                request.refresh_phase(phase, now);
            } else {
                request.refresh_phase(phase, request.phase_started_at);
            }
        }
        for id in completed {
            self.active.remove(&id);
        }
        for snapshot in failed {
            self.record_failed_request(snapshot);
        }
    }

    pub(crate) fn queued_len(&self) -> usize {
        self.queued.len()
    }

    pub(crate) fn active_len(&self) -> usize {
        self.active.len()
    }

    pub(crate) fn submitted_count(&self) -> usize {
        self.submitted
    }

    pub(crate) fn activated_count(&self) -> usize {
        self.activated
    }

    pub(crate) fn canceled_count(&self) -> usize {
        self.canceled
    }

    pub(crate) fn failed_count(&self) -> usize {
        self.failed_count
    }

    pub(crate) fn timing_stats(&self) -> AssetRequestTimingStats {
        AssetRequestTimingStats {
            completed_requests: self.completed,
            average_queue_wait: average_duration(self.total_queue_wait, self.completed),
            average_canceled_queue_wait: average_duration(
                self.total_canceled_queue_wait,
                self.canceled,
            ),
            average_active_time: average_duration(self.total_active_time, self.completed),
            average_total_time: average_duration(self.total_total_time, self.completed),
            loading: self.phase_timings.loading.stats(),
            decoding: self.phase_timings.decoding.stats(),
            waiting_dependencies: self.phase_timings.waiting_dependencies.stats(),
            ready_to_install: self.phase_timings.ready_to_install.stats(),
            installing: self.phase_timings.installing.stats(),
            unloading: self.phase_timings.unloading.stats(),
        }
    }

    pub(crate) fn oldest_queued_age(&self, now: Instant) -> Option<Duration> {
        self.queued
            .iter()
            .map(|request| request.queued_duration(now))
            .max()
    }

    pub(crate) fn oldest_active_age(&self, now: Instant) -> Option<Duration> {
        self.active
            .values()
            .filter_map(|request| request.active_duration(now))
            .max()
    }

    pub(crate) fn queued_snapshots(&self, now: Instant) -> Vec<AssetRequestSnapshot> {
        self.queued
            .iter()
            .map(|request| request.snapshot(now))
            .collect()
    }

    #[cfg(test)]
    pub(crate) fn active_snapshots<F>(
        &self,
        now: Instant,
        mut state_for: F,
    ) -> Vec<AssetRequestSnapshot>
    where
        F: FnMut(AssetId) -> Option<(u64, AssetState)>,
    {
        self.active_snapshots_phase(now, |id| {
            state_for(id)
                .map(|(generation, state)| (generation, AssetRequestPhase::from_state(state)))
        })
    }

    pub(crate) fn active_snapshots_phase<F>(
        &self,
        now: Instant,
        mut phase_for: F,
    ) -> Vec<AssetRequestSnapshot>
    where
        F: FnMut(AssetId) -> Option<(u64, AssetRequestPhase)>,
    {
        let mut snapshots = self
            .active
            .iter()
            .map(|(id, request)| {
                let mut request = request.clone();
                if let Some((generation, phase)) = phase_for(*id) {
                    request.generation = generation;
                    if phase != request.phase {
                        request.refresh_phase(phase, now);
                    }
                }
                request.snapshot(now)
            })
            .collect::<Vec<_>>();
        snapshots.sort_by_key(|snapshot| snapshot.request_id);
        snapshots
    }

    pub(crate) fn failed_snapshots(&self) -> Vec<AssetRequestSnapshot> {
        self.failed.iter().cloned().collect()
    }

    pub(crate) fn canceled_snapshots(&self) -> Vec<AssetRequestSnapshot> {
        self.recent_canceled.iter().cloned().collect()
    }

    pub(crate) fn record_failed_for_asset(
        &mut self,
        id: AssetId,
        generation: u64,
        failure_phase: AssetFailurePhase,
        last_error: String,
        failed_at: Instant,
    ) {
        let Some(request) = self.active.get_mut(&id) else {
            return;
        };
        if request.mark_failed_counted() {
            self.failed_count = self.failed_count.saturating_add(1);
        }
        let mut request = request.clone();
        request.generation = generation;
        request.complete(AssetRequestPhase::Failed, failed_at);
        let mut snapshot = request.snapshot(failed_at);
        snapshot.failure_phase = Some(failure_phase);
        snapshot.last_error = Some(last_error);
        self.record_failed_request(snapshot);
    }

    fn record_completed_request(&mut self, request: &AssetRequest, completed_at: Instant) {
        self.completed = self.completed.saturating_add(1);
        let queue_wait = request
            .started_at
            .map(|started_at| started_at.saturating_duration_since(request.queued_at))
            .unwrap_or_default();
        let active_time = request.active_duration(completed_at).unwrap_or_default();
        self.total_queue_wait = self.total_queue_wait.saturating_add(queue_wait);
        self.total_active_time = self.total_active_time.saturating_add(active_time);
        self.total_total_time = self
            .total_total_time
            .saturating_add(completed_at.saturating_duration_since(request.queued_at));
    }

    fn record_failed_request(&mut self, snapshot: AssetRequestSnapshot) {
        if self
            .failed
            .iter()
            .any(|existing| existing.request_id == snapshot.request_id)
        {
            return;
        }
        if self.failed.len() == REQUEST_SNAPSHOT_LIMIT {
            self.failed.pop_front();
        }
        self.failed.push_back(snapshot);
    }

    fn record_canceled_request(&mut self, snapshot: AssetRequestSnapshot) {
        if self.recent_canceled.len() == REQUEST_SNAPSHOT_LIMIT {
            self.recent_canceled.pop_front();
        }
        self.recent_canceled.push_back(snapshot);
    }
}

#[derive(Default)]
struct PhaseTimingTotals {
    loading: PhaseTimingAccumulator,
    decoding: PhaseTimingAccumulator,
    waiting_dependencies: PhaseTimingAccumulator,
    ready_to_install: PhaseTimingAccumulator,
    installing: PhaseTimingAccumulator,
    unloading: PhaseTimingAccumulator,
}

impl PhaseTimingTotals {
    fn record_request_phase(&mut self, request: &AssetRequest, ended_at: Instant) {
        let duration = request.phase_duration(ended_at);
        match request.phase {
            AssetRequestPhase::Loading => self.loading.record(duration),
            AssetRequestPhase::Decoding => self.decoding.record(duration),
            AssetRequestPhase::WaitingDependencies => self.waiting_dependencies.record(duration),
            AssetRequestPhase::ReadyToInstall => self.ready_to_install.record(duration),
            AssetRequestPhase::Installing => self.installing.record(duration),
            AssetRequestPhase::Unloading => self.unloading.record(duration),
            AssetRequestPhase::Queued
            | AssetRequestPhase::Installed
            | AssetRequestPhase::Unloaded
            | AssetRequestPhase::Failed
            | AssetRequestPhase::Canceled => {}
        }
    }
}

#[derive(Default)]
struct PhaseTimingAccumulator {
    samples: usize,
    total: Duration,
}

impl PhaseTimingAccumulator {
    fn record(&mut self, duration: Duration) {
        self.samples = self.samples.saturating_add(1);
        self.total = self.total.saturating_add(duration);
    }

    fn stats(&self) -> AssetRequestPhaseTimingStats {
        AssetRequestPhaseTimingStats {
            samples: self.samples,
            average: average_duration(self.total, self.samples),
        }
    }
}

fn average_duration(total: Duration, count: usize) -> Option<Duration> {
    if count == 0 {
        return None;
    }

    let nanos = total.as_nanos() / count as u128;
    Some(Duration::from_nanos(
        u64::try_from(nanos).unwrap_or(u64::MAX),
    ))
}

pub(crate) fn activate_queued_requests(
    store: &mut AssetStore,
    requests: &mut AssetRequests,
    now: Instant,
) -> Result<(), AssetError> {
    while let Some(request) = requests.pop_queued() {
        debug_assert_eq!(request.phase, AssetRequestPhase::Queued);
        let _request_id = request.request_id.get();
        if !store.is_referenced(request.id) {
            requests.cancel_queued(request, now);
            continue;
        }

        let activation =
            store.activate_record_for_load(request.id, request.requested_type, request.priority)?;
        let phase = AssetRequestPhase::from_state(activation.state);
        requests.activate(request, phase, activation.load_generation, now);
        debug_assert_ne!(
            AssetRequestPhase::from_state(activation.state),
            AssetRequestPhase::Queued
        );
    }
    Ok(())
}

pub(crate) fn refresh_active_requests(
    store: &AssetStore,
    requests: &mut AssetRequests,
    load_queue: Option<&AssetLoadQueue>,
    now: Instant,
) {
    requests.refresh_active_phase(now, |id| request_phase_for_record(store, load_queue, id));
}

pub(crate) fn refresh_active_requests_after_drive(
    store: &AssetStore,
    requests: &mut AssetRequests,
    load_queue: &mut AssetLoadQueue,
    now: Instant,
) -> usize {
    let canceled = load_queue.cancel_non_loading(store);
    refresh_active_requests(store, requests, Some(load_queue), now);
    canceled
}

pub(crate) fn request_phase_for_record(
    store: &AssetStore,
    load_queue: Option<&AssetLoadQueue>,
    id: AssetId,
) -> Option<(u64, AssetRequestPhase)> {
    let record = store.diagnostic_record(id, Instant::now())?;
    let phase = if record.state == AssetState::Loading {
        load_queue
            .and_then(|load_queue| load_queue.phase(id, record.load_generation))
            .map(asset_source_phase_to_request_phase)
            .unwrap_or(AssetRequestPhase::Loading)
    } else {
        AssetRequestPhase::from_state(record.state)
    };
    Some((record.load_generation, phase))
}

fn asset_source_phase_to_request_phase(phase: AssetSourceLoadPhase) -> AssetRequestPhase {
    match phase {
        AssetSourceLoadPhase::Queued | AssetSourceLoadPhase::Reading => AssetRequestPhase::Loading,
        AssetSourceLoadPhase::Decoding => AssetRequestPhase::Decoding,
    }
}
