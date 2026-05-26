use crate::gpu::GpuContext;
use crate::render::execution::{
    ComputeStepNode, FramePipeline, FrameViewNode, GraphPassStepNode, HeadlessKeepAliveNode,
    PhaseState, PhaseStepNode, PostFxStepNode, PreparedFrame, PreparedView, RenderPassStepNode,
    RuntimeRenderServices, SceneColorSeedNode, ViewExecutionContext,
};
use crate::render::graph::{CompiledPass, PhysicalResources, RenderGraph, RenderGraphError};
use crate::render::pipeline::PipelineStep;
use crate::render::runtime::ViewportBlitNode;
use crate::render::view::SCENE_HDR_FORMAT;
pub(crate) type RuntimeFramePipeline<'a> = FramePipeline<'a, dyn RuntimeRenderServices + 'a>;

pub(crate) fn build_runtime_pipeline<'a>(
    steps: &'a mut [PipelineStep],
    gpu: &GpuContext,
    viewport_blit: &'a mut ViewportBlitNode,
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
    pipeline.add_view_node(Box::new(BorrowedViewportBlitNode::new(viewport_blit)));
    pipeline
}

struct BorrowedViewportBlitNode<'a> {
    inner: &'a mut ViewportBlitNode,
}

impl<'a> BorrowedViewportBlitNode<'a> {
    fn new(inner: &'a mut ViewportBlitNode) -> Self {
        Self { inner }
    }
}

impl FrameViewNode<dyn RuntimeRenderServices + '_> for BorrowedViewportBlitNode<'_> {
    fn name(&self) -> &'static str {
        <ViewportBlitNode as FrameViewNode<dyn RuntimeRenderServices>>::name(self.inner)
    }

    fn is_enabled(&self, frame: &PreparedFrame<'_>) -> bool {
        <ViewportBlitNode as FrameViewNode<dyn RuntimeRenderServices>>::is_enabled(
            self.inner, frame,
        )
    }

    fn is_view_enabled(&self, frame: &PreparedFrame<'_>, view: &PreparedView<'_>) -> bool {
        <ViewportBlitNode as FrameViewNode<dyn RuntimeRenderServices>>::is_view_enabled(
            self.inner, frame, view,
        )
    }

    fn setup(
        &mut self,
        graph: &mut RenderGraph,
        state: &mut PhaseState,
        frame: &PreparedFrame<'_>,
        view: &PreparedView<'_>,
    ) {
        <ViewportBlitNode as FrameViewNode<dyn RuntimeRenderServices>>::setup(
            self.inner, graph, state, frame, view,
        );
    }

    fn execute(
        &mut self,
        pass: &CompiledPass,
        ctx: &mut GpuContext,
        resources: &PhysicalResources<'_>,
        execution: &ViewExecutionContext<'_>,
        services: &mut (dyn RuntimeRenderServices + '_),
    ) -> Result<(), RenderGraphError> {
        <ViewportBlitNode as FrameViewNode<dyn RuntimeRenderServices>>::execute(
            self.inner, pass, ctx, resources, execution, services,
        )
    }

    fn draw_calls(
        &self,
        execution: &ViewExecutionContext<'_>,
        services: &(dyn RuntimeRenderServices + '_),
    ) -> usize {
        <ViewportBlitNode as FrameViewNode<dyn RuntimeRenderServices>>::draw_calls(
            self.inner, execution, services,
        )
    }
}
