//! Render-graph construction, compilation, diagnostics, and physical resources.

pub use crate::render::graph::{
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
