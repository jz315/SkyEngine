//! SkyEngine application framework.
//!
//! Provides windowing, input, and the main loop that ties the GPU backend
//! to the ECS world.

pub mod config;
pub mod input;
pub mod runner;

pub use config::AppConfig;
pub use input::{Input, KeyCode};
pub use runner::{App, AppLifecycle, FrameContext};
