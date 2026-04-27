mod asset;
mod builtins;
mod contexts;
mod features;
mod passes;
mod phases;

pub(crate) use asset::MaterialRegistration;
pub use asset::{
    PipelineStep, PipelineStepDescriptor, RenderBackendKind, RenderPipelineAsset,
    RenderPipelineBuilder, RenderPipelineDescriptor,
};
pub use builtins::{
    Bloom, DdgiUpdateCompute, SceneMaterialPrepass, SceneNormalPrepass, ToneMap, Vignette,
};
pub use contexts::{
    ComputePassExecuteContext, ComputePassSetupContext, PostFxPassExecuteContext,
    PostFxPassSetupContext, RenderPassExecuteContext, RenderPassSetupContext,
    RenderPhaseExecuteContext, RenderPhaseSetupContext,
};
pub(crate) use features::AnyRenderFeature;
#[cfg(feature = "live2d")]
pub use features::Live2DFeature;
pub use features::{RenderFeature, SpriteFeature};
pub use passes::{ComputePass, PostFxPass, RenderPass};
pub use phases::RenderPhase;
