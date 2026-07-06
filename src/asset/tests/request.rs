use crate::asset::load::AssetLoadQueue;
use crate::asset::request::*;
use crate::asset::store::AssetStore;
use crate::asset::{AssetError, AssetId, AssetRequestStatus, AssetState};
use std::sync::mpsc;
use std::time::{Duration, Instant};

#[test]
fn request_phase_maps_record_states() {
    assert_eq!(
        AssetRequestPhase::from_state(AssetState::Loaded),
        AssetRequestPhase::ReadyToInstall
    );
    assert_eq!(
        AssetRequestPhase::from_state(AssetState::WaitingDependencies),
        AssetRequestPhase::WaitingDependencies
    );
    assert_eq!(
        AssetRequestPhase::from_state(AssetState::Uninstalling),
        AssetRequestPhase::Unloading
    );
}

#[test]
fn request_snapshot_reports_generation_and_active_age() {
    let queued_at = Instant::now();
    let started_at = queued_at + Duration::from_millis(5);
    let now = started_at + Duration::from_millis(10);
    let mut request = AssetRequest::new(
        AssetRequestId::new(7),
        AssetId::new(),
        3,
        None,
        9,
        queued_at,
    );

    request.activate(AssetRequestPhase::Loading, 4, started_at);
    let snapshot = request.snapshot(now);

    assert_eq!(snapshot.request_id, 7);
    assert_eq!(snapshot.generation, 4);
    assert_eq!(snapshot.priority, 9);
    assert_eq!(snapshot.progress.completed_steps, 1);
    assert_eq!(snapshot.progress.total_steps, 6);
    assert_eq!(snapshot.progress.label, "loading source");
    assert_eq!(snapshot.progress.percent(), 16);
    assert_eq!(snapshot.queued_age, Duration::from_millis(15));
    assert_eq!(snapshot.active_age, Some(Duration::from_millis(10)));
    assert_eq!(snapshot.phase_age, Duration::from_millis(10));
}

#[test]
fn request_manager_tracks_queued_active_and_canceled_counts() {
    let queued_at = Instant::now();
    let started_at = queued_at + Duration::from_millis(2);
    let mut requests = AssetRequests::default();
    let first = AssetId::new();
    let second = AssetId::new();

    requests.enqueue(first, 0, None, 4, queued_at);
    requests.enqueue(second, 0, None, 2, queued_at);
    assert_eq!(requests.queued_len(), 2);
    assert_eq!(requests.submitted_count(), 2);
    assert!(requests.oldest_queued_age(started_at).is_some());

    let request = requests
        .pop_queued()
        .expect("first request should be queued");
    requests.activate(request, AssetRequestPhase::Loading, 1, started_at);
    let request = requests
        .pop_queued()
        .expect("second request should be queued");
    requests.cancel_queued(request, started_at);

    assert_eq!(requests.queued_len(), 0);
    assert_eq!(requests.active_len(), 1);
    assert_eq!(requests.activated_count(), 1);
    assert_eq!(requests.canceled_count(), 1);
    assert_eq!(
        requests.oldest_active_age(started_at + Duration::from_millis(3)),
        Some(Duration::from_millis(3))
    );
    let canceled = requests.canceled_snapshots();
    assert_eq!(canceled.len(), 1);
    assert_eq!(canceled[0].asset_id, second);
    assert_eq!(canceled[0].status, AssetRequestStatus::Canceled);
    assert_eq!(canceled[0].priority, 2);
    assert_eq!(
        requests.active_snapshots(started_at, |id| Some((
            if id == first { 1 } else { 0 },
            AssetState::Loading,
        )))[0]
            .asset_id,
        first
    );

    let completed_at = started_at + Duration::from_millis(8);
    requests.refresh_active(completed_at, |id| {
        assert_eq!(id, first);
        Some((1, AssetState::Installed))
    });
    assert_eq!(requests.active_len(), 0);
    let timings = requests.timing_stats();
    assert_eq!(timings.completed_requests, 1);
    assert_eq!(timings.average_queue_wait, Some(Duration::from_millis(2)));
    assert_eq!(
        timings.average_canceled_queue_wait,
        Some(Duration::from_millis(2))
    );
    assert_eq!(timings.average_active_time, Some(Duration::from_millis(8)));
    assert_eq!(timings.average_total_time, Some(Duration::from_millis(10)));
    assert_eq!(timings.loading.samples, 1);
    assert_eq!(timings.loading.average, Some(Duration::from_millis(8)));
    assert_eq!(timings.decoding.samples, 0);
    assert_eq!(timings.installing.samples, 0);
    assert_eq!(timings.unloading.samples, 0);
}

#[test]
fn request_manager_tracks_phase_timing_when_phase_changes() {
    let queued_at = Instant::now();
    let started_at = queued_at + Duration::from_millis(1);
    let decoding_at = started_at + Duration::from_millis(4);
    let waiting_at = decoding_at + Duration::from_millis(2);
    let installing_at = waiting_at + Duration::from_millis(6);
    let unloading_at = installing_at + Duration::from_millis(3);
    let completed_at = unloading_at + Duration::from_millis(5);
    let mut requests = AssetRequests::default();
    let id = AssetId::new();

    requests.enqueue(id, 0, None, 4, queued_at);
    let request = requests.pop_queued().expect("request should be queued");
    requests.activate(request, AssetRequestPhase::Loading, 1, started_at);
    requests.refresh_active_phase(decoding_at, |_| Some((1, AssetRequestPhase::Decoding)));
    requests.refresh_active(waiting_at, |_| Some((1, AssetState::WaitingDependencies)));
    requests.refresh_active(installing_at, |_| Some((1, AssetState::Installing)));
    requests.refresh_active_phase(unloading_at, |_| Some((1, AssetRequestPhase::Unloading)));
    requests.refresh_active(completed_at, |_| Some((1, AssetState::Unloaded)));

    let timings = requests.timing_stats();
    assert_eq!(timings.loading.samples, 1);
    assert_eq!(timings.loading.average, Some(Duration::from_millis(4)));
    assert_eq!(timings.decoding.samples, 1);
    assert_eq!(timings.decoding.average, Some(Duration::from_millis(2)));
    assert_eq!(timings.waiting_dependencies.samples, 1);
    assert_eq!(
        timings.waiting_dependencies.average,
        Some(Duration::from_millis(6))
    );
    assert_eq!(timings.installing.samples, 1);
    assert_eq!(timings.installing.average, Some(Duration::from_millis(3)));
    assert_eq!(timings.unloading.samples, 1);
    assert_eq!(timings.unloading.average, Some(Duration::from_millis(5)));
}

#[test]
fn request_manager_keeps_recent_failed_request_snapshots() {
    let queued_at = Instant::now();
    let started_at = queued_at + Duration::from_millis(2);
    let failed_at = started_at + Duration::from_millis(7);
    let mut requests = AssetRequests::default();
    let id = AssetId::new();

    requests.enqueue(id, 0, None, 8, queued_at);
    let request = requests.pop_queued().expect("request should be queued");
    requests.activate(request, AssetRequestPhase::Loading, 3, started_at);
    requests.refresh_active(failed_at, |_| Some((3, AssetState::Failed)));

    let failed = requests.failed_snapshots();
    assert_eq!(requests.failed_count(), 1);
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].asset_id, id);
    assert_eq!(failed[0].generation, 3);
    assert_eq!(failed[0].priority, 8);
    assert_eq!(failed[0].status, AssetRequestStatus::Failed);
    assert_eq!(failed[0].queued_age, Duration::from_millis(9));
    assert_eq!(failed[0].active_age, Some(Duration::from_millis(7)));
    assert_eq!(failed[0].phase_age, Duration::from_millis(7));
}

#[test]
fn request_driver_activates_referenced_records_and_cancels_unreferenced() {
    let active_id = AssetId::new();
    let canceled_id = AssetId::new();
    let now = Instant::now();
    let mut store = AssetStore::default();
    let mut active_record = crate::asset::store::AssetRecord::new(active_id, "dummy".to_string());
    active_record.strong_ref_count = 1;
    store.records.insert(active_id, active_record);
    store.records.insert(
        canceled_id,
        crate::asset::store::AssetRecord::new(canceled_id, "dummy".to_string()),
    );

    let mut requests = AssetRequests::default();
    requests.enqueue(active_id, 0, None, 5, now);
    requests.enqueue(canceled_id, 0, None, 3, now);

    activate_queued_requests(&mut store, &mut requests, now)
        .expect("request activation should succeed");

    assert_eq!(requests.queued_len(), 0);
    assert_eq!(requests.active_len(), 1);
    assert_eq!(requests.activated_count(), 1);
    assert_eq!(requests.canceled_count(), 1);
    assert_eq!(store.records[&active_id].state, AssetState::Loading);
    assert_eq!(store.records[&active_id].load_generation, 1);
    assert_eq!(store.records[&active_id].load_priority, 5);
    assert_eq!(store.records[&canceled_id].state, AssetState::Unloaded);

    store
        .records
        .get_mut(&active_id)
        .expect("active record")
        .state = AssetState::Installed;
    refresh_active_requests(&store, &mut requests, None, now + Duration::from_millis(1));
    assert_eq!(requests.active_len(), 0);
}

#[test]
fn request_driver_finalizes_stale_source_loads_before_phase_refresh() {
    let id = AssetId::new();
    let now = Instant::now();
    let mut store = AssetStore::default();
    let mut record = crate::asset::store::AssetRecord::new(id, "dummy".to_string());
    record.state = AssetState::Loading;
    record.strong_ref_count = 1;
    record.load_generation = 1;
    store.records.insert(id, record);

    let mut requests = AssetRequests::default();
    requests.enqueue(id, 1, None, 7, now);
    let request = requests.pop_queued().expect("request should be queued");
    requests.activate(request, AssetRequestPhase::Loading, 1, now);

    let mut load_queue = AssetLoadQueue::new(1, 2);
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    assert!(load_queue
        .submit(id, 1, 7, move || {
            started_tx.send(()).expect("started receiver alive");
            release_rx.recv().expect("release sender alive");
            crate::asset::load::CompletedLoad {
                id,
                generation: 1,
                entry: crate::asset::types::AssetManifestEntry {
                    asset_id: id,
                    asset_type: "dummy".to_string(),
                    importer: "dummy".to_string(),
                    cooker: "dummy".to_string(),
                    version: 1,
                    source_path: "dummy.asset".to_string(),
                    cooked_path: "dummy.cooked".to_string(),
                    dependencies: Vec::new(),
                    import_settings: serde_json::Value::Null,
                },
                cooked_hash: None,
                timings: crate::asset::load::AssetLoadTimingSample::default(),
                result: Err(AssetError::Internal {
                    message: "stale completion".to_string(),
                }),
            }
        })
        .expect("submit should succeed"));
    started_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("worker should start running load");

    store
        .records
        .get_mut(&id)
        .expect("record should exist")
        .load_generation = 2;

    let canceled = refresh_active_requests_after_drive(
        &store,
        &mut requests,
        &mut load_queue,
        now + Duration::from_millis(3),
    );

    assert_eq!(canceled, 1);
    assert_eq!(load_queue.inflight_len(), 0);
    let snapshots = requests.active_snapshots_phase(now + Duration::from_millis(3), |asset_id| {
        request_phase_for_record(&store, Some(&load_queue), asset_id)
    });
    assert_eq!(snapshots.len(), 1);
    assert_eq!(snapshots[0].asset_id, id);
    assert_eq!(snapshots[0].generation, 2);
    assert_eq!(snapshots[0].progress.label, "loading source");

    release_tx.send(()).expect("worker should still wait");
}
