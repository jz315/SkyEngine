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
use crate::render::postfx::vignette::Vignette;

const HDR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

pub struct VignetteNode {
    pub(crate) vignette: Vignette,
}

impl VignetteNode {
    pub fn new(ctx: &GpuContext) -> Self {
        Self {
            vignette: Vignette::new(ctx, HDR_FORMAT),
        }
    }
}

impl FrameViewNode for VignetteNode {
    fn name(&self) -> &'static str {
        "vignette"
    }

    fn is_enabled(&self, frame: &PreparedFrame<'_>) -> bool {
        frame
            .payload::<RenderSettings>()
            .copied()
            .unwrap_or_default()
            .vignette
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
        self.vignette.intensity = settings.vignette.intensity;
        self.vignette.smoothness = settings.vignette.smoothness;

        let input = require_current_color(state, self.name());
        let vignette_out = graph.create_texture(|b| {
            b.name("vignette_out")
                .size(TargetSize::Exact(
                    view.target_size()[0],
                    view.target_size()[1],
                ))
                .format(HDR_FORMAT);
        });
        graph.add_render_pass("vignette", |s| {
            s.read(input.handle());
            s.write_color(0, vignette_out);
        });
        state.set_current_color(vignette_out, HDR_FORMAT);
    }

    fn execute(
        &mut self,
        pass: &CompiledPass,
        ctx: &mut GpuContext,
        resources: &PhysicalResources<'_>,
        _execution: &ViewExecutionContext<'_>,
    ) -> Result<(), RenderGraphError> {
        let input_handle = pass_first_read_texture(pass, self.name(), "input");
        let output_handle = pass_first_write_texture(pass, self.name(), "output");

        let input_rt = require_render_target(resources, input_handle, self.name(), "input");
        let output_rt = require_render_target(resources, output_handle, self.name(), "output");
        self.vignette.apply_to_target(ctx, input_rt, output_rt);
        Ok(())
    }

    fn draw_calls(&self, _execution: &ViewExecutionContext<'_>) -> usize {
        1
    }
}
