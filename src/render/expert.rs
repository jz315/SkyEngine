//! Expert rendering API.
//!
//! This namespace preserves the lower-level render graph / pass / target API for
//! advanced users who want direct control over GPU resources and execution.
//! `SpriteFramePipeline` also lives here as the opt-in sprite-domain adapter; it
//! is no longer part of the default high-level render story.

pub use super::core::{
    camera::{Camera2D, CameraUniform, RenderView, ViewUniform},
    color::Color,
    fullscreen::{compose_fullscreen_shader, FullscreenPass, FullscreenPipeline},
    target::{RenderTarget, RenderTargetDescriptor},
    texture::{Texture, TextureCreateDesc, TextureError, TextureFileDesc, TextureUploadDesc},
    viewport::ViewportRect,
};
pub use super::domains::sprite::GpuScene2D;
pub use super::domains::sprite::{
    SpriteCompositeNode, SpriteDomainExecuteContext, SpriteDomainFeature, SpriteDomainSetupContext,
    SpriteDomainStage, SpriteFramePipeline, SpriteLightNode, SpriteSceneNode,
};
pub use super::frame_pipeline::{
    CompletedViewState, FinalizeExecutionContext, FinalizePhaseState, FrameExecutionStats,
    FrameFinalizeNode, FramePayloadStore, FramePipeline, FrameSetupNode, FrameViewNode, PhaseState,
    PreparedFrame, PreparedView, ResourceSlotMap, SetupExecutionContext, SlotResource,
    TextureFormat, TextureSlot, ViewExecutionContext, ViewPayloadStore,
};
pub use super::graph::{
    AliasingStats, BufferBuilder, BufferHandle, ColorOutput, CompiledPass, CopyOp, CopyPassSetup,
    DebugProfiler, DepthStencilOutput, ImportedTexture, LoadOp, PassFlags, PassHandle, PassSetup,
    PassType, PhysicalResources, PhysicalTextureRef, RenderGraph, RenderGraphError,
    RenderGraphProfiler, ResourceRef, TargetSize, TextureBuilder, TextureHandle,
};
pub use super::light::{color_temperature, Light2D};
pub use super::output_chain::{BloomNode, ToneMapNode, ViewportBlitNode, VignetteNode};
pub use super::passes::{
    batch::{Sprite, SpriteBatch},
    composite_pass::CompositePass,
    light_pass::LightPass,
    mesh_pass::{MeshDraw, MeshPass, MeshPassError},
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
