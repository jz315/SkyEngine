use std::any::Any;

use crate::gpu::GpuContext;
use crate::render::execution::{
    CompletedViewState, FinalizeExecutionContext, FinalizePhaseState, PhaseState, PreparedFrame,
    PreparedView, SceneTexture, TextureFormat, TextureSlot, ViewExecutionContext,
};
use crate::render::gpu::{GpuScene, Texture};
use crate::render::graph::{
    CompiledPass, PhysicalResources, RenderGraph, ResourceRef, TargetSize, TextureHandle,
    TextureSubresource,
};
use crate::render::phase::DrawFunctionRegistry;
use crate::render::resources::blackboard::Blackboard;
use crate::render::resources::material::MaterialRegistry;
use crate::render::resources::mesh::MeshRegistry;
use crate::render::resources::{LightTable, SceneLightingResources, SceneShadowResources};
use crate::render::runtime::{HistoryTextureRequest, HistoryTextureStore};
use crate::render::view::SceneView;
use crate::render::RenderSettings;

use crate::render::pipeline::TextureSpec;

mod compute;
mod graph;
mod pass;
mod phase;
mod postfx;
mod shared;

#[cfg(test)]
mod tests;

pub use compute::{ComputePassExecuteContext, ComputePassSetupContext};
pub use graph::{GraphPassExecuteContext, GraphPassSetupContext};
pub use pass::{RenderPassExecuteContext, RenderPassSetupContext};
pub use phase::{PhaseDrawServices, PhaseExecuteContext, PhaseSetupContext};
pub use postfx::{PostFxPassExecuteContext, PostFxPassSetupContext};

use shared::*;
