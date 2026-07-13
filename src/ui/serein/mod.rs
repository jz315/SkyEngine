//! Serein declarative game UI integration.
//!
//! This module adapts Serein's authoring and runtime model into SkyEngine:
//! small core elements, stable IDs, declarative builders, runtime-owned layout,
//! interaction, and animation state. Rendering and platform input are adapted
//! to SkyEngine through the standalone `serein-wgpu` and `serein-winit` crates.

mod api;
mod backend;
mod config;
mod font_provider;
mod image_provider;
mod input_bridge;
mod plugin;
mod renderer;
mod window;

pub use api::{compose, open_window, register_skin};
pub use backend::SereinUiBackend;
pub use config::{IntoSereinClearColor, SereinUiConfig, SereinWindowConfig};
pub use plugin::{install_serein_ui_backend, SereinUiPlugin};
pub use serein::expert;
pub use serein::prelude::*;
pub use serein::testing::{TargetPoint, UiActionTrace, UiTestDriver, UiTestError};
pub use serein::{apply_ease, has_anim_property};

/// Convert standalone Serein UI colors into SkyEngine render colors at the
/// integration boundary.
#[inline]
pub fn to_render_color(value: serein::Color) -> crate::render::Color {
    crate::render::Color::new(value.r, value.g, value.b, value.a)
}

/// Convert SkyEngine render colors into standalone Serein UI colors.
#[inline]
pub fn from_render_color(value: crate::render::Color) -> serein::Color {
    serein::Color::new(value.r, value.g, value.b, value.a)
}
