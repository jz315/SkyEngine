//! Experimental EUI-NEO-inspired declarative game UI.
//!
//! This module ports the EUI-NEO authoring and runtime model into SkyEngine:
//! small core elements, stable IDs, declarative builders, runtime-owned layout,
//! interaction, and animation state. Rendering and platform input are adapted
//! to SkyEngine instead of copying EUI-NEO's OpenGL/GLFW backend.

mod api;
mod backend;
mod config;
mod font_provider;
mod image_provider;
mod input_bridge;
mod plugin;
mod renderer;
mod window;

/// Re-export of the standalone `eui-neo` core crate.
pub mod eui {
    pub use eui_neo::*;
}

pub use api::{compose, open_window, register_skin};
pub use backend::NeoUiBackend;
pub use config::{IntoNeoClearColor, NeoUiConfig, NeoWindowConfig};
pub use eui_neo::expert;
pub use eui_neo::prelude::*;
pub use eui_neo::{apply_ease, has_anim_property};
pub use plugin::{install_neo_ui_backend, NeoUiPlugin};

/// Convert standalone Neo UI colors into SkyEngine render colors at the
/// integration boundary.
#[inline]
pub fn to_render_color(value: eui_neo::Color) -> crate::render::Color {
    crate::render::Color::new(value.r, value.g, value.b, value.a)
}

/// Convert SkyEngine render colors into standalone Neo UI colors.
#[inline]
pub fn from_render_color(value: crate::render::Color) -> eui_neo::Color {
    eui_neo::Color::new(value.r, value.g, value.b, value.a)
}
