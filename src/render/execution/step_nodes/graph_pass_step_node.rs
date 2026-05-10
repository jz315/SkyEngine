use super::*;

pub(crate) struct GraphPassStepNode<'a> {
    pass: &'a mut dyn GraphPass,
}

impl<'a> GraphPassStepNode<'a> {
    #[inline]
    pub(crate) fn new(pass: &'a mut dyn GraphPass) -> Self {
        Self { pass }
    }
}

impl FrameViewNode<dyn RuntimeRenderServices + '_> for GraphPassStepNode<'_> {
    fn name(&self) -> &'static str {
        self.pass.name()
    }

    fn is_enabled(&self, frame: &PreparedFrame<'_>) -> bool {
        frame
            .views()
            .iter()
            .any(|view| self.pass.is_enabled(frame, view))
    }

    fn is_view_enabled(&self, frame: &PreparedFrame<'_>, view: &PreparedView<'_>) -> bool {
        self.pass.is_enabled(frame, view)
    }

    fn setup(
        &mut self,
        graph: &mut RenderGraph,
        state: &mut PhaseState,
        frame: &PreparedFrame<'_>,
        view: &PreparedView<'_>,
    ) {
        let mut context = GraphPassSetupContext::new(graph, state, frame, view);
        self.pass.setup(&mut context);
    }

    fn execute(
        &mut self,
        pass: &CompiledPass,
        ctx: &mut GpuContext,
        resources: &PhysicalResources<'_>,
        execution: &ViewExecutionContext<'_>,
        _services: &mut (dyn RuntimeRenderServices + '_),
    ) -> Result<(), RenderGraphError> {
        if !self.is_view_enabled(execution.frame(), execution.view()) {
            return Ok(());
        }
        let mut context = GraphPassExecuteContext::new(ctx, pass, resources, execution);
        self.pass.execute(&mut context)?;
        Ok(())
    }

    fn draw_calls(
        &self,
        execution: &ViewExecutionContext<'_>,
        _services: &(dyn RuntimeRenderServices + '_),
    ) -> usize {
        if !self.is_view_enabled(execution.frame(), execution.view()) {
            return 0;
        }
        self.pass.draw_calls(execution)
    }

    fn resize(&mut self, ctx: &GpuContext, width: u32, height: u32) {
        self.pass.resize(ctx, width, height);
    }
}
