use std::time::Instant;

use super::events::AssetEventLog;
use super::load::{AssetLoadQueue, AssetSourceLoadPhase};
use super::provider::AssetProvider;
use super::reload::AssetReloadController;
use super::request::{request_phase_for_record, AssetRequests};
use super::store::{AssetRecordDiagnostic, AssetStore};
use super::types::{
    AssetConfig, AssetDependencyBlocker, AssetDependencyBlockerReason, AssetDiagnosticsSnapshot,
    AssetError, AssetFailureSnapshot, AssetId, AssetRequestSnapshot, AssetState, AssetStats,
};

pub(crate) fn stats(
    store: &AssetStore,
    requests: &AssetRequests,
    load_queue: &AssetLoadQueue,
    provider: &dyn AssetProvider,
    events: &AssetEventLog,
    now: Instant,
) -> AssetStats {
    let records = store.diagnostic_records(now);
    let mut stats = AssetStats {
        records: records.len(),
        queued_requests: requests.queued_len(),
        active_requests: requests.active_len(),
        submitted_requests: requests.submitted_count(),
        activated_requests: requests.activated_count(),
        canceled_requests: requests.canceled_count(),
        failed_requests: requests.failed_count(),
        oldest_queued_request_age: requests.oldest_queued_age(now),
        oldest_active_request_age: requests.oldest_active_age(now),
        request_timings: requests.timing_stats(),
        load_timings: load_queue.timing_stats(),
        inflight_loads: load_queue.inflight_len(),
        load_worker_threads: load_queue.worker_count(),
        running_load_jobs: load_queue.running_len(),
        queued_load_jobs: load_queue.queued_len(),
        load_queue_capacity: load_queue.queue_capacity(),
        oldest_queued_load_job_age: load_queue.oldest_queued_age(now),
        source_load_phases: load_queue.source_load_phase_counts(),
        deferred_load_submissions: load_queue.deferred_submission_count(),
        provider: provider.stats(),
        retained_events: events.len(),
        ..AssetStats::default()
    };

    for record in &records {
        stats.strong_references += record.strong_ref_count;
        stats.dependency_references += record.dependency_ref_count;
        stats.states.record(record.state);
        stats
            .active_state_ages
            .record(record.state, record.state_age);
    }

    stats
}

pub(crate) fn queued_request_snapshots(
    requests: &AssetRequests,
    now: Instant,
) -> Vec<AssetRequestSnapshot> {
    requests.queued_snapshots(now)
}

pub(crate) fn active_request_snapshots(
    store: &AssetStore,
    requests: &AssetRequests,
    load_queue: &AssetLoadQueue,
    now: Instant,
) -> Vec<AssetRequestSnapshot> {
    let mut snapshots = requests.active_snapshots_phase(now, |id| {
        request_phase_for_record(store, Some(load_queue), id)
    });
    for snapshot in &mut snapshots {
        let Some(record) = store.diagnostic_record(snapshot.asset_id, now) else {
            continue;
        };
        hydrate_dependency_diagnostics(snapshot, store, &record);
        snapshot.last_error = record.error.as_ref().map(ToString::to_string);
        snapshot.failure_phase = record.failure_phase;
        if record.state == AssetState::Loading {
            if let Some(source_phase) = load_queue.phase(snapshot.asset_id, record.load_generation)
            {
                snapshot.progress = source_load_progress(source_phase);
            }
        }
        if let Some(progress) = record.install_progress {
            snapshot.progress = progress;
        }
    }
    snapshots
}

fn source_load_progress(phase: AssetSourceLoadPhase) -> super::types::AssetRequestProgress {
    match phase {
        AssetSourceLoadPhase::Queued => {
            super::types::AssetRequestProgress::new(1, 6, "queued for source worker")
        }
        AssetSourceLoadPhase::Reading => {
            super::types::AssetRequestProgress::new(1, 6, "reading source")
        }
        AssetSourceLoadPhase::Decoding => {
            super::types::AssetRequestProgress::new(2, 6, "decoding source")
        }
    }
}

pub(crate) fn failed_request_snapshots(
    store: &AssetStore,
    requests: &AssetRequests,
) -> Vec<AssetRequestSnapshot> {
    let mut snapshots = requests.failed_snapshots();
    for snapshot in &mut snapshots {
        let Some(record) = store.diagnostic_record(snapshot.asset_id, Instant::now()) else {
            continue;
        };
        hydrate_dependency_diagnostics(snapshot, store, &record);
        if snapshot.last_error.is_none() {
            snapshot.last_error = record.error.as_ref().map(ToString::to_string);
        }
        if snapshot.failure_phase.is_none() {
            snapshot.failure_phase = record.failure_phase;
        }
    }
    snapshots
}

pub(crate) fn canceled_request_snapshots(requests: &AssetRequests) -> Vec<AssetRequestSnapshot> {
    requests.canceled_snapshots()
}

fn hydrate_dependency_diagnostics(
    snapshot: &mut AssetRequestSnapshot,
    store: &AssetStore,
    record: &AssetRecordDiagnostic,
) {
    let details = dependency_blocker_details(store, &record.dependencies, record.error.as_ref());
    snapshot.dependency_blockers = details.iter().map(|blocker| blocker.asset_id).collect();
    snapshot.dependency_blocker_details = details;
    snapshot.dependency_cycle = dependency_cycle(record.error.as_ref());
}

fn dependency_blocker_details(
    store: &AssetStore,
    dependencies: &[AssetId],
    error: Option<&AssetError>,
) -> Vec<AssetDependencyBlocker> {
    let mut details = dependencies
        .iter()
        .copied()
        .filter_map(|dependency| dependency_blocker_for(store, dependency, None))
        .collect::<Vec<_>>();

    if let Some((dependency, reason)) = dependency_error_blocker(error) {
        if let Some(existing) = details
            .iter_mut()
            .find(|blocker| blocker.asset_id == dependency)
        {
            existing.reason = reason;
        } else if let Some(blocker) = dependency_blocker_for(store, dependency, Some(reason)) {
            details.push(blocker);
        }
    }

    details
}

fn dependency_blocker_for(
    store: &AssetStore,
    dependency: AssetId,
    reason_override: Option<AssetDependencyBlockerReason>,
) -> Option<AssetDependencyBlocker> {
    let state = store.record_state(dependency);
    if state == Some(AssetState::Installed) && reason_override.is_none() {
        return None;
    }

    let reason = reason_override.unwrap_or_else(|| match state {
        None => AssetDependencyBlockerReason::Missing,
        Some(AssetState::Failed) => AssetDependencyBlockerReason::Failed,
        Some(_) => AssetDependencyBlockerReason::Waiting,
    });

    Some(AssetDependencyBlocker {
        asset_id: dependency,
        reason,
        state,
    })
}

fn dependency_error_blocker(
    error: Option<&AssetError>,
) -> Option<(AssetId, AssetDependencyBlockerReason)> {
    match error {
        Some(AssetError::MissingDependency { dependency, .. }) => {
            Some((*dependency, AssetDependencyBlockerReason::Missing))
        }
        Some(AssetError::DependencyFailed { dependency, .. }) => {
            Some((*dependency, AssetDependencyBlockerReason::Failed))
        }
        _ => None,
    }
}

fn dependency_cycle(error: Option<&AssetError>) -> Vec<AssetId> {
    match error {
        Some(AssetError::DependencyCycle { cycle }) => cycle.clone(),
        _ => Vec::new(),
    }
}

pub(crate) fn failed_asset_snapshots(store: &AssetStore) -> Vec<AssetFailureSnapshot> {
    let mut snapshots = store
        .diagnostic_records(Instant::now())
        .into_iter()
        .filter_map(|record| {
            record.error.clone().map(|error| AssetFailureSnapshot {
                asset_id: record.asset_id,
                asset_type: record.asset_type.clone(),
                state: record.state,
                generation: record.load_generation,
                phase: record.failure_phase,
                error,
                reload_pending: record.reload_pending,
            })
        })
        .collect::<Vec<_>>();
    snapshots.sort_by(|left, right| left.asset_id.to_string().cmp(&right.asset_id.to_string()));
    snapshots
}

pub(crate) fn snapshot(
    config: &AssetConfig,
    store: &AssetStore,
    requests: &AssetRequests,
    load_queue: &AssetLoadQueue,
    provider: &dyn AssetProvider,
    events: &AssetEventLog,
    reload: &AssetReloadController,
    now: Instant,
) -> AssetDiagnosticsSnapshot {
    let reload_status = reload.status(config, now);
    AssetDiagnosticsSnapshot {
        stats: stats(store, requests, load_queue, provider, events, now),
        queued_requests: queued_request_snapshots(requests, now),
        active_requests: active_request_snapshots(store, requests, load_queue, now),
        canceled_requests: canceled_request_snapshots(requests),
        failed_requests: failed_request_snapshots(store, requests),
        failures: failed_asset_snapshots(store),
        last_reload_report: reload_status.last_report.clone(),
        reload_status,
    }
}
