//! Expert rendering API.
//!
//! This namespace preserves the lower-level render graph / pass / target API for
//! advanced users who want direct control over GPU resources and execution.

pub use super::core::{
    camera::{Camera2D, CameraUniform, RenderView, ViewUniform},
    color::Color,
    fullscreen::{compose_fullscreen_shader, FullscreenPass, FullscreenPipeline},
    target::{RenderTarget, RenderTargetDescriptor},
    texture::{Texture, TextureCreateDesc, TextureError, TextureFileDesc, TextureUploadDesc},
};
pub use super::gpu_scene2d::GpuScene2D;
pub use super::graph::{
    AliasingStats, BufferBuilder, BufferHandle, ColorOutput, CompiledPass, CopyOp, CopyPassSetup,
    DebugProfiler, DepthStencilOutput, ImportedTexture, LoadOp, PassFlags, PassHandle, PassSetup,
    PassType, PhysicalResources, PhysicalTextureRef, RenderGraph, RenderGraphError,
    RenderGraphProfiler, ResourceRef, TargetSize, TextureBuilder, TextureHandle,
};
pub use super::light::{color_temperature, Light2D};
pub use super::passes::{
    batch::{Sprite, SpriteBatch},
    composite_pass::CompositePass,
    light_pass::LightPass,
    mesh_pass::{MeshDraw, MeshPass, MeshPassError},
};
pub use super::pipeline::{
    BloomNode, CompositeNode, FramePayloads2D, LightNode, PipelineState2D, RenderFeature2D,
    RenderPipeline, SpritePass, ToneMapNode, ViewportBlitNode, VignetteNode,
};
pub use super::postfx::{bloom::Bloom, tonemap::ToneMap, vignette::Vignette, PostFx};
pub use super::resources::{
    atlas::{AtlasError, AtlasPacker, TextureAtlas, UvRect},
    blackboard::Blackboard,
    material::{
        MaterialBindingLayout, MaterialError, MaterialInstance, MaterialPipelineCache,
        MaterialPipelineDesc, MaterialProperties, MaterialResourceBindings, PropertyType,
    },
    mesh::{Mesh, MeshError, MeshIndexData},
};

#[cfg(feature = "live2d")]
pub mod live2d {
    pub use super::super::live2d::{
        Live2DExpressionPlayer, Live2DLoadError, Live2DModel, Live2DModelResource,
        Live2DOverlayNode, Live2DPhysics, Live2DPhysicsOptions, Live2DPose, Live2DRenderer,
        Live2DUserModel, PreparedLive2DFrame, PreparedLive2DFrameSet,
    };

    pub mod asset {
        pub use super::super::super::live2d::asset::*;
    }

    pub mod model {
        pub use super::super::super::live2d::model::*;
    }

    pub mod render {
        pub use super::super::super::live2d::render::*;
    }

    pub mod runtime {
        pub use super::super::super::live2d::runtime::*;
    }
}
