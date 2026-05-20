use log::LevelFilter;

use super::LogConsole;

pub const DEFAULT_LOG_CAPACITY: usize = 1024;

/// Runtime log capture options consumed by the app runner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LogOptions {
    /// Maximum level captured by SkyEngine's logger.
    pub level: LevelFilter,
    /// Which captured messages are mirrored to stderr.
    pub console: LogConsole,
    /// Maximum number of entries retained in the in-memory log buffer.
    pub capacity: usize,
    /// Collapse consecutive duplicate entries into one row with a repeat count.
    pub collapse: bool,
}

impl LogOptions {
    #[inline]
    pub const fn new() -> Self {
        Self {
            level: LevelFilter::Info,
            console: LogConsole::WarningsAndErrors,
            capacity: DEFAULT_LOG_CAPACITY,
            collapse: true,
        }
    }

    #[inline]
    pub const fn off() -> Self {
        Self {
            level: LevelFilter::Off,
            console: LogConsole::Off,
            capacity: DEFAULT_LOG_CAPACITY,
            collapse: true,
        }
    }

    #[inline]
    pub const fn with_level(mut self, level: LevelFilter) -> Self {
        self.level = level;
        self
    }

    #[inline]
    pub const fn with_console(mut self, console: LogConsole) -> Self {
        self.console = console;
        self
    }

    #[inline]
    pub const fn with_capacity(mut self, capacity: usize) -> Self {
        self.capacity = capacity;
        self
    }

    #[inline]
    pub const fn with_collapse(mut self, collapse: bool) -> Self {
        self.collapse = collapse;
        self
    }
}

impl Default for LogOptions {
    fn default() -> Self {
        Self::new()
    }
}
