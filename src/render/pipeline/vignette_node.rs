use crate::gpu::GpuContext;
use crate::render::ecs::RenderSettings2D;
use crate::render::graph::{
    CompiledPass, PhysicalResources, RenderGraph, RenderGraphError, ResourceRef, TargetSize,
};
use crate::render::pipeline::{FeatureExecutionContext2D, PipelineState2D, RenderFeature2D};
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

impl RenderFeature2D for VignetteNode {
    fn name(&self) -> &'static str {
        "vignette"
    }

    fn is_enabled(&self, settings: &RenderSettings2D, _has_surface: bool) -> bool {
        settings.vignette.enabled
    }

    fn setup(&mut self, graph: &mut RenderGraph, state: &mut PipelineState2D) {
        let input = state
            .current()
            .expect("VignetteNode requires current input");
        let vignette_out = graph.create_texture(|b| {
            b.name("vignette_out")
                .size(TargetSize::Exact(
                    state.view_size()[0],
                    state.view_size()[1],
                ))
                .format(HDR_FORMAT);
        });
        graph.add_render_pass("vignette", |s| {
            s.read(input);
            s.write_color(0, vignette_out);
        });
        state.set_current(vignette_out, HDR_FORMAT);
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
            .expect("vignette should have a read");
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
            .expect("vignette should have a write");

        let input_rt = resources
            .render_target(input_handle)
            .expect("vignette input should be allocated");
        let output_rt = resources
            .render_target(output_handle)
            .expect("vignette output should be allocated");
        self.vignette.apply_to_target(ctx, input_rt, output_rt);
        Ok(())
    }

    fn apply_settings(&mut self, settings: &RenderSettings2D) {
        self.vignette.intensity = settings.vignette.intensity;
        self.vignette.smoothness = settings.vignette.smoothness;
    }

    fn draw_calls(&self, _execution: &FeatureExecutionContext2D<'_>) -> usize {
        1
    }
}
