mod asset;
mod builtins;
mod contexts;
mod features;
mod passes;
mod phases;
mod resource_spec;

pub(crate) use asset::MaterialRegistration;
pub use asset::{
    KajiyaDpiMode, KajiyaRendererSettings, PipelineStep, PipelineStepDescriptor, RenderBackendKind,
    RenderPipelineAsset, RenderPipelineBuilder, RenderPipelineDescriptor,
};
pub use builtins::{
    Bloom, ContactShadows, DebugView, GiCompositePass, GiUpdateCompute, SceneMaterialPrepass,
    SceneNormalPrepass, Sharpen, TemporalAntiAliasing, ToneMap, Vignette,
};
pub use contexts::{
    ComputePassExecuteContext, ComputePassSetupContext, GraphPassExecuteContext,
    GraphPassSetupContext, PostFxPassExecuteContext, PostFxPassSetupContext,
    RenderPassExecuteContext, RenderPassSetupContext, RenderPhaseExecuteContext,
    RenderPhaseSetupContext,
};
pub(crate) use features::AnyRenderFeature;
#[cfg(feature = "live2d")]
pub use features::Live2DFeature;
pub use features::{RenderFeature, SpriteFeature};
pub use passes::{ComputePass, GraphPass, PostFxPass, RenderPass};
pub use phases::RenderPhase;
pub use resource_spec::TextureSpec;
