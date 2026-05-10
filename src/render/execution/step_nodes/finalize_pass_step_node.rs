use super::*;

pub(crate) struct RenderPassStepNode<'a> {
    pass: &'a mut dyn RenderPass,
}

impl<'a> RenderPassStepNode<'a> {
    #[inline]
    pub(crate) fn new(pass: &'a mut dyn RenderPass) -> Self {
        Self { pass }
    }
}

impl FrameFinalizeNode for RenderPassStepNode<'_> {
    fn name(&self) -> &'static str {
        self.pass.name()
    }

    fn setup(
        &mut self,
        graph: &mut RenderGraph,
        state: &mut FinalizePhaseState<'_>,
        frame: &PreparedFrame<'_>,
    ) {
        let mut context = RenderPassSetupContext::new(graph, state, frame);
        self.pass.setup(&mut context);
    }

    fn execute(
        &mut self,
        pass: &CompiledPass,
        ctx: &mut GpuContext,
        resources: &PhysicalResources<'_>,
        execution: &FinalizeExecutionContext<'_>,
    ) -> Result<(), RenderGraphError> {
        let mut context = RenderPassExecuteContext::new(ctx, pass, resources, execution);
        self.pass.execute(&mut context)?;
        Ok(())
    }
}
