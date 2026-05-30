use std::time::Instant;

use super::events::{self, AssetEventLog};
use super::request::AssetRequests;
use super::store::AssetStore;
use super::types::{AssetError, AssetFailurePhase, AssetId};

pub(crate) fn fail_record<F>(
    store: &mut AssetStore,
    events: &mut AssetEventLog,
    id: AssetId,
    error: AssetError,
    phase: AssetFailurePhase,
    dependency_priority: i32,
    asset_type_for: F,
) where
    F: FnMut(AssetId) -> Option<String>,
{
    let outcome = store
        .fail_record(id, error, phase)
        .expect("record should exist");
    let release = store.apply_dependency_lease_update(
        outcome.dependency_update,
        dependency_priority,
        asset_type_for,
    );
    events::push_release_events(events, store, release);
    events::push_failed_event(events, store, id, outcome.event_state);
    let release = store.schedule_release_if_unused(id);
    events::push_release_events(events, store, release);
}

pub(crate) fn fail_record_and_request<F>(
    store: &mut AssetStore,
    events: &mut AssetEventLog,
    requests: &mut AssetRequests,
    id: AssetId,
    error: AssetError,
    phase: AssetFailurePhase,
    dependency_priority: i32,
    failed_at: Instant,
    asset_type_for: F,
) where
    F: FnMut(AssetId) -> Option<String>,
{
    let last_error = error.to_string();
    fail_record(
        store,
        events,
        id,
        error,
        phase,
        dependency_priority,
        asset_type_for,
    );
    if let Some(load_generation) = store.load_generation(id) {
        requests.record_failed_for_asset(id, load_generation, phase, last_error, failed_at);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset::request::{AssetRequestPhase, AssetRequests};
    use crate::asset::store::AssetRecord;
    use crate::asset::types::{AssetEventKind, AssetRequestStatus, AssetState};

    #[test]
    fn failure_helper_records_failure_event_and_schedules_release() {
        let id = AssetId::new();
        let mut store = AssetStore::default();
        let mut record = AssetRecord::new(id, "dummy".to_string());
        record.state = AssetState::Loading;
        store.records.insert(id, record);
        let mut events = AssetEventLog::default();
        let mut cursor = events.cursor();

        fail_record(
            &mut store,
            &mut events,
            id,
            AssetError::AssetNotFound { id },
            AssetFailurePhase::Lookup,
            0,
            |_| None,
        );

        let emitted = events.events_since(&mut cursor);
        assert_eq!(emitted.len(), 1);
        assert_eq!(emitted[0].kind, AssetEventKind::Failed);
        assert_eq!(emitted[0].state, AssetState::Failed);
        assert_eq!(emitted[0].failure_phase, Some(AssetFailurePhase::Lookup));
        assert_eq!(store.records[&id].state, AssetState::Unloading);
    }

    #[test]
    fn failure_helper_records_active_request_snapshot() {
        let id = AssetId::new();
        let now = Instant::now();
        let mut store = AssetStore::default();
        let mut record = AssetRecord::new(id, "dummy".to_string());
        record.state = AssetState::Loading;
        record.load_generation = 7;
        store.records.insert(id, record);
        let mut events = AssetEventLog::default();
        let mut requests = AssetRequests::default();
        requests.enqueue(id, 7, None, 3, now);
        let request = requests.pop_queued().expect("queued request");
        requests.activate(request, AssetRequestPhase::Loading, 7, now);

        fail_record_and_request(
            &mut store,
            &mut events,
            &mut requests,
            id,
            AssetError::AssetNotFound { id },
            AssetFailurePhase::Lookup,
            0,
            now,
            |_| None,
        );

        let failed = requests.failed_snapshots();
        assert_eq!(requests.failed_count(), 1);
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].asset_id, id);
        assert_eq!(failed[0].generation, 7);
        assert_eq!(failed[0].status, AssetRequestStatus::Failed);
        assert_eq!(failed[0].failure_phase, Some(AssetFailurePhase::Lookup));
        let expected_error = AssetError::AssetNotFound { id }.to_string();
        assert_eq!(
            failed[0].last_error.as_deref(),
            Some(expected_error.as_str())
        );
    }
}
