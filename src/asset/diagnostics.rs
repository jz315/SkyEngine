use std::time::Instant;

use super::events::AssetEventLog;
use super::load::{AssetLoadQueue, AssetSourceLoadPhase};
use super::provider::AssetProvider;
use super::reload::AssetReloadController;
use super::request::{request_phase_for_record, AssetRequests};
use super::store::{AssetRecordDiagnostic, AssetStore};
use super::types::{
    AssetConfig, AssetDependencyBlocker, AssetDependencyBlockerReason, AssetDiagnosticsSnapshot,
    AssetError, AssetFailureSnapshot, AssetId, AssetReloadStatus, AssetRequestSnapshot, AssetState,
    AssetStats,
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

pub(crate) fn reload_status(
    config: &AssetConfig,
    reload: &AssetReloadController,
    now: Instant,
) -> AssetReloadStatus {
    reload.status(config, now)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::any::Any;
    use std::sync::Arc;
    use std::time::Duration;

    use crate::asset::install::{
        AssetInstallBudget, AssetInstallContext, AssetInstallPoll, AssetInstallTask,
    };
    use crate::asset::load::{AssetLoadTimingSample, AssetSourceLoadPhase, CompletedLoad};
    use crate::asset::provider::MemoryAssetProvider;
    use crate::asset::request::AssetRequestPhase;
    use crate::asset::store::AssetRecord;
    use crate::asset::types::{
        AssetDependencyBlockerReason, AssetError, AssetFailurePhase, AssetId, AssetManifestEntry,
        AssetRequestProgress, AssetRequestStatus, AssetState,
    };

    #[test]
    fn diagnostics_stats_and_snapshots_are_read_only_views() {
        let id = AssetId::new();
        let queued_at = Instant::now();
        let now = queued_at + Duration::from_millis(25);
        let mut store = AssetStore::default();
        let mut requests = AssetRequests::default();
        let load_queue = AssetLoadQueue::new(1, 4);
        let events = AssetEventLog::default();

        let mut record = AssetRecord::new(id, "dummy".to_string());
        record.state = AssetState::Failed;
        record.strong_ref_count = 2;
        record.dependency_ref_count = 1;
        record.load_generation = 3;
        record.error = Some(AssetError::Internal {
            message: "boom".to_string(),
        });
        record.failure_phase = Some(AssetFailurePhase::Install);
        record.reload_pending = true;
        store.records.insert(id, record);
        requests.enqueue(id, 3, None, 9, queued_at);

        let provider = MemoryAssetProvider::new();
        let stats = stats(&store, &requests, &load_queue, &provider, &events, now);
        assert_eq!(stats.records, 1);
        assert_eq!(stats.queued_requests, 1);
        assert_eq!(stats.load_worker_threads, 1);
        assert_eq!(stats.running_load_jobs, 0);
        assert_eq!(stats.load_queue_capacity, 4);
        assert_eq!(stats.deferred_load_submissions, 0);
        assert_eq!(stats.provider, Default::default());
        assert_eq!(stats.submitted_requests, 1);
        assert_eq!(stats.strong_references, 2);
        assert_eq!(stats.dependency_references, 1);
        assert_eq!(stats.states.failed, 1);
        assert_eq!(stats.load_timings, Default::default());
        assert_eq!(
            stats.oldest_queued_request_age,
            Some(Duration::from_millis(25))
        );

        let queued = queued_request_snapshots(&requests, now);
        assert_eq!(queued.len(), 1);
        assert_eq!(queued[0].asset_id, id);
        assert_eq!(queued[0].status, AssetRequestStatus::Queued);
        assert_eq!(queued[0].progress.label, "queued");
        assert_eq!(queued[0].progress.percent(), 0);
        assert_eq!(queued[0].priority, 9);

        let canceled = canceled_request_snapshots(&requests);
        assert!(canceled.is_empty());

        let failures = failed_asset_snapshots(&store);
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].asset_id, id);
        assert_eq!(failures[0].state, AssetState::Failed);
        assert_eq!(failures[0].phase, Some(AssetFailurePhase::Install));
        assert!(failures[0].reload_pending);
    }

    #[test]
    fn active_request_snapshot_reports_dependency_blockers_and_last_error() {
        let parent = AssetId::new();
        let dependency = AssetId::new();
        let queued_at = Instant::now();
        let started_at = queued_at + Duration::from_millis(1);
        let now = started_at + Duration::from_millis(5);
        let mut store = AssetStore::default();
        let mut requests = AssetRequests::default();
        let load_queue = AssetLoadQueue::new(1, 4);

        let mut parent_record = AssetRecord::new(parent, "dummy".to_string());
        parent_record.state = AssetState::WaitingDependencies;
        parent_record.load_generation = 2;
        parent_record.dependencies = vec![dependency];
        parent_record.failure_phase = Some(AssetFailurePhase::Dependency);
        parent_record.error = Some(AssetError::Internal {
            message: "previous attempt failed".to_string(),
        });
        store.records.insert(parent, parent_record);

        let mut dependency_record = AssetRecord::new(dependency, "dummy".to_string());
        dependency_record.state = AssetState::Loading;
        store.records.insert(dependency, dependency_record);

        requests.enqueue(parent, 2, None, 11, queued_at);
        let request = requests.pop_queued().expect("request should be queued");
        requests.activate(
            request,
            AssetRequestPhase::WaitingDependencies,
            2,
            started_at,
        );

        let active = active_request_snapshots(&store, &requests, &load_queue, now);

        assert_eq!(active.len(), 1);
        assert_eq!(active[0].asset_id, parent);
        assert_eq!(active[0].status, AssetRequestStatus::WaitingDependencies);
        assert_eq!(active[0].progress.label, "waiting dependencies");
        assert_eq!(active[0].progress.percent(), 50);
        assert_eq!(active[0].dependency_blockers, vec![dependency]);
        assert_eq!(active[0].dependency_blocker_details.len(), 1);
        assert_eq!(active[0].dependency_blocker_details[0].asset_id, dependency);
        assert_eq!(
            active[0].dependency_blocker_details[0].reason,
            AssetDependencyBlockerReason::Waiting
        );
        assert_eq!(
            active[0].dependency_blocker_details[0].state,
            Some(AssetState::Loading)
        );
        assert!(active[0].dependency_cycle.is_empty());
        assert_eq!(
            active[0].last_error.as_deref(),
            Some("Internal asset error: previous attempt failed")
        );
        assert_eq!(active[0].failure_phase, Some(AssetFailurePhase::Dependency));
    }

    #[test]
    fn failed_request_snapshot_hydrates_error_and_dependency_blockers() {
        let parent = AssetId::new();
        let dependency = AssetId::new();
        let queued_at = Instant::now();
        let started_at = queued_at + Duration::from_millis(1);
        let failed_at = started_at + Duration::from_millis(5);
        let mut store = AssetStore::default();
        let mut requests = AssetRequests::default();

        let mut parent_record = AssetRecord::new(parent, "dummy".to_string());
        parent_record.state = AssetState::Failed;
        parent_record.load_generation = 4;
        parent_record.dependencies = vec![dependency];
        parent_record.failure_phase = Some(AssetFailurePhase::Dependency);
        parent_record.error = Some(AssetError::DependencyFailed {
            id: parent,
            dependency,
        });
        store.records.insert(parent, parent_record);

        let mut dependency_record = AssetRecord::new(dependency, "dummy".to_string());
        dependency_record.state = AssetState::Failed;
        store.records.insert(dependency, dependency_record);

        requests.enqueue(parent, 4, None, 6, queued_at);
        let request = requests.pop_queued().expect("request should be queued");
        requests.activate(request, AssetRequestPhase::Loading, 4, started_at);
        requests.refresh_active(failed_at, |_| Some((4, AssetState::Failed)));

        let failed = failed_request_snapshots(&store, &requests);

        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].asset_id, parent);
        assert_eq!(failed[0].status, AssetRequestStatus::Failed);
        assert_eq!(failed[0].dependency_blockers, vec![dependency]);
        assert_eq!(failed[0].dependency_blocker_details.len(), 1);
        assert_eq!(
            failed[0].dependency_blocker_details[0].reason,
            AssetDependencyBlockerReason::Failed
        );
        assert_eq!(
            failed[0].dependency_blocker_details[0].state,
            Some(AssetState::Failed)
        );
        assert_eq!(failed[0].failure_phase, Some(AssetFailurePhase::Dependency));
        let expected = format!("Asset `{parent}` dependency `{dependency}` failed");
        assert_eq!(failed[0].last_error.as_deref(), Some(expected.as_str()));
    }

    #[test]
    fn failed_request_snapshot_preserves_failure_context_after_record_recovers() {
        let id = AssetId::new();
        let queued_at = Instant::now();
        let started_at = queued_at + Duration::from_millis(1);
        let failed_at = started_at + Duration::from_millis(5);
        let mut store = AssetStore::default();
        let mut requests = AssetRequests::default();

        let mut record = AssetRecord::new(id, "dummy".to_string());
        record.state = AssetState::Installed;
        record.load_generation = 8;
        store.records.insert(id, record);

        requests.enqueue(id, 8, None, 6, queued_at);
        let request = requests.pop_queued().expect("request should be queued");
        requests.activate(request, AssetRequestPhase::Loading, 8, started_at);
        requests.record_failed_for_asset(
            id,
            8,
            AssetFailurePhase::Lookup,
            "old lookup failure".to_string(),
            failed_at,
        );

        let failed = failed_request_snapshots(&store, &requests);

        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].asset_id, id);
        assert_eq!(failed[0].status, AssetRequestStatus::Failed);
        assert_eq!(failed[0].failure_phase, Some(AssetFailurePhase::Lookup));
        assert_eq!(failed[0].last_error.as_deref(), Some("old lookup failure"));
    }

    #[test]
    fn active_request_snapshot_uses_background_decode_phase() {
        let id = AssetId::new();
        let queued_at = Instant::now();
        let started_at = queued_at + Duration::from_millis(1);
        let now = started_at + Duration::from_millis(5);
        let mut store = AssetStore::default();
        let mut requests = AssetRequests::default();
        let mut load_queue = AssetLoadQueue::new(1, 4);
        let (decode_tx, decode_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();

        let mut record = AssetRecord::new(id, "dummy".to_string());
        record.state = AssetState::Loading;
        record.load_generation = 4;
        store.records.insert(id, record);

        requests.enqueue(id, 4, None, 6, queued_at);
        let request = requests.pop_queued().expect("request should be queued");
        requests.activate(request, AssetRequestPhase::Loading, 4, started_at);

        assert!(load_queue
            .submit_with_phase(id, 4, 0, move |phase| {
                phase.set(AssetSourceLoadPhase::Decoding);
                decode_tx.send(()).expect("decode receiver alive");
                release_rx.recv().expect("release sender alive");
                CompletedLoad {
                    id,
                    generation: 4,
                    entry: AssetManifestEntry {
                        asset_id: id,
                        asset_type: "dummy".to_string(),
                        importer: "dummy.importer".to_string(),
                        cooker: "dummy.cooker".to_string(),
                        version: 1,
                        source_path: format!("{id}.dummy"),
                        cooked_path: format!("{id}.dummyc"),
                        dependencies: Vec::new(),
                        import_settings: serde_json::Value::Null,
                    },
                    cooked_hash: None,
                    timings: AssetLoadTimingSample::default(),
                    result: Err(AssetError::Internal {
                        message: "held for diagnostics".to_string(),
                    }),
                }
            })
            .expect("submit should succeed"));
        decode_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("worker should enter decode phase");

        requests.refresh_active_phase(now, |asset_id| {
            request_phase_for_record(&store, Some(&load_queue), asset_id)
        });
        let active = active_request_snapshots(&store, &requests, &load_queue, now);

        assert_eq!(active.len(), 1);
        assert_eq!(active[0].status, AssetRequestStatus::Decoding);
        assert_eq!(active[0].progress.label, "decoding source");
        assert_eq!(active[0].progress.percent(), 33);

        release_tx.send(()).expect("worker should still wait");
    }

    #[test]
    fn active_request_snapshot_reports_source_worker_wait_progress() {
        let running_id = AssetId::new();
        let queued_id = AssetId::new();
        let queued_at = Instant::now();
        let started_at = queued_at + Duration::from_millis(1);
        let now = started_at + Duration::from_millis(5);
        let mut store = AssetStore::default();
        let mut requests = AssetRequests::default();
        let mut load_queue = AssetLoadQueue::new(1, 4);
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();

        for id in [running_id, queued_id] {
            let mut record = AssetRecord::new(id, "dummy".to_string());
            record.state = AssetState::Loading;
            record.load_generation = 4;
            store.records.insert(id, record);

            requests.enqueue(id, 4, None, 6, queued_at);
            let request = requests.pop_queued().expect("request should be queued");
            requests.activate(request, AssetRequestPhase::Loading, 4, started_at);
        }

        assert!(load_queue
            .submit_with_phase(running_id, 4, 0, move |phase| {
                phase.set(AssetSourceLoadPhase::Reading);
                started_tx.send(()).expect("started receiver alive");
                release_rx.recv().expect("release sender alive");
                CompletedLoad {
                    id: running_id,
                    generation: 4,
                    entry: AssetManifestEntry {
                        asset_id: running_id,
                        asset_type: "dummy".to_string(),
                        importer: "dummy.importer".to_string(),
                        cooker: "dummy.cooker".to_string(),
                        version: 1,
                        source_path: format!("{running_id}.dummy"),
                        cooked_path: format!("{running_id}.dummyc"),
                        dependencies: Vec::new(),
                        import_settings: serde_json::Value::Null,
                    },
                    cooked_hash: None,
                    timings: AssetLoadTimingSample::default(),
                    result: Err(AssetError::Internal {
                        message: "held for diagnostics".to_string(),
                    }),
                }
            })
            .expect("running submit should succeed"));
        started_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("worker should enter read phase");
        assert!(load_queue
            .submit_with_phase(queued_id, 4, 0, move |_phase| CompletedLoad {
                id: queued_id,
                generation: 4,
                entry: AssetManifestEntry {
                    asset_id: queued_id,
                    asset_type: "dummy".to_string(),
                    importer: "dummy.importer".to_string(),
                    cooker: "dummy.cooker".to_string(),
                    version: 1,
                    source_path: format!("{queued_id}.dummy"),
                    cooked_path: format!("{queued_id}.dummyc"),
                    dependencies: Vec::new(),
                    import_settings: serde_json::Value::Null,
                },
                cooked_hash: None,
                timings: AssetLoadTimingSample::default(),
                result: Err(AssetError::Internal {
                    message: "queued for diagnostics".to_string(),
                }),
            })
            .expect("queued submit should succeed"));

        requests.refresh_active_phase(now, |asset_id| {
            request_phase_for_record(&store, Some(&load_queue), asset_id)
        });
        let active = active_request_snapshots(&store, &requests, &load_queue, now);
        let running = active
            .iter()
            .find(|snapshot| snapshot.asset_id == running_id)
            .expect("running request snapshot");
        let queued = active
            .iter()
            .find(|snapshot| snapshot.asset_id == queued_id)
            .expect("queued request snapshot");

        assert_eq!(running.status, AssetRequestStatus::Loading);
        assert_eq!(running.progress.label, "reading source");
        assert_eq!(queued.status, AssetRequestStatus::Loading);
        assert_eq!(queued.progress.label, "queued for source worker");

        release_tx.send(()).expect("worker should still wait");
    }

    #[test]
    fn request_snapshot_reports_missing_dependency_and_cycles() {
        let parent = AssetId::new();
        let missing = AssetId::new();
        let cycle_peer = AssetId::new();
        let queued_at = Instant::now();
        let started_at = queued_at + Duration::from_millis(1);
        let failed_at = started_at + Duration::from_millis(5);
        let mut store = AssetStore::default();
        let mut requests = AssetRequests::default();

        let mut parent_record = AssetRecord::new(parent, "dummy".to_string());
        parent_record.state = AssetState::Failed;
        parent_record.load_generation = 4;
        parent_record.dependencies = vec![missing];
        parent_record.error = Some(AssetError::DependencyCycle {
            cycle: vec![parent, cycle_peer, parent],
        });
        store.records.insert(parent, parent_record);

        requests.enqueue(parent, 4, None, 6, queued_at);
        let request = requests.pop_queued().expect("request should be queued");
        requests.activate(request, AssetRequestPhase::Loading, 4, started_at);
        requests.refresh_active(failed_at, |_| Some((4, AssetState::Failed)));

        let failed = failed_request_snapshots(&store, &requests);

        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].dependency_blockers, vec![missing]);
        assert_eq!(failed[0].dependency_blocker_details.len(), 1);
        assert_eq!(
            failed[0].dependency_blocker_details[0].reason,
            AssetDependencyBlockerReason::Missing
        );
        assert_eq!(failed[0].dependency_blocker_details[0].state, None);
        assert_eq!(failed[0].dependency_cycle, vec![parent, cycle_peer, parent]);
    }

    struct ProgressInstallTask;

    impl AssetInstallTask for ProgressInstallTask {
        type Output = Arc<dyn Any + Send + Sync>;

        fn progress(&self) -> Option<AssetRequestProgress> {
            Some(AssetRequestProgress::new(2, 4, "decoding audio"))
        }

        fn poll_install(
            &mut self,
            _ctx: AssetInstallContext<'_>,
            _budget: AssetInstallBudget,
        ) -> Result<AssetInstallPoll<Self::Output>, AssetError> {
            Ok(AssetInstallPoll::Pending)
        }
    }

    #[test]
    fn active_request_snapshot_uses_install_task_progress_when_available() {
        let id = AssetId::new();
        let queued_at = Instant::now();
        let started_at = queued_at + Duration::from_millis(1);
        let now = started_at + Duration::from_millis(5);
        let mut store = AssetStore::default();
        let mut requests = AssetRequests::default();
        let load_queue = AssetLoadQueue::new(1, 4);

        let mut record = AssetRecord::new(id, "dummy".to_string());
        record.state = AssetState::Installing;
        record.load_generation = 7;
        record.defer_install(Box::new(ProgressInstallTask));
        store.records.insert(id, record);

        requests.enqueue(id, 7, None, 3, queued_at);
        let request = requests.pop_queued().expect("request should be queued");
        requests.activate(request, AssetRequestPhase::Installing, 7, started_at);

        let active = active_request_snapshots(&store, &requests, &load_queue, now);

        assert_eq!(active.len(), 1);
        assert_eq!(active[0].status, AssetRequestStatus::Installing);
        assert_eq!(active[0].progress.label, "decoding audio");
        assert_eq!(active[0].progress.percent(), 50);
    }
}
