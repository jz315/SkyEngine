use std::collections::VecDeque;

use super::store::{AssetReleaseOutcome, AssetStore};
use super::types::{AssetEvent, AssetEventCursor, AssetEventKind, AssetId, AssetState};

const DEFAULT_ASSET_EVENT_LOG_CAPACITY: usize = 1024;

pub(crate) struct AssetEventLog {
    events: VecDeque<AssetEvent>,
    next_sequence: u64,
    capacity: usize,
}

impl Default for AssetEventLog {
    fn default() -> Self {
        Self::new(DEFAULT_ASSET_EVENT_LOG_CAPACITY)
    }
}

impl AssetEventLog {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            events: VecDeque::new(),
            next_sequence: 0,
            capacity,
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.events.len()
    }

    pub(crate) fn cursor(&self) -> AssetEventCursor {
        AssetEventCursor::new(self.next_sequence)
    }

    pub(crate) fn events_since(&self, cursor: &mut AssetEventCursor) -> Vec<AssetEvent> {
        let next_sequence = cursor.next_sequence();
        let events = self
            .events
            .iter()
            .filter(|event| event.sequence >= next_sequence)
            .cloned()
            .collect();
        cursor.set_next_sequence(self.next_sequence);
        events
    }

    pub(crate) fn push_from_store(
        &mut self,
        store: &AssetStore,
        id: AssetId,
        kind: AssetEventKind,
        state: AssetState,
    ) {
        let context = store.event_record_context(id);

        let event = AssetEvent {
            sequence: self.next_sequence,
            id,
            kind,
            state,
            generation: context.generation,
            asset_type: context.asset_type,
            failure_phase: context.failure_phase,
            manifest_fingerprint: context.manifest_fingerprint,
            content_hash: context.content_hash,
            dependencies: context.dependencies,
            reload_pending: context.reload_pending,
        };
        self.next_sequence = self.next_sequence.wrapping_add(1);
        self.events.push_back(event);
        while self.events.len() > self.capacity {
            self.events.pop_front();
        }
    }
}

pub(crate) fn push_release_events(
    log: &mut AssetEventLog,
    store: &AssetStore,
    outcome: AssetReleaseOutcome,
) {
    for id in outcome.immediate_unloaded {
        push_unloaded_event(log, store, id);
    }
}

pub(crate) fn push_unloaded_event(log: &mut AssetEventLog, store: &AssetStore, id: AssetId) {
    log.push_from_store(store, id, AssetEventKind::Unloaded, AssetState::Unloaded);
}

pub(crate) fn push_loaded_event(log: &mut AssetEventLog, store: &AssetStore, id: AssetId) {
    log.push_from_store(store, id, AssetEventKind::Loaded, AssetState::Loaded);
}

pub(crate) fn push_installed_event(log: &mut AssetEventLog, store: &AssetStore, id: AssetId) {
    log.push_from_store(store, id, AssetEventKind::Installed, AssetState::Installed);
}

pub(crate) fn push_reloaded_event(log: &mut AssetEventLog, store: &AssetStore, id: AssetId) {
    log.push_from_store(store, id, AssetEventKind::Reloaded, AssetState::Installed);
}

pub(crate) fn push_failed_event(
    log: &mut AssetEventLog,
    store: &AssetStore,
    id: AssetId,
    state: AssetState,
) {
    log.push_from_store(store, id, AssetEventKind::Failed, state);
}

pub(crate) fn push_reload_queued_events(
    log: &mut AssetEventLog,
    store: &AssetStore,
    queued: &[AssetId],
) {
    for id in queued {
        log.push_from_store(
            store,
            *id,
            AssetEventKind::ReloadQueued,
            AssetState::Loading,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset::store::{AssetRecord, AssetStore};

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
}
