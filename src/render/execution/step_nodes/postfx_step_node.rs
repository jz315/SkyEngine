use super::*;

pub(crate) struct PostFxStepNode<'a> {
    fx: &'a mut dyn PostFxPass,
}

impl<'a> PostFxStepNode<'a> {
    #[inline]
    pub(crate) fn new(fx: &'a mut dyn PostFxPass) -> Self {
        Self { fx }
    }
}

impl FrameViewNode<dyn RuntimeRenderServices + '_> for PostFxStepNode<'_> {
    fn name(&self) -> &'static str {
        self.fx.name()
    }

    fn is_enabled(&self, frame: &PreparedFrame<'_>) -> bool {
        frame
            .views()
            .iter()
            .any(|view| self.fx.is_enabled(frame, view))
    }

    fn is_view_enabled(&self, frame: &PreparedFrame<'_>, view: &PreparedView<'_>) -> bool {
        if view
            .payload::<SceneView>()
            .is_some_and(|scene_view| !scene_view.presents_to_surface())
        {
            return false;
        }
        self.fx.is_enabled(frame, view)
    }

    fn setup(
        &mut self,
        graph: &mut RenderGraph,
        state: &mut PhaseState,
        frame: &PreparedFrame<'_>,
        view: &PreparedView<'_>,
    ) {
        let mut context = PostFxPassSetupContext::new(graph, state, frame, view);
        self.fx.setup(&mut context);
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
        let mut context = PostFxPassExecuteContext::new(ctx, pass, resources, execution);
        self.fx.execute(&mut context)?;
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
        self.fx.draw_calls(execution)
    }

    fn resize(&mut self, ctx: &GpuContext, width: u32, height: u32) {
        self.fx.resize(ctx, width, height);
    }
}
