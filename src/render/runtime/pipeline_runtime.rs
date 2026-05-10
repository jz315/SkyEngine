use crate::gpu::GpuContext;
use crate::render::execution::{
    ComputeStepNode, FramePipeline, GraphPassStepNode, HeadlessKeepAliveNode, PhaseStepNode,
    PostFxStepNode, RenderPassStepNode, RuntimeRenderServices, SceneColorSeedNode,
};
use crate::render::pipeline::PipelineStep;
use crate::render::runtime::ViewportBlitNode;
use crate::render::view::SCENE_HDR_FORMAT;
pub(crate) type RuntimeFramePipeline<'a> = FramePipeline<'a, dyn RuntimeRenderServices + 'a>;

pub(crate) fn build_runtime_pipeline<'a>(
    steps: &'a mut [PipelineStep],
    gpu: &GpuContext,
) -> RuntimeFramePipeline<'a> {
    let mut pipeline = FramePipeline::new();
    let seed_format = if steps.iter().any(|step| {
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

    for step in steps {
        match step {
            PipelineStep::Phase(phase) => {
                pipeline.add_view_node(Box::new(PhaseStepNode::new(phase.as_mut())));
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
