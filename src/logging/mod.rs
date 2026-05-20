//! Lightweight app-facing log capture.
//!
//! SkyEngine uses the standard [`log`] facade for writes. This module provides
//! a small in-memory console store for app overlays, tools, and tests.

mod console;
mod entry;
mod logger;
mod options;
mod store;

pub use console::LogConsole;
pub use entry::{LogCursor, LogEntry};
pub use options::LogOptions;
pub use store::LogStore;

#[cfg(feature = "app")]
pub(crate) use logger::{drain_logger, set_logger_frame, try_install_logger};
