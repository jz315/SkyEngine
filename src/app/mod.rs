//! SkyEngine application framework.
//!
//! Provides windowing, input, and the main loop that ties the GPU backend
//! to the ECS world.

pub(crate) mod asset_diagnostics;
pub mod config;
mod frame;
pub(crate) mod input;
mod lifecycle;
#[cfg(any(feature = "audio", feature = "video"))]
pub(crate) mod media_diagnostics;
pub(crate) mod pacing;
pub mod platform;
mod plugins;
pub(crate) mod render_diagnostics;
pub mod runner;
pub(crate) mod screenshots;
pub(crate) mod services;
pub mod windows;

#[cfg(feature = "egui")]
pub(crate) mod egui_integration;

pub use crate::diagnostics::{
    DiagnosticCursor, DiagnosticEvent, DiagnosticField, DiagnosticSeverity, Diagnostics,
};
pub use crate::logging::{LogConsole, LogCursor, LogEntry, LogOptions, LogStore};
pub use crate::math::{LogicalDelta, LogicalPoint, LogicalSize, PhysicalSize};
pub use config::{RedrawMode, RunnerOptions, WindowOptions, WindowSizeMode};
#[cfg(feature = "ui-core")]
pub use frame::UiFrame;
pub use frame::{FrameContext, Windows};
#[cfg(feature = "audio")]
pub use plugins::AudioPlugin;
#[cfg(feature = "video")]
pub use plugins::VideoPlugin;
pub use plugins::{AssetPlugin, InputPlugin, LogPlugin, RenderPlugin, RunnerPlugin, WindowPlugin};
pub use runner::{App, AppState, SetupContext};

/// Re-export the egui crate for user convenience.
///
/// ```rust,ignore
/// use sky_engine::app::egui;
/// ```
#[cfg(feature = "egui")]
pub use egui;
