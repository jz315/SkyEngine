use std::path::Path;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct QueuedLogEntry {
    pub level: log::Level,
    pub target: String,
    pub message: String,
    pub module_path: Option<String>,
    pub file: Option<String>,
    pub line: Option<u32>,
    pub frame: Option<u64>,
}

impl QueuedLogEntry {
    pub(crate) fn from_record(record: &log::Record<'_>, frame: Option<u64>) -> Self {
        Self {
            level: record.level(),
            target: record.target().to_owned(),
            message: record.args().to_string(),
            module_path: record.module_path().map(str::to_owned),
            file: record.file().map(str::to_owned),
            line: record.line(),
            frame,
        }
    }
}

/// Cursor used to read only log entries that arrived since the last read.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LogCursor {
    next_sequence: u64,
}

impl LogCursor {
    #[inline]
    pub const fn new() -> Self {
        Self { next_sequence: 0 }
    }

    #[inline]
    pub const fn from_next_sequence(next_sequence: u64) -> Self {
        Self { next_sequence }
    }

    #[inline]
    pub const fn next_sequence(self) -> u64 {
        self.next_sequence
    }

    pub(crate) fn advance_to(&mut self, next_sequence: u64) {
        self.next_sequence = next_sequence;
    }
}

/// One retained log entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogEntry {
    /// Sequence assigned when this entry was first inserted.
    pub sequence: u64,
    /// Most recent sequence represented by this entry.
    pub last_sequence: u64,
    pub level: log::Level,
    pub target: String,
    pub message: String,
    pub module_path: Option<String>,
    pub file: Option<String>,
    pub line: Option<u32>,
    pub frame: Option<u64>,
    /// Number of consecutive identical records represented by this entry.
    pub repeat_count: u32,
}

impl LogEntry {
    #[cfg(test)]
    pub(crate) fn from_record(sequence: u64, record: &log::Record<'_>, frame: Option<u64>) -> Self {
        Self::from_queued(sequence, QueuedLogEntry::from_record(record, frame))
    }

    pub(crate) fn from_queued(sequence: u64, queued: QueuedLogEntry) -> Self {
        Self {
            sequence,
            last_sequence: sequence,
            level: queued.level,
            target: queued.target,
            message: queued.message,
            module_path: queued.module_path,
            file: queued.file,
            line: queued.line,
            frame: queued.frame,
            repeat_count: 1,
        }
    }

    #[inline]
    pub fn short_file(&self) -> Option<&str> {
        self.file
            .as_deref()
            .and_then(|file| Path::new(file).file_name())
            .and_then(|file| file.to_str())
            .or(self.file.as_deref())
    }

    pub(crate) fn can_collapse_with(&self, other: &Self) -> bool {
        self.level == other.level
            && self.target == other.target
            && self.message == other.message
            && self.file == other.file
            && self.line == other.line
    }

    pub(crate) fn absorb_repeat(&mut self, other: &Self) {
        self.last_sequence = other.last_sequence;
        self.frame = other.frame;
        self.repeat_count = self.repeat_count.saturating_add(1);
    }
}
