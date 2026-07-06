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
