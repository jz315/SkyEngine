//! Shared scene descriptors and ECS components with no renderer-family policy.

mod camera;
pub(crate) mod color;
mod hierarchy;
pub(crate) mod indirect_lighting;
mod settings;

pub use camera::{Camera, CameraViewport, MainCamera};
pub use hierarchy::Parent;
pub use settings::{
    BloomSettings, ContactShadowsSettings, GlobalIllumination, RenderDebugView, RenderLayerMask,
    RenderSettings, SharpenSettings, TemporalAntiAliasingSettings, ToneMapSettings,
    VignetteSettings,
};
