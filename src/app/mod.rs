//! SkyEngine application framework.
//!
//! Provides windowing, input, and the main loop that ties the GPU backend
//! to the ECS world.

pub mod config;
pub mod input;
pub mod runner;

#[cfg(feature = "egui")]
pub(crate) mod egui_integration;

pub use config::{AppConfig, RedrawMode};
pub use input::{Input, KeyCode};
pub use runner::{App, AppState, FrameContext};

/// Re-export the egui crate for user convenience.
///
/// ```rust,ignore
/// use sky_engine::app::egui;
/// ```
#[cfg(feature = "egui")]
pub use ::egui;
