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
use crate::render::postfx::bloom::{Bloom, DRAW_CALLS_PER_APPLY};

const HDR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

pub struct BloomNode {
    pub(crate) bloom: Bloom,
}

impl BloomNode {
    pub fn new(ctx: &GpuContext) -> Self {
        let [width, height] = ctx.surface_size();
        Self {
            bloom: Bloom::new(ctx, width, height, HDR_FORMAT),
        }
    }
}

impl FrameViewNode for BloomNode {
    fn name(&self) -> &'static str {
        "bloom"
    }

    fn is_enabled(&self, frame: &PreparedFrame<'_>) -> bool {
        frame
            .payload::<RenderSettings>()
            .copied()
            .unwrap_or_default()
            .bloom
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
        self.bloom.threshold = settings.bloom.threshold;
        self.bloom.intensity = settings.bloom.intensity;
        self.bloom.radius = settings.bloom.radius;

        let input = require_current_color(state, self.name());
        let bloom_out = graph.create_texture(|b| {
            b.name("bloom_out")
                .size(TargetSize::Exact(
                    view.target_size()[0],
                    view.target_size()[1],
                ))
                .format(HDR_FORMAT);
        });
        graph.add_render_pass("bloom", |s| {
            s.read(input.handle());
            s.write_color(0, bloom_out);
        });
        state.set_current_color(bloom_out, HDR_FORMAT);
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
        self.bloom.apply(ctx, input_rt, output_rt);
        Ok(())
    }

    fn resize(&mut self, ctx: &GpuContext, _width: u32, _height: u32) {
        let [w, h] = ctx.surface_size();
        self.bloom = Bloom::new(ctx, w, h, HDR_FORMAT);
    }

    fn draw_calls(&self, _execution: &ViewExecutionContext<'_>) -> usize {
        DRAW_CALLS_PER_APPLY
    }
}
