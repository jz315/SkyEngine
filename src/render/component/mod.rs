//! ECS-facing rendering components and renderer settings.

mod camera;
mod hierarchy;
mod light;
mod mesh;
mod settings;
mod sprite;
mod tilemap;

#[cfg(feature = "live2d")]
mod live2d;

pub use crate::math::Transform;
pub use camera::{Camera, CameraViewport, MainCamera};
pub use hierarchy::Parent;
pub use light::{
    DirectionalLight, PointLight, ShadowSamplingMode, ShadowUpdatePolicy, SpotLight,
    MAX_DIRECTIONAL_SHADOW_CASCADES,
};
#[cfg(feature = "live2d")]
pub use live2d::{Live2DAnimator, Live2DCommand, Live2DCommands, Live2DModelInstance};
pub use mesh::{MeshRenderer, WgpuMeshRenderer, ALL_SHADOW_CASCADE_MASK};
pub use settings::{
    BloomSettings, ContactShadowsSettings, GlobalIllumination, RenderDebugView, RenderLayerMask,
    RenderSettings, SharpenSettings, TemporalAntiAliasingSettings, ToneMapSettings,
    VignetteSettings,
};
pub use sprite::{SortingLayer, SpriteRenderer};
pub use tilemap::{
    TileAnimation, TileAnimationFrame, TilemapDepthSort, TilemapOrientation, TilemapRenderOrder,
    TilemapRenderer, TilemapStaggerAxis, TilemapStaggerIndex, TilesetGrid, TilesetTileRect,
};

pub use crate::render::animation::{SpriteAnimationClip, SpriteAnimationFrame, SpriteAnimator};
