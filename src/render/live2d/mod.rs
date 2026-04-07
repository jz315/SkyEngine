//! Live2D Cubism rendering module for SkyEngine.
//!
//! Provides loading and GPU rendering of Live2D models (`.model3.json`)
//! using the Cubism SDK Core via FFI and wgpu for GPU operations.

pub mod asset;
pub mod model;
pub mod render;
pub mod runtime;

#[allow(unused_imports)]
pub use asset::{
    Live2DDisplayInfo, Live2DDisplayNamedEntry, Live2DHitArea, Live2DLoadError,
    Live2DModelResource, Live2DUserDataEntry,
};
pub use model::Live2DModel;
#[allow(unused_imports)]
pub use render::{Live2DOverlayNode, Live2DRenderer, PreparedLive2DFrame, PreparedLive2DFrameSet};
#[allow(unused_imports)]
pub use runtime::{
    Live2DBreath, Live2DExpressionPlayer, Live2DEyeBlink, Live2DLipSync, Live2DLook, Live2DPhysics,
    Live2DPhysicsOptions, Live2DPose, Live2DUserModel, MotionFinishedEvent, MotionFiredEvent,
    MotionHandle, MotionPriority, MotionStartedEvent, INVALID_MOTION_HANDLE,
};
