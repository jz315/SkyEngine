//! SkyEngine application framework.
//!
//! Provides windowing, input, and the main loop that ties the GPU backend
//! to the ECS world.

pub mod config;
pub(crate) mod diagnostic_console;
mod frame;
pub(crate) mod input;
mod lifecycle;
pub(crate) mod pacing;
pub mod platform;
pub mod runner;
pub(crate) mod screenshots;
pub(crate) mod services;
pub mod windows;

#[cfg(feature = "egui")]
pub(crate) mod egui_integration;

pub use crate::diagnostics::DiagnosticConsole;
pub use config::{AppConfig, RedrawMode};
#[cfg(feature = "ui-core")]
pub use frame::UiFrame;
pub use frame::{FrameContext, Windows};
pub use runner::{App, AppState, SetupContext};

/// Re-export the egui crate for user convenience.
///
/// ```rust,ignore
/// use sky_engine::app::egui;
/// ```
#[cfg(feature = "egui")]
pub use egui;
