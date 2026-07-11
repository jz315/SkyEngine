use super::*;

pub(crate) struct SceneColorSeedNode {
    format: wgpu::TextureFormat,
}

impl SceneColorSeedNode {
    #[inline]
    pub(crate) fn new(format: wgpu::TextureFormat) -> Self {
        Self { format }
    }
}

impl FrameViewNode<dyn RuntimeRenderServices + '_> for SceneColorSeedNode {
    fn name(&self) -> &'static str {
        "scene_color_seed"
    }

    fn setup(
        &mut self,
        graph: &mut RenderGraph,
        state: &mut PhaseState,
        _frame: &PreparedFrame<'_>,
        view: &PreparedView<'_>,
    ) {
        if view
            .payload::<SceneView>()
            .is_some_and(|scene_view| !scene_view.presents_to_surface())
        {
            return;
        }
        let target = graph.create_texture(|builder| {
            builder
                .name("scene_color_seed")
                .size(TargetSize::Exact(
                    view.target_size()[0],
                    view.target_size()[1],
                ))
                .format(self.format);
        });
        graph.add_render_pass("scene_color_seed", |setup| {
            setup.write_color(0, target);
        });
        state.set_current_color(target, self.format);
        state.set_scene_color(target, self.format);
    }

    fn execute(
        &mut self,
        pass: &CompiledPass,
        ctx: &mut GpuContext,
        resources: &PhysicalResources<'_>,
        execution: &ViewExecutionContext<'_>,
        _services: &mut (dyn RuntimeRenderServices + '_),
    ) -> Result<(), RenderGraphError> {
        let color = execution
            .frame_payload::<RenderSettings>()
            .cloned()
            .unwrap_or_default()
            .clear_color
            .to_wgpu();
        let output_handle = pass_first_write_texture(pass, self.name(), "output");
        let output = require_render_target(resources, output_handle, self.name(), "output");
        let color_attachments = [Some(wgpu::RenderPassColorAttachment {
            view: output.view(),
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(color),
                store: wgpu::StoreOp::Store,
            },
        })];
        let mut frame = ctx.frame();
        let _render_pass = frame.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("scene_color_seed"),
            color_attachments: &color_attachments,
            depth_stencil_attachment: None,
            ..Default::default()
        });
        Ok(())
    }
}

pub(crate) struct HeadlessKeepAliveNode;

impl FrameViewNode<dyn RuntimeRenderServices + '_> for HeadlessKeepAliveNode {
    fn name(&self) -> &'static str {
        "scene_headless_keepalive"
    }

    fn is_enabled(&self, frame: &PreparedFrame<'_>) -> bool {
        !frame.has_surface()
    }

    fn setup(
        &mut self,
        graph: &mut RenderGraph,
        state: &mut PhaseState,
        _frame: &PreparedFrame<'_>,
        view: &PreparedView<'_>,
    ) {
        if view
            .payload::<SceneView>()
            .is_some_and(|scene_view| scene_view.is_shadow())
        {
            return;
        }
        let input = require_current_color(state, self.name());
        let sink = graph.create_texture(|builder| {
            builder
                .name("scene_headless_keepalive")
                .size(TargetSize::Exact(
                    view.target_size()[0],
                    view.target_size()[1],
                ))
                .format(input.format())
                .persistent();
        });
        graph.add_render_pass("scene_headless_keepalive", |setup| {
            setup.read(input.handle());
            setup.write_color(0, sink);
        });
        state.set_current_color(sink, input.format());
    }

    fn execute(
        &mut self,
        _pass: &CompiledPass,
        _ctx: &mut GpuContext,
        _resources: &PhysicalResources<'_>,
        _execution: &ViewExecutionContext<'_>,
        _services: &mut (dyn RuntimeRenderServices + '_),
    ) -> Result<(), RenderGraphError> {
        Ok(())
    }
}
