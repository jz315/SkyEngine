use crate::asset::events::*;
use crate::asset::store::AssetReleaseOutcome;
use crate::asset::store::{AssetRecord, AssetStore};
use crate::asset::{AssetEventCursor, AssetEventKind, AssetId, AssetState};

#[test]
fn event_log_captures_record_context_and_advances_cursor() {
    let id = AssetId::new();
    let dependency = AssetId::new();
    let mut store = AssetStore::default();
    let mut record = AssetRecord::new(id, "dummy".to_string());
    record.load_generation = 7;
    record.loaded_entry_fingerprint = Some("fingerprint".to_string());
    record.loaded_cooked_hash = Some("hash".to_string());
    record.dependencies = vec![dependency];
    record.reload_pending = true;
    record.failure_phase = Some(crate::asset::types::AssetFailurePhase::Decode);
    store.records.insert(id, record);

    let mut log = AssetEventLog::new(8);
    let mut cursor = log.cursor();
    log.push_from_store(&store, id, AssetEventKind::Loaded, AssetState::Loaded);

    let events = log.events_since(&mut cursor);

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].sequence, 0);
    assert_eq!(events[0].id, id);
    assert_eq!(events[0].kind, AssetEventKind::Loaded);
    assert_eq!(events[0].state, AssetState::Loaded);
    assert_eq!(events[0].generation, 7);
    assert_eq!(events[0].asset_type, "dummy");
    assert_eq!(
        events[0].failure_phase,
        Some(crate::asset::types::AssetFailurePhase::Decode)
    );
    assert_eq!(
        events[0].manifest_fingerprint.as_deref(),
        Some("fingerprint")
    );
    assert_eq!(events[0].content_hash.as_deref(), Some("hash"));
    assert_eq!(events[0].dependencies, vec![dependency]);
    assert!(events[0].reload_pending);
    assert!(log.events_since(&mut cursor).is_empty());
}

#[test]
fn event_log_enforces_capacity_but_keeps_monotonic_sequence() {
    let mut store = AssetStore::default();
    let mut log = AssetEventLog::new(2);
    let mut cursor = AssetEventCursor::new(0);

    let first = AssetId::new();
    let second = AssetId::new();
    let third = AssetId::new();
    store
        .records
        .insert(first, AssetRecord::new(first, "dummy".to_string()));
    store
        .records
        .insert(second, AssetRecord::new(second, "dummy".to_string()));
    store
        .records
        .insert(third, AssetRecord::new(third, "dummy".to_string()));

    log.push_from_store(
        &store,
        first,
        AssetEventKind::Installed,
        AssetState::Installed,
    );
    log.push_from_store(&store, second, AssetEventKind::Failed, AssetState::Failed);
    log.push_from_store(
        &store,
        third,
        AssetEventKind::Unloaded,
        AssetState::Unloaded,
    );

    let events = log.events_since(&mut cursor);

    assert_eq!(log.len(), 2);
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].sequence, 1);
    assert_eq!(events[0].id, second);
    assert_eq!(events[1].sequence, 2);
    assert_eq!(events[1].id, third);
    assert_eq!(cursor.next_sequence(), 3);
}

#[test]
fn release_outcome_emits_unloaded_events() {
    let id = AssetId::new();
    let mut store = AssetStore::default();
    store
        .records
        .insert(id, AssetRecord::new(id, "dummy".to_string()));
    let mut log = AssetEventLog::new(8);
    let mut cursor = log.cursor();

    push_release_events(
        &mut log,
        &store,
        AssetReleaseOutcome {
            immediate_unloaded: vec![id],
        },
    );

    let events = log.events_since(&mut cursor);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].id, id);
    assert_eq!(events[0].kind, AssetEventKind::Unloaded);
    assert_eq!(events[0].state, AssetState::Unloaded);
}

#[test]
fn queued_reload_events_are_emitted_for_each_prepared_asset() {
    let first = AssetId::new();
    let second = AssetId::new();
    let mut store = AssetStore::default();
    let mut first_record = AssetRecord::new(first, "dummy".to_string());
    first_record.load_generation = 3;
    let mut second_record = AssetRecord::new(second, "dummy".to_string());
    second_record.load_generation = 5;
    store.records.insert(first, first_record);
    store.records.insert(second, second_record);
    let mut log = AssetEventLog::new(8);
    let mut cursor = log.cursor();

    push_reload_queued_events(&mut log, &store, &[first, second]);

    let events = log.events_since(&mut cursor);
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].id, first);
    assert_eq!(events[0].kind, AssetEventKind::ReloadQueued);
    assert_eq!(events[0].state, AssetState::Loading);
    assert_eq!(events[0].generation, 3);
    assert_eq!(events[1].id, second);
    assert_eq!(events[1].kind, AssetEventKind::ReloadQueued);
    assert_eq!(events[1].state, AssetState::Loading);
    assert_eq!(events[1].generation, 5);
}
