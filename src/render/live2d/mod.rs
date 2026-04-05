//! Live2D Cubism rendering module for SkyEngine.
//!
//! Provides loading and GPU rendering of Live2D models (`.model3.json`)
//! using the Cubism SDK Core via FFI and wgpu for GPU operations.

pub mod clipping;
mod expression;
pub mod loader;
pub mod model;
mod motion;
mod physics;
pub mod pose;
pub mod renderer;
mod runtime;

pub use expression::Live2DExpressionPlayer;
pub use loader::Live2DModelResource;
pub use model::Live2DModel;
pub use physics::Live2DPhysics;
pub use pose::Live2DPose;
pub use renderer::Live2DRenderer;
