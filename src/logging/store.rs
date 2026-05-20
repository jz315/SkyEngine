use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use super::entry::QueuedLogEntry;
use super::{LogCursor, LogEntry};

#[cfg_attr(not(any(test, feature = "app")), allow(dead_code))]
enum PushOutcome {
    Inserted(LogEntry),
    Collapsed,
}

struct LogStoreInner {
    entries: VecDeque<LogEntry>,
    next_sequence: u64,
    dropped: u64,
    capacity: usize,
    frame: Option<u64>,
}

impl LogStoreInner {
    fn new(capacity: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            next_sequence: 0,
            dropped: 0,
            capacity: capacity.max(1),
            frame: None,
        }
    }

    #[cfg_attr(not(any(test, feature = "app")), allow(dead_code))]
    fn push(&mut self, entry: LogEntry, collapse: bool) -> PushOutcome {
        if collapse {
            if let Some(last) = self.entries.back_mut() {
                if last.last_sequence.wrapping_add(1) == entry.sequence
                    && last.can_collapse_with(&entry)
                {
                    last.absorb_repeat(&entry);
                    return PushOutcome::Collapsed;
                }
            }
        }

        self.entries.push_back(entry.clone());
        while self.entries.len() > self.capacity {
            self.entries.pop_front();
            self.dropped = self.dropped.wrapping_add(1);
        }
        PushOutcome::Inserted(entry)
    }

    #[cfg_attr(not(feature = "app"), allow(dead_code))]
    fn note_dropped(&mut self, count: u64) {
        self.next_sequence = self.next_sequence.wrapping_add(count);
        self.dropped = self.dropped.wrapping_add(count);
    }

    fn missed_since(&self, cursor: LogCursor) -> u64 {
        let Some(first) = self.entries.front() else {
            return self.next_sequence.saturating_sub(cursor.next_sequence());
        };
        first.sequence.saturating_sub(cursor.next_sequence())
    }
}

/// Thread-safe ring buffer of recent log entries.
pub struct LogStore {
    inner: Mutex<LogStoreInner>,
}

impl LogStore {
    pub fn new() -> Self {
        Self::with_capacity(super::options::DEFAULT_LOG_CAPACITY)
    }

    pub fn shared(capacity: usize) -> Arc<Self> {
        Arc::new(Self::with_capacity(capacity))
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            inner: Mutex::new(LogStoreInner::new(capacity)),
        }
    }

    pub fn entries(&self) -> Vec<LogEntry> {
        self.inner
            .lock()
            .expect("log store lock should not be poisoned")
            .entries
            .iter()
            .cloned()
            .collect()
    }

    pub fn entries_since(&self, cursor: &mut LogCursor) -> Vec<LogEntry> {
        let inner = self
            .inner
            .lock()
            .expect("log store lock should not be poisoned");
        let entries = inner
            .entries
            .iter()
            .filter(|entry| entry.last_sequence >= cursor.next_sequence())
            .cloned()
            .collect();
        cursor.advance_to(inner.next_sequence);
        entries
    }

    pub fn cursor(&self) -> LogCursor {
        LogCursor::from_next_sequence(
            self.inner
                .lock()
                .expect("log store lock should not be poisoned")
                .next_sequence,
        )
    }

    pub fn missed_since(&self, cursor: LogCursor) -> u64 {
        self.inner
            .lock()
            .expect("log store lock should not be poisoned")
            .missed_since(cursor)
    }

    pub fn dropped_count(&self) -> u64 {
        self.inner
            .lock()
            .expect("log store lock should not be poisoned")
            .dropped
    }

    pub fn capacity(&self) -> usize {
        self.inner
            .lock()
            .expect("log store lock should not be poisoned")
            .capacity
    }

    pub fn len(&self) -> usize {
        self.inner
            .lock()
            .expect("log store lock should not be poisoned")
            .entries
            .len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn clear(&self) {
        let mut inner = self
            .inner
            .lock()
            .expect("log store lock should not be poisoned");
        inner.entries.clear();
    }

    pub fn set_frame(&self, frame: Option<u64>) {
        self.inner
            .lock()
            .expect("log store lock should not be poisoned")
            .frame = frame;
    }

    #[cfg_attr(not(feature = "app"), allow(dead_code))]
    pub(crate) fn ingest_queued<I>(
        &self,
        dropped_before: u64,
        entries: I,
        collapse: bool,
    ) -> Vec<LogEntry>
    where
        I: IntoIterator<Item = QueuedLogEntry>,
    {
        let mut inner = self
            .inner
            .lock()
            .expect("log store lock should not be poisoned");
        if dropped_before > 0 {
            inner.note_dropped(dropped_before);
        }

        let mut inserted = Vec::new();
        for queued in entries {
            let sequence = inner.next_sequence;
            inner.next_sequence = inner.next_sequence.wrapping_add(1);
            let entry = LogEntry::from_queued(sequence, queued);
            if let PushOutcome::Inserted(entry) = inner.push(entry, collapse) {
                inserted.push(entry);
            }
        }
        inserted
    }

    #[cfg(test)]
    pub(crate) fn push_record(&self, record: &log::Record<'_>, collapse: bool) -> Option<LogEntry> {
        let mut inner = self
            .inner
            .lock()
            .expect("log store lock should not be poisoned");
        let sequence = inner.next_sequence;
        inner.next_sequence = inner.next_sequence.wrapping_add(1);
        let entry = LogEntry::from_record(sequence, record, inner.frame);
        match inner.push(entry, collapse) {
            PushOutcome::Inserted(entry) => Some(entry),
            PushOutcome::Collapsed => None,
        }
    }
}

impl Default for LogStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use log::{Level, Record};

    use super::*;

    fn push_record(store: &LogStore, message: &'static str, collapse: bool) {
        let args = format_args!("{message}");
        let record = Record::builder()
            .args(args)
            .level(Level::Warn)
            .target("sky_engine::test")
            .module_path(Some("sky_engine::test"))
            .file(Some("src/test.rs"))
            .line(Some(12))
            .build();
        store.push_record(&record, collapse);
    }

    #[test]
    fn entries_since_returns_collapsed_entry_again_after_repeat() {
        let store = LogStore::with_capacity(8);
        let mut cursor = LogCursor::new();

        push_record(&store, "same", true);
        let first = store.entries_since(&mut cursor);
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].repeat_count, 1);

        push_record(&store, "same", true);
        let second = store.entries_since(&mut cursor);
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].repeat_count, 2);
    }

    #[test]
    fn capacity_tracks_dropped_entries() {
        let store = LogStore::with_capacity(1);
        let cursor = LogCursor::new();

        push_record(&store, "one", false);
        push_record(&store, "two", false);

        assert_eq!(store.len(), 1);
        assert_eq!(store.dropped_count(), 1);
        assert_eq!(store.missed_since(cursor), 1);
    }
}
