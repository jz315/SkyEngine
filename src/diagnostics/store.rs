use std::cell::RefCell;
use std::collections::VecDeque;

use rustc_hash::FxHashSet;

use crate::ecs::World;

use super::{DiagnosticEvent, DiagnosticKey};

const DEFAULT_DIAGNOSTIC_CAPACITY: usize = 1024;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DiagnosticCursor {
    next_sequence: u64,
}

impl DiagnosticCursor {
    #[inline]
    pub const fn new() -> Self {
        Self { next_sequence: 0 }
    }

    pub const fn from_next_sequence(next_sequence: u64) -> Self {
        Self { next_sequence }
    }

    #[inline]
    pub const fn next_sequence(self) -> u64 {
        self.next_sequence
    }
}

struct DiagnosticsInner {
    entries: VecDeque<DiagnosticEvent>,
    seen_once: FxHashSet<DiagnosticKey>,
    next_sequence: u64,
    dropped: u64,
    capacity: usize,
}

impl DiagnosticsInner {
    fn new(capacity: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            seen_once: FxHashSet::default(),
            next_sequence: 0,
            dropped: 0,
            capacity: capacity.max(1),
        }
    }

    fn push(&mut self, mut event: DiagnosticEvent) -> DiagnosticEvent {
        event.sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.wrapping_add(1);
        self.entries.push_back(event.clone());
        while self.entries.len() > self.capacity {
            self.entries.pop_front();
            self.dropped = self.dropped.wrapping_add(1);
        }
        event
    }

    fn missed_since(&self, cursor: DiagnosticCursor) -> u64 {
        let Some(first) = self.entries.front() else {
            return 0;
        };
        first.sequence.saturating_sub(cursor.next_sequence)
    }
}

pub struct Diagnostics {
    inner: RefCell<DiagnosticsInner>,
}

impl Diagnostics {
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_DIAGNOSTIC_CAPACITY)
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            inner: RefCell::new(DiagnosticsInner::new(capacity)),
        }
    }

    pub fn resource(world: &mut World) -> &Self {
        if !world.contains_resource::<Self>() {
            world.insert_resource(Self::default());
        }
        world
            .get_resource::<Self>()
            .expect("Diagnostics resource should exist")
    }

    pub fn report(&self, event: impl Into<DiagnosticEvent>) -> DiagnosticEvent {
        self.inner.borrow_mut().push(event.into())
    }

    pub fn report_once(&self, event: impl Into<DiagnosticEvent>) -> Option<DiagnosticEvent> {
        let event = event.into();
        let key = event.dedup_key();
        let mut inner = self.inner.borrow_mut();
        if !inner.seen_once.insert(key) {
            return None;
        }
        Some(inner.push(event))
    }

    pub fn entries(&self) -> Vec<DiagnosticEvent> {
        self.inner.borrow().entries.iter().cloned().collect()
    }

    pub fn cursor(&self) -> DiagnosticCursor {
        DiagnosticCursor::from_next_sequence(self.inner.borrow().next_sequence)
    }

    pub fn events_since(&self, cursor: &mut DiagnosticCursor) -> Vec<DiagnosticEvent> {
        let inner = self.inner.borrow();
        let events = inner
            .entries
            .iter()
            .filter(|diagnostic| diagnostic.sequence >= cursor.next_sequence)
            .cloned()
            .collect();
        cursor.next_sequence = inner.next_sequence;
        events
    }

    pub fn missed_since(&self, cursor: DiagnosticCursor) -> u64 {
        self.inner.borrow().missed_since(cursor)
    }

    pub fn dropped_count(&self) -> u64 {
        self.inner.borrow().dropped
    }

    pub fn capacity(&self) -> usize {
        self.inner.borrow().capacity
    }

    pub fn clear(&self) {
        let mut inner = self.inner.borrow_mut();
        inner.entries.clear();
        inner.seen_once.clear();
    }

    pub fn len(&self) -> usize {
        self.inner.borrow().entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for Diagnostics {
    fn default() -> Self {
        Self::new()
    }
}
