use super::*;

pub(crate) struct ComputeStepNode<'a> {
    compute: &'a mut dyn ComputePass,
}

impl<'a> ComputeStepNode<'a> {
    #[inline]
    pub(crate) fn new(compute: &'a mut dyn ComputePass) -> Self {
        Self { compute }
    }
}

impl FrameViewNode<dyn RuntimeRenderServices + '_> for ComputeStepNode<'_> {
    fn name(&self) -> &'static str {
        self.compute.name()
    }

    fn setup(
        &mut self,
        graph: &mut RenderGraph,
        state: &mut PhaseState,
        frame: &PreparedFrame<'_>,
        view: &PreparedView<'_>,
    ) {
        let mut context = ComputePassSetupContext::new(graph, state, frame, view);
        self.compute.setup(&mut context);
    }

    fn execute(
        &mut self,
        pass: &CompiledPass,
        ctx: &mut GpuContext,
        resources: &PhysicalResources<'_>,
        execution: &ViewExecutionContext<'_>,
        _services: &mut (dyn RuntimeRenderServices + '_),
    ) -> Result<(), RenderGraphError> {
        let mut context = ComputePassExecuteContext::new(ctx, pass, resources, execution);
        self.compute.execute(&mut context)?;
        Ok(())
    }
}
