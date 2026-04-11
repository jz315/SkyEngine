use crate::gpu::GpuContext;
use crate::render::ecs::RenderSettings;
use crate::render::frame_pipeline::{
    pass_first_read_texture, pass_first_write_texture, require_current_color,
    require_render_target, FrameViewNode, PhaseState, PreparedFrame, PreparedView,
    ViewExecutionContext,
};
use crate::render::graph::{
    CompiledPass, PhysicalResources, RenderGraph, RenderGraphError, TargetSize,
};
use crate::render::postfx::tonemap::ToneMap;

pub struct ToneMapNode {
    pub(crate) tonemap: ToneMap,
}

impl ToneMapNode {
    pub fn new(ctx: &GpuContext) -> Self {
        Self {
            tonemap: ToneMap::new(ctx, ctx.surface_format()),
        }
    }
}

impl FrameViewNode for ToneMapNode {
    fn name(&self) -> &'static str {
        "tonemap"
    }

    fn is_enabled(&self, frame: &PreparedFrame<'_>) -> bool {
        frame
            .payload::<RenderSettings>()
            .copied()
            .unwrap_or_default()
            .tonemap
            .enabled
    }

    fn setup(
        &mut self,
        graph: &mut RenderGraph,
        state: &mut PhaseState,
        frame: &PreparedFrame<'_>,
        view: &PreparedView<'_>,
    ) {
        let settings = frame
            .payload::<RenderSettings>()
            .copied()
            .unwrap_or_default();
        self.tonemap.exposure = settings.tonemap.exposure;
        self.tonemap.gamma = settings.tonemap.gamma.max(0.001);

        let input = require_current_color(state, self.name());

        let output = graph.create_texture(|b| {
            b.name("tonemap_out")
                .size(TargetSize::Exact(
                    view.target_size()[0],
                    view.target_size()[1],
                ))
                .format(state.surface_format());
        });

        graph.add_render_pass("tonemap", |s| {
            s.read(input.handle());
            s.write_color(0, output);
        });
        state.set_current_color(output, state.surface_format());
    }

    fn execute(
        &mut self,
        pass: &CompiledPass,
        ctx: &mut GpuContext,
        resources: &PhysicalResources<'_>,
        _execution: &ViewExecutionContext<'_>,
    ) -> Result<(), RenderGraphError> {
        let input_handle = pass_first_read_texture(pass, self.name(), "input");
        let input_rt = require_render_target(resources, input_handle, self.name(), "input");

        let output_handle = pass_first_write_texture(pass, self.name(), "output");
        let output_rt = require_render_target(resources, output_handle, self.name(), "output");

        if self.tonemap.gamma > 0.0 {
            self.tonemap.apply_to_target(ctx, input_rt, output_rt);
        } else {
            let saved_exposure = self.tonemap.exposure;
            let saved_gamma = self.tonemap.gamma;
            self.tonemap.exposure = 1.0;
            self.tonemap.gamma = 1.0;
            self.tonemap.apply_to_target(ctx, input_rt, output_rt);
            self.tonemap.exposure = saved_exposure;
            self.tonemap.gamma = saved_gamma;
        }
        Ok(())
    }

    fn draw_calls(&self, _execution: &ViewExecutionContext<'_>) -> usize {
        1
    }
}
