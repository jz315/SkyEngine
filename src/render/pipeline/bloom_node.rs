use crate::gpu::GpuContext;
use crate::render::ecs::RenderSettings2D;
use crate::render::graph::{
    CompiledPass, PhysicalResources, RenderGraph, RenderGraphError, ResourceRef, TargetSize,
};
use crate::render::pipeline::{FeatureExecutionContext2D, PipelineState2D, RenderFeature2D};
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

impl RenderFeature2D for BloomNode {
    fn name(&self) -> &'static str {
        "bloom"
    }

    fn is_enabled(&self, settings: &RenderSettings2D, _has_surface: bool) -> bool {
        settings.bloom.enabled
    }

    fn setup(&mut self, graph: &mut RenderGraph, state: &mut PipelineState2D) {
        let input = state.current().expect("BloomNode requires current input");
        let bloom_out = graph.create_texture(|b| {
            b.name("bloom_out")
                .size(TargetSize::Exact(
                    state.view_size()[0],
                    state.view_size()[1],
                ))
                .format(HDR_FORMAT);
        });
        graph.add_render_pass("bloom", |s| {
            s.read(input);
            s.write_color(0, bloom_out);
        });
        state.set_current(bloom_out);
    }

    fn execute(
        &mut self,
        pass: &CompiledPass,
        ctx: &mut GpuContext,
        resources: &PhysicalResources<'_>,
        _execution: &FeatureExecutionContext2D<'_>,
    ) -> Result<(), RenderGraphError> {
        let input_handle = pass
            .reads
            .iter()
            .find_map(|r| {
                if let ResourceRef::Texture(th) = r {
                    Some(*th)
                } else {
                    None
                }
            })
            .expect("bloom should have a read");
        let output_handle = pass
            .writes
            .iter()
            .find_map(|w| {
                if let ResourceRef::Texture(th) = w {
                    Some(*th)
                } else {
                    None
                }
            })
            .expect("bloom should have a write");

        let input_rt = resources
            .render_target(input_handle)
            .expect("bloom input should be allocated");
        let output_rt = resources
            .render_target(output_handle)
            .expect("bloom output should be allocated");
        self.bloom.apply(ctx, input_rt, output_rt);
        Ok(())
    }

    fn apply_settings(&mut self, settings: &RenderSettings2D) {
        self.bloom.threshold = settings.bloom.threshold;
        self.bloom.intensity = settings.bloom.intensity;
        self.bloom.radius = settings.bloom.radius;
    }

    fn resize(&mut self, ctx: &GpuContext, _width: u32, _height: u32) {
        let [w, h] = ctx.surface_size();
        self.bloom = Bloom::new(ctx, w, h, HDR_FORMAT);
    }

    fn draw_calls(&self, _execution: &FeatureExecutionContext2D<'_>) -> usize {
        DRAW_CALLS_PER_APPLY
    }
}
