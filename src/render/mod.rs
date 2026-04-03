//! SkyEngine 2D rendering framework.

pub mod atlas;
pub mod batch;
pub mod blackboard;
pub mod camera;
pub mod color;
pub mod composite_pass;
pub mod fullscreen;
pub mod graph;
pub mod light;
pub mod light_pass;
pub mod material;
pub mod postfx;
pub mod target;
pub mod texture;

pub use atlas::{AtlasPacker, TextureAtlas, UvRect};
pub use batch::{Sprite, SpriteBatch};
pub use blackboard::Blackboard;
pub use camera::{Camera2D, CameraUniform};
pub use color::Color;
pub use composite_pass::CompositePass;
pub use fullscreen::{compose_fullscreen_shader, FullscreenPass, FullscreenPipeline};
pub use graph::{
    BufferBuilder, BufferHandle, ColorOutput, CompiledPass, CopyOp, CopyPassSetup, DebugProfiler,
    DepthStencilOutput, ImportedTexture, LoadOp, PassFlags, PassHandle, PassSetup, PassType,
    PhysicalResources, PhysicalTextureRef, RenderGraph, RenderGraphError, RenderGraphProfiler,
    ResourceRef, TargetSize, TextureBuilder, TextureHandle,
};
pub use light::{color_temperature, Light2D};
pub use light_pass::LightPass;
pub use material::{
    MaterialBindingLayout, MaterialInstance, MaterialPipelineCache, MaterialPipelineDesc,
    MaterialProperties, MaterialResourceBindings, PropertyType,
};
pub use postfx::{bloom::Bloom, tonemap::ToneMap, vignette::Vignette, PostFx};

pub use target::RenderTarget;
pub use texture::Texture;
