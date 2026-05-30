//! Expert rendering API.
//!
//! This namespace collects the lower-level render graph, frame execution, pass,
//! target, draw dispatch, and GPU resource APIs for renderer authors and engine
//! tools. Normal application code should start from `sky_engine::render`.
//!
//! Prefer this namespace when code needs to participate in renderer internals:
//! `FramePipeline`, `RenderGraph`, `PreparedFrame`, `PreparedView`, phase draw
//! contexts, draw functions, low-level meshes, GPU tables, render targets, and
//! direct texture readback. These APIs are public for advanced integration, but
//! they are not the primary gameplay compatibility surface.

pub use super::composite::CompositePass;
pub use super::execution::{
    CompletedViewState, ComputePassExecuteContext, ComputePassSetupContext,
    FinalizeExecutionContext, FinalizePhaseState, FrameExecutionStats, FrameFinalizeNode,
    FramePayloadStore, FramePipeline, FrameSetupNode, FrameViewNode, GraphPassExecuteContext,
    GraphPassSetupContext, PhaseDrawServices, PhaseExecuteContext, PhaseSetupContext, PhaseState,
    PostFxPassExecuteContext, PostFxPassSetupContext, PreparedFrame, PreparedView,
    RenderPassExecuteContext, RenderPassSetupContext, ResourceSlotMap, SceneTexture,
    SetupExecutionContext, SlotResource, TextureFormat, TextureSlot, ViewExecutionContext,
    ViewPayloadStore,
};
pub use super::extract::{
    ExtractContext, ExtractError, ExtractSchedule, ExtractSprites, Extractor,
};
pub use super::gpu::{
    compose_fullscreen_shader, is_depth_format, read_render_target, read_render_target_subresource,
    read_texture, read_texture_subresource, sampled_texture_entry, sampler_entry,
    storage_buffer_entry, storage_texture_entry, uniform_buffer_entry, ComputePipelineCache,
    FullscreenPass, FullscreenPipeline, GpuScene, GpuTable, GpuTableManager, ModelMatrixTable,
    RenderTarget, RenderTargetDescriptor, Texture, TextureCreateDesc, TextureError,
    TextureFileDesc, TextureReadback, TextureReadbackError, TextureUploadDesc,
    DEFAULT_DEPTH_FORMAT,
};
pub use super::graph::{
    AliasingStats, BufferBuilder, BufferHandle, ColorOutput, CompiledPass, CopyOp, CopyOpDebug,
    CopyPassSetup, DebugProfiler, DepthStencilOutput, ImportedTexture, LoadOp, PassFlags,
    PassHandle, PassSetup, PassType, PhysicalResourceViewStats, PhysicalResources,
    PhysicalTextureRef, QueueAssignmentDiagnostic, QueueDiagnosticClass, QueueScheduleBlocker,
    QueueScheduleDiagnostic, QueueScheduleReason, RenderGraph, RenderGraphAliasGroupDebug,
    RenderGraphAliasMemberDebug, RenderGraphAliasRedirectDebug, RenderGraphBufferResourceDebug,
    RenderGraphDebugDump, RenderGraphDotOptions, RenderGraphError, RenderGraphLifetimeDebug,
    RenderGraphPassDebug, RenderGraphProfiler, RenderGraphResourceDebug, RenderGraphResourceKind,
    RenderGraphTextureResourceDebug, ResourceRef, TargetSize, TextureBuilder, TextureHandle,
};
pub use super::lighting::{
    color_temperature, DirectionalShadowPhase, GpuLight, GpuLightKind, Light2D, LightPass,
    LightTable, SceneLightingResources,
};
pub use super::mesh::{MeshDraw, MeshPass, MeshPassError};
pub use super::phase::{
    entity_sort_key, opaque_sort_key, transparent_sort_key, DrawContext, DrawError, DrawFunction,
    DrawFunctionId, DrawFunctionRegistry, DrawMesh, DrawSprite, OpaquePhase, PhaseItem,
    TransparentPhase,
};
pub use super::pipeline::{GraphPass, TextureSpec};
pub use super::postfx::{
    bloom::{Bloom, BloomGraph},
    tonemap::ToneMap,
    vignette::Vignette,
    PostFx,
};
pub use super::resources::{
    atlas::{AtlasError, AtlasPacker, TextureAtlas, UvRect},
    blackboard::Blackboard,
    material::{
        AlphaMode, Material, MaterialBindingLayout, MaterialError, MaterialHandle,
        MaterialPipelineCache, MaterialPipelineDesc, MaterialRegistry, MaterialRenderState,
        PipelineCache, SceneBindingDesc, SceneBindingKind, ShaderSource, SpriteMaterial,
        StandardMaterial, UnlitMaterial,
    },
    mesh::{
        BoundingSphere, Mesh, MeshDescriptor, MeshError, MeshHandle, MeshIndexData, MeshRegistry,
        Ray, RayAabb, RayBlasNode, RayHit, RayMesh, RayTriangle, SubMesh, VertexAttribute,
        VertexLayout, VertexSemantic,
    },
};
pub use super::runtime::{HistoryTexture, HistoryTextureRequest, HistoryTextureSize};
pub use super::sprite::batch::SpriteBatch;
pub use super::sprite::Sprite;
pub use super::view::{
    Camera, Frustum, RenderView, SceneView, TemporalViewState, ViewUniform, ViewportRect,
};
pub use super::Color;
pub use crate::math::{Projection, Quat, Transform};

#[cfg(feature = "live2d")]
pub mod live2d {
    pub use super::super::live2d::{
        Live2DExpressionPlayer, Live2DLoadError, Live2DModel, Live2DModelResource, Live2DPhysics,
        Live2DPhysicsOptions, Live2DPose, Live2DRenderer, Live2DUserModel, PreparedLive2DFrame,
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
