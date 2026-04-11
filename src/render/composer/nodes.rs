use crate::gpu::GpuContext;
use crate::render::frame_pipeline::{
    pass_first_write_texture, require_current_color, require_render_target, FrameViewNode,
    PhaseState, PreparedFrame, PreparedView, ViewExecutionContext,
};
use crate::render::graph::{
    CompiledPass, PhysicalResources, RenderGraph, RenderGraphError, TargetSize,
};
use crate::render::RenderSettings;

pub(crate) struct ClearColorSeedNode;

impl FrameViewNode for ClearColorSeedNode {
    fn name(&self) -> &'static str {
        "scene_clear_seed"
    }

    fn setup(
        &mut self,
        graph: &mut RenderGraph,
        state: &mut PhaseState,
        frame: &PreparedFrame<'_>,
        view: &PreparedView<'_>,
    ) {
        let target = graph.create_texture(|builder| {
            builder
                .name("scene_clear_seed")
                .size(TargetSize::Exact(
                    view.target_size()[0],
                    view.target_size()[1],
                ))
                .format(frame.surface_format());
        });
        graph.add_render_pass("scene_clear_seed", |setup| {
            setup.write_color(0, target);
        });
        state.set_current_color(target, frame.surface_format());
    }

    fn execute(
        &mut self,
        pass: &CompiledPass,
        ctx: &mut GpuContext,
        resources: &PhysicalResources<'_>,
        execution: &ViewExecutionContext<'_>,
    ) -> Result<(), RenderGraphError> {
        let color = execution
            .frame_payload::<RenderSettings>()
            .copied()
            .unwrap_or_default()
            .clear_color
            .to_wgpu();
        let output_handle = pass_first_write_texture(pass, self.name(), "output");
        let output = require_render_target(resources, output_handle, self.name(), "output");
        ctx.with_render_pass(
            &wgpu::RenderPassDescriptor {
                label: Some("scene_clear_seed"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: output.view(),
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                ..Default::default()
            },
            |_render_pass| {},
        );
        Ok(())
    }
}

pub(crate) struct HeadlessKeepAliveNode;

impl FrameViewNode for HeadlessKeepAliveNode {
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
    ) -> Result<(), RenderGraphError> {
        Ok(())
    }
}
