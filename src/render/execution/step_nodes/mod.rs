use crate::gpu::GpuContext;
use crate::render::execution::{
    bind_current_as_scene_color, ensure_scene_texture, pass_first_write_texture,
    require_current_color, require_render_target, FinalizeExecutionContext, FinalizePhaseState,
    FrameFinalizeNode, FrameViewNode, PhaseState, PreparedFrame, PreparedView, SceneTexture,
    ViewExecutionContext,
};
use crate::render::gi::GiRuntime;
use crate::render::gpu::GpuScene;
use crate::render::gpu::Texture;
use crate::render::graph::{
    CompiledPass, LoadOp, PhysicalResources, RenderGraph, RenderGraphError, TargetSize,
};
use crate::render::lighting::shadow::{
    SceneShadowGraphResources, SceneShadowResources, ShadowSceneBindingLayout, ShadowViewBinding,
};
use crate::render::phase::{
    DrawContext, DrawFunctionRegistry, OpaquePhase, StandaloneDrawContext, TransparentPhase,
};
use crate::render::pipeline::{ComputePass, GraphPass, PostFxPass, RenderPass, RenderPhase};
use crate::render::resources::material::MaterialRegistry;
use crate::render::resources::mesh::MeshRegistry;
use crate::render::view::SceneView;
use crate::render::{
    ComputePassExecuteContext, ComputePassSetupContext, GraphPassExecuteContext,
    GraphPassSetupContext, PhaseDrawServices, PhaseExecuteContext, PhaseSetupContext,
    PostFxPassExecuteContext, PostFxPassSetupContext, RenderPassExecuteContext,
    RenderPassSetupContext,
};
use crate::render::{RenderSettings, DEFAULT_DEPTH_FORMAT};

mod compute_step_node;
mod finalize_pass_step_node;
mod graph_pass_step_node;
mod phase_step_node;
mod postfx_step_node;
mod render_services;
mod scene_step_nodes;

pub(crate) use compute_step_node::ComputeStepNode;
pub(crate) use finalize_pass_step_node::RenderPassStepNode;
pub(crate) use graph_pass_step_node::GraphPassStepNode;
pub(crate) use phase_step_node::PhaseStepNode;
pub(crate) use postfx_step_node::PostFxStepNode;
pub(crate) use render_services::{RenderServices, RuntimeRenderServices};
pub(crate) use scene_step_nodes::{HeadlessKeepAliveNode, SceneColorSeedNode};
