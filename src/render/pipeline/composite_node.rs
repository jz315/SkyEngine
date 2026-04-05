use crate::gpu::GpuContext;
use crate::render::graph::{
    CompiledPass, PhysicalResources, RenderGraph, RenderGraphError, ResourceRef, TargetSize,
};
use crate::render::passes::composite_pass::CompositePass;
use crate::render::pipeline::{FeatureExecutionContext2D, PipelineState2D, RenderFeature2D};

const HDR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

pub struct CompositeNode {
    pub(crate) composite_pass: CompositePass,
}

impl CompositeNode {
    pub fn new(ctx: &GpuContext) -> Self {
        Self {
            composite_pass: CompositePass::new(ctx, HDR_FORMAT),
        }
    }
}

impl RenderFeature2D for CompositeNode {
    fn name(&self) -> &'static str {
        "composite"
    }

    fn setup(&mut self, graph: &mut RenderGraph, state: &mut PipelineState2D) {
        let scene_tex = state
            .scene_color()
            .expect("CompositeNode requires scene color");
        let light_tex = state.lightmap().expect("CompositeNode requires lightmap");

        let composite_out = graph.create_texture(|b| {
            b.name("composite_out")
                .size(TargetSize::Exact(
                    state.view_size()[0],
                    state.view_size()[1],
                ))
                .format(HDR_FORMAT);
        });
        graph.add_render_pass("composite", |s| {
            s.read(scene_tex);
            s.read(light_tex);
            s.write_color(0, composite_out);
        });
        state.set_current(composite_out);
    }

    fn execute(
        &mut self,
        pass: &CompiledPass,
        ctx: &mut GpuContext,
        resources: &PhysicalResources<'_>,
        _execution: &FeatureExecutionContext2D<'_>,
    ) -> Result<(), RenderGraphError> {
        let mut read_textures = pass.reads.iter().filter_map(|resource| match resource {
            ResourceRef::Texture(handle) => Some(*handle),
            _ => None,
        });
        let scene_tex = read_textures
            .next()
            .expect("composite should read scene color");
        let light_tex = read_textures
            .next()
            .expect("composite should read lightmap");

        let scene_rt = resources
            .render_target(scene_tex)
            .expect("scene_tex should be allocated");
        let light_rt = resources
            .render_target(light_tex)
            .expect("light_tex should be allocated");

        let composite_out_rt = pass
            .writes
            .iter()
            .find_map(|w| {
                if let ResourceRef::Texture(th) = w {
                    resources.render_target(*th)
                } else {
                    None
                }
            })
            .expect("composite output should be allocated");

        self.composite_pass
            .render_to_target(ctx, scene_rt, light_rt, composite_out_rt);
        Ok(())
    }

    fn draw_calls(&self, _execution: &FeatureExecutionContext2D<'_>) -> usize {
        1
    }
}
