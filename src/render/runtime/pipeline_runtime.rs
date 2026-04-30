use crate::gpu::GpuContext;
use crate::render::execution::FramePipeline;
use crate::render::pipeline::PipelineStep;
use crate::render::runtime::ViewportBlitNode;
use crate::render::view::SCENE_HDR_FORMAT;

use super::composer::RenderComposer;
use super::nodes::{
    ComputeStepNode, GraphPassStepNode, HeadlessKeepAliveNode, PhaseStepNode, PostFxStepNode,
    RenderPassStepNode, SceneColorSeedNode,
};

pub(crate) fn build_runtime_pipeline(
    composer: &mut RenderComposer,
    gpu: &GpuContext,
) -> FramePipeline {
    let mut pipeline = FramePipeline::new();
    let seed_format = if composer.plan.steps.iter().any(|step| {
        matches!(
            step,
            PipelineStep::PostFx(fx) if fx.requires_hdr_input()
        )
    }) {
        SCENE_HDR_FORMAT
    } else {
        gpu.surface_format()
    };
    pipeline.add_view_node(Box::new(SceneColorSeedNode::new(seed_format)));

    let fallback_texture =
        composer.runtime.fallback_texture.as_ref().expect(
            "phase runtime should initialize fallback texture before building the pipeline",
        );
    let draw_functions = &mut composer.resources.draw_functions as *mut _;
    let materials = &mut composer.resources.material_registry as *mut _;

    for step in &mut composer.plan.steps {
        match step {
            PipelineStep::Phase(phase) => {
                pipeline.add_view_node(Box::new(PhaseStepNode::new(
                    phase.as_mut(),
                    unsafe { &mut *draw_functions },
                    unsafe { &mut *materials },
                    &composer.resources.mesh_registry,
                    fallback_texture,
                )));
            }
            PipelineStep::Compute(compute) => {
                pipeline.add_view_node(Box::new(ComputeStepNode::new(compute.as_mut())));
            }
            PipelineStep::Graph(pass) => {
                pipeline.add_view_node(Box::new(GraphPassStepNode::new(pass.as_mut())));
            }
            PipelineStep::Pass(pass) => {
                pipeline.add_finalize_node(Box::new(RenderPassStepNode::new(pass.as_mut())));
            }
            PipelineStep::PostFx(fx) => {
                pipeline.add_view_node(Box::new(PostFxStepNode::new(fx.as_mut())));
            }
        }
    }

    pipeline.add_view_node(Box::new(HeadlessKeepAliveNode));
    pipeline.add_view_node(Box::new(ViewportBlitNode::new(gpu)));
    pipeline
}
