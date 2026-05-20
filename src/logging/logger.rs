use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};

use crossbeam_queue::ArrayQueue;

use super::console::write_entry_to_stderr;
use super::entry::QueuedLogEntry;
use super::{LogConsole, LogOptions, LogStore};

const NO_FRAME: u64 = u64::MAX;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LogInstallStatus {
    Installed,
    AlreadyInstalled,
}

struct LoggerState {
    queue: ArrayQueue<QueuedLogEntry>,
    level: AtomicUsize,
    console: AtomicU8,
    collapse: AtomicBool,
    frame: AtomicU64,
    dropped: AtomicU64,
}

impl LoggerState {
    fn new(options: LogOptions) -> Self {
        Self {
            queue: ArrayQueue::new(options.capacity.max(1)),
            level: AtomicUsize::new(level_to_usize(options.level)),
            console: AtomicU8::new(console_to_u8(options.console)),
            collapse: AtomicBool::new(options.collapse),
            frame: AtomicU64::new(NO_FRAME),
            dropped: AtomicU64::new(0),
        }
    }

    fn configure(&self, options: LogOptions) {
        self.level
            .store(level_to_usize(options.level), Ordering::Release);
        self.console
            .store(console_to_u8(options.console), Ordering::Release);
        self.collapse.store(options.collapse, Ordering::Release);
    }

    fn level(&self) -> log::LevelFilter {
        level_from_usize(self.level.load(Ordering::Acquire))
    }

    #[cfg_attr(not(feature = "app"), allow(dead_code))]
    fn console(&self) -> LogConsole {
        console_from_u8(self.console.load(Ordering::Acquire))
    }

    #[cfg_attr(not(feature = "app"), allow(dead_code))]
    fn collapse(&self) -> bool {
        self.collapse.load(Ordering::Acquire)
    }

    #[cfg_attr(not(feature = "app"), allow(dead_code))]
    fn set_frame(&self, frame: Option<u64>) {
        self.frame
            .store(frame.unwrap_or(NO_FRAME), Ordering::Release);
    }

    fn frame(&self) -> Option<u64> {
        match self.frame.load(Ordering::Acquire) {
            NO_FRAME => None,
            frame => Some(frame),
        }
    }

    fn push(&self, entry: QueuedLogEntry) {
        match self.queue.push(entry) {
            Ok(()) => {}
            Err(entry) => {
                if self.queue.force_push(entry).is_some() {
                    self.dropped.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
    }

    #[cfg_attr(not(feature = "app"), allow(dead_code))]
    fn drain_into(&self, store: &LogStore) -> usize {
        let dropped = self.dropped.swap(0, Ordering::AcqRel);
        let mut queued = Vec::new();
        for _ in 0..self.queue.capacity() {
            let Some(entry) = self.queue.pop() else {
                break;
            };
            queued.push(entry);
        }

        let console = self.console();
        let inserted = store.ingest_queued(dropped, queued, self.collapse());
        for entry in &inserted {
            if console.allows(entry.level) {
                write_entry_to_stderr(entry);
            }
        }
        inserted.len()
    }
}

struct SkyLogger {
    state: Arc<LoggerState>,
}

impl log::Log for SkyLogger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        level_enabled(metadata.level(), self.state.level())
    }

    fn log(&self, record: &log::Record<'_>) {
        if !level_enabled(record.level(), self.state.level()) {
            return;
        }
        self.state
            .push(QueuedLogEntry::from_record(record, self.state.frame()));
    }

    fn flush(&self) {}
}

static LOGGER_STATE: OnceLock<Arc<LoggerState>> = OnceLock::new();
static SKY_LOGGER_INSTALLED: AtomicBool = AtomicBool::new(false);

#[cfg_attr(not(feature = "app"), allow(dead_code))]
pub(crate) fn try_install_logger(options: LogOptions) -> LogInstallStatus {
    let state = LOGGER_STATE
        .get_or_init(|| Arc::new(LoggerState::new(options)))
        .clone();
    state.configure(options);

    if SKY_LOGGER_INSTALLED.load(Ordering::Acquire) {
        log::set_max_level(options.level);
        return LogInstallStatus::Installed;
    }

    let logger = SkyLogger { state };
    match log::set_boxed_logger(Box::new(logger)) {
        Ok(()) => {
            SKY_LOGGER_INSTALLED.store(true, Ordering::Release);
            log::set_max_level(options.level);
            LogInstallStatus::Installed
        }
        Err(_) => LogInstallStatus::AlreadyInstalled,
    }
}

#[cfg_attr(not(feature = "app"), allow(dead_code))]
pub(crate) fn set_logger_frame(frame: Option<u64>) {
    if let Some(state) = LOGGER_STATE.get() {
        state.set_frame(frame);
    }
}

#[cfg_attr(not(feature = "app"), allow(dead_code))]
pub(crate) fn drain_logger(store: &LogStore) -> usize {
    let Some(state) = LOGGER_STATE.get() else {
        return 0;
    };
    state.drain_into(store)
}

fn level_enabled(level: log::Level, filter: log::LevelFilter) -> bool {
    match filter {
        log::LevelFilter::Off => false,
        log::LevelFilter::Error => level <= log::Level::Error,
        log::LevelFilter::Warn => level <= log::Level::Warn,
        log::LevelFilter::Info => level <= log::Level::Info,
        log::LevelFilter::Debug => level <= log::Level::Debug,
        log::LevelFilter::Trace => level <= log::Level::Trace,
    }
}

fn level_to_usize(level: log::LevelFilter) -> usize {
    match level {
        log::LevelFilter::Off => 0,
        log::LevelFilter::Error => 1,
        log::LevelFilter::Warn => 2,
        log::LevelFilter::Info => 3,
        log::LevelFilter::Debug => 4,
        log::LevelFilter::Trace => 5,
    }
}

fn level_from_usize(level: usize) -> log::LevelFilter {
    match level {
        0 => log::LevelFilter::Off,
        1 => log::LevelFilter::Error,
        2 => log::LevelFilter::Warn,
        3 => log::LevelFilter::Info,
        4 => log::LevelFilter::Debug,
        _ => log::LevelFilter::Trace,
    }
}

fn console_to_u8(console: LogConsole) -> u8 {
    match console {
        LogConsole::Off => 0,
        LogConsole::Errors => 1,
        LogConsole::WarningsAndErrors => 2,
        LogConsole::All => 3,
    }
}

#[cfg_attr(not(feature = "app"), allow(dead_code))]
fn console_from_u8(console: u8) -> LogConsole {
    match console {
        0 => LogConsole::Off,
        1 => LogConsole::Errors,
        2 => LogConsole::WarningsAndErrors,
        _ => LogConsole::All,
    }
}
