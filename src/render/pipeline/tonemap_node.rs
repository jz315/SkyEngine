use crate::gpu::GpuContext;
use crate::render::ecs::RenderSettings2D;
use crate::render::graph::{
    CompiledPass, PhysicalResources, RenderGraph, RenderGraphError, ResourceRef, TargetSize,
};
use crate::render::pipeline::{FeatureExecutionContext2D, PipelineState2D, RenderFeature2D};
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

impl RenderFeature2D for ToneMapNode {
    fn name(&self) -> &'static str {
        "tonemap"
    }

    fn is_enabled(&self, settings: &RenderSettings2D, _has_surface: bool) -> bool {
        settings.tonemap.enabled
    }

    fn setup(&mut self, graph: &mut RenderGraph, state: &mut PipelineState2D) {
        let input = state.current().expect("ToneMapNode requires current input");

        let output = graph.create_texture(|b| {
            b.name("tonemap_out")
                .size(TargetSize::Exact(
                    state.view_size()[0],
                    state.view_size()[1],
                ))
                .format(state.surface_format());
        });

        graph.add_render_pass("tonemap", |s| {
            s.read(input);
            s.write_color(0, output);
        });
        state.set_current(output, state.surface_format());
    }

    fn execute(
        &mut self,
        pass: &CompiledPass,
        ctx: &mut GpuContext,
        resources: &PhysicalResources<'_>,
        _execution: &FeatureExecutionContext2D<'_>,
    ) -> Result<(), RenderGraphError> {
        let final_hdr = pass
            .reads
            .iter()
            .find_map(|resource| match resource {
                ResourceRef::Texture(handle) => Some(*handle),
                _ => None,
            })
            .expect("tonemap should have input texture");
        let input_rt = resources
            .render_target(final_hdr)
            .expect("final_hdr should be allocated");

        let output_handle = pass
            .writes
            .iter()
            .find_map(|resource| match resource {
                ResourceRef::Texture(handle) => Some(*handle),
                _ => None,
            })
            .expect("tonemap should have output texture");
        let output_rt = resources
            .render_target(output_handle)
            .expect("tonemap output should be allocated");

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

    fn apply_settings(&mut self, settings: &RenderSettings2D) {
        self.tonemap.exposure = settings.tonemap.exposure;
        self.tonemap.gamma = settings.tonemap.gamma.max(0.001);
    }

    fn draw_calls(&self, _execution: &FeatureExecutionContext2D<'_>) -> usize {
        1
    }
}
