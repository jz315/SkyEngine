//! Live2D Cubism rendering module for SkyEngine.
//!
//! Provides loading and GPU rendering of Live2D models (`.model3.json`)
//! using the Cubism SDK Core via FFI and wgpu for GPU operations.

mod backend;
mod component;
mod draw;
mod feature;

pub mod asset;
pub mod model;
pub mod render;
pub mod runtime;

#[allow(unused_imports)]
pub use asset::{
    Live2DDisplayInfo, Live2DDisplayNamedEntry, Live2DHitArea, Live2DLoadError,
    Live2DModelResource, Live2DUserDataEntry,
};
pub(crate) use backend::Live2DPhaseRenderer;
pub use component::{
    Live2DAnimator, Live2DCommand, Live2DCommands, Live2DLookTarget, Live2DModelInstance,
    Live2DModelPoint,
};
pub(crate) use draw::DrawLive2D;
pub use feature::Live2DFeature;
#[allow(unused_imports)]
pub(crate) use feature::{
    live2d_instance_visible_in_view, sort_live2d_scene_instances, Live2DSceneInstance,
    PreparedLive2DPhaseView,
};
pub use model::Live2DModel;
#[allow(unused_imports)]
pub use render::{Live2DRenderer, PreparedLive2DFrame};
#[allow(unused_imports)]
pub use runtime::{
    Live2DBreath, Live2DExpressionPlayer, Live2DEyeBlink, Live2DLipSync, Live2DLook, Live2DPhysics,
    Live2DPhysicsOptions, Live2DPose, Live2DUpdateTimings, Live2DUserModel, MotionFinishedEvent,
    MotionFiredEvent, MotionHandle, MotionPriority, MotionStartedEvent, INVALID_MOTION_HANDLE,
};
