//! Structured runtime diagnostics for engine services.
//!
//! The regular [`crate::logging`] module captures human-facing log text. This
//! module keeps small structured events that tools, overlays, tests, and
//! editor integrations can consume without parsing log messages.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

pub const DEFAULT_DIAGNOSTIC_CAPACITY: usize = 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DiagnosticCursor {
    next_sequence: u64,
}

impl DiagnosticCursor {
    #[inline]
    #[must_use]
    pub const fn new() -> Self {
        Self { next_sequence: 0 }
    }

    #[inline]
    #[must_use]
    pub const fn from_next_sequence(next_sequence: u64) -> Self {
        Self { next_sequence }
    }

    #[inline]
    #[must_use]
    pub const fn next_sequence(self) -> u64 {
        self.next_sequence
    }

    fn advance_to(&mut self, next_sequence: u64) {
        self.next_sequence = next_sequence;
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiagnosticField {
    pub key: String,
    pub value: String,
}

impl DiagnosticField {
    #[must_use]
    pub fn new(key: impl Into<String>, value: impl ToString) -> Self {
        Self {
            key: key.into(),
            value: value.to_string(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiagnosticEvent {
    pub sequence: u64,
    pub frame: Option<u64>,
    pub category: String,
    pub code: String,
    pub severity: DiagnosticSeverity,
    pub message: String,
    pub fields: Vec<DiagnosticField>,
}

impl DiagnosticEvent {
    #[must_use]
    pub fn new(
        category: impl Into<String>,
        code: impl Into<String>,
        severity: DiagnosticSeverity,
        message: impl Into<String>,
    ) -> Self {
        Self {
            sequence: 0,
            frame: None,
            category: category.into(),
            code: code.into(),
            severity,
            message: message.into(),
            fields: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_frame(mut self, frame: Option<u64>) -> Self {
        self.frame = frame;
        self
    }

    #[must_use]
    pub fn with_field(mut self, key: impl Into<String>, value: impl ToString) -> Self {
        self.fields.push(DiagnosticField::new(key, value));
        self
    }
}

#[derive(Debug)]
struct DiagnosticsInner {
    events: VecDeque<DiagnosticEvent>,
    next_sequence: u64,
    dropped: u64,
    capacity: usize,
}

impl DiagnosticsInner {
    fn new(capacity: usize) -> Self {
        Self {
            events: VecDeque::new(),
            next_sequence: 0,
            dropped: 0,
            capacity: capacity.max(1),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Diagnostics {
    inner: Arc<Mutex<DiagnosticsInner>>,
}

impl Diagnostics {
    #[must_use]
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_DIAGNOSTIC_CAPACITY)
    }

    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(DiagnosticsInner::new(capacity))),
        }
    }

    pub fn push(&self, mut event: DiagnosticEvent) -> u64 {
        let mut inner = self
            .inner
            .lock()
            .expect("diagnostics lock should not be poisoned");
        let sequence = inner.next_sequence;
        inner.next_sequence = inner.next_sequence.wrapping_add(1);
        event.sequence = sequence;
        inner.events.push_back(event);
        while inner.events.len() > inner.capacity {
            inner.events.pop_front();
            inner.dropped = inner.dropped.wrapping_add(1);
        }
        sequence
    }

    #[must_use]
    pub fn events(&self) -> Vec<DiagnosticEvent> {
        self.inner
            .lock()
            .expect("diagnostics lock should not be poisoned")
            .events
            .iter()
            .cloned()
            .collect()
    }

    pub fn events_since(&self, cursor: &mut DiagnosticCursor) -> Vec<DiagnosticEvent> {
        let inner = self
            .inner
            .lock()
            .expect("diagnostics lock should not be poisoned");
        let events = inner
            .events
            .iter()
            .filter(|event| event.sequence >= cursor.next_sequence())
            .cloned()
            .collect();
        cursor.advance_to(inner.next_sequence);
        events
    }

    #[must_use]
    pub fn cursor(&self) -> DiagnosticCursor {
        DiagnosticCursor::from_next_sequence(
            self.inner
                .lock()
                .expect("diagnostics lock should not be poisoned")
                .next_sequence,
        )
    }

    #[must_use]
    pub fn dropped_count(&self) -> u64 {
        self.inner
            .lock()
            .expect("diagnostics lock should not be poisoned")
            .dropped
    }

    #[must_use]
    pub fn capacity(&self) -> usize {
        self.inner
            .lock()
            .expect("diagnostics lock should not be poisoned")
            .capacity
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.inner
            .lock()
            .expect("diagnostics lock should not be poisoned")
            .events
            .len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn clear(&self) {
        self.inner
            .lock()
            .expect("diagnostics lock should not be poisoned")
            .events
            .clear();
    }
}

impl Default for Diagnostics {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_since_advances_cursor() {
        let diagnostics = Diagnostics::with_capacity(4);
        let mut cursor = DiagnosticCursor::new();
        diagnostics.push(DiagnosticEvent::new(
            "test",
            "test.one",
            DiagnosticSeverity::Info,
            "one",
        ));

        let first = diagnostics.events_since(&mut cursor);
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].sequence, 0);
        assert!(diagnostics.events_since(&mut cursor).is_empty());
    }

    #[test]
    fn capacity_drops_oldest_events() {
        let diagnostics = Diagnostics::with_capacity(1);
        diagnostics.push(DiagnosticEvent::new(
            "test",
            "test.one",
            DiagnosticSeverity::Info,
            "one",
        ));
        diagnostics.push(DiagnosticEvent::new(
            "test",
            "test.two",
            DiagnosticSeverity::Info,
            "two",
        ));

        let events = diagnostics.events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].code, "test.two");
        assert_eq!(diagnostics.dropped_count(), 1);
    }
}
