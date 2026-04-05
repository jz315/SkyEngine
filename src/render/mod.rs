//! SkyEngine 2D rendering framework.

pub mod core;
pub mod graph;
pub mod light;
pub mod passes;
pub mod postfx;
pub mod resources;

#[cfg(feature = "live2d")]
pub mod live2d;

pub use core::{
    camera::{Camera2D, CameraUniform, RenderView, ViewUniform},
    color::Color,
    fullscreen::{compose_fullscreen_shader, FullscreenPass, FullscreenPipeline},
    target::{RenderTarget, RenderTargetDescriptor},
    texture::{Texture, TextureCreateDesc, TextureError, TextureFileDesc, TextureUploadDesc},
};
pub use graph::{
    AliasingStats, BufferBuilder, BufferHandle, ColorOutput, CompiledPass, CopyOp, CopyPassSetup,
    DebugProfiler, DepthStencilOutput, ImportedTexture, LoadOp, PassFlags, PassHandle, PassSetup,
    PassType, PhysicalResources, PhysicalTextureRef, RenderGraph, RenderGraphError,
    RenderGraphProfiler, ResourceRef, TargetSize, TextureBuilder, TextureHandle,
};
pub use light::{color_temperature, Light2D};
pub use passes::{
    batch::{Sprite, SpriteBatch},
    composite_pass::CompositePass,
    light_pass::LightPass,
    mesh_pass::{MeshDraw, MeshPass, MeshPassError},
};
pub use postfx::{bloom::Bloom, tonemap::ToneMap, vignette::Vignette, PostFx};
pub use resources::{
    atlas::{AtlasError, AtlasPacker, TextureAtlas, UvRect},
    blackboard::Blackboard,
    material::{
        MaterialBindingLayout, MaterialError, MaterialInstance, MaterialPipelineCache,
        MaterialPipelineDesc, MaterialProperties, MaterialResourceBindings, PropertyType,
    },
    mesh::{Mesh, MeshError, MeshIndexData},
};
