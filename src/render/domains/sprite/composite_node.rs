use crate::gpu::GpuContext;
use crate::render::frame_pipeline::{
    pass_first_write_texture, pass_nth_read_texture, require_render_target, FrameViewNode,
    PhaseState, PreparedFrame, PreparedView, ViewExecutionContext,
};
use crate::render::graph::{
    CompiledPass, PhysicalResources, RenderGraph, RenderGraphError, TargetSize,
};
use crate::render::passes::composite_pass::CompositePass;

use super::require_texture_slot;

const HDR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

pub struct SpriteCompositeNode {
    pub(crate) composite_pass: CompositePass,
}

impl SpriteCompositeNode {
    pub fn new(ctx: &GpuContext) -> Self {
        Self {
            composite_pass: CompositePass::new(ctx, HDR_FORMAT),
        }
    }
}

impl FrameViewNode for SpriteCompositeNode {
    fn name(&self) -> &'static str {
        "composite"
    }

    fn setup(
        &mut self,
        graph: &mut RenderGraph,
        state: &mut PhaseState,
        _frame: &PreparedFrame<'_>,
        view: &PreparedView<'_>,
    ) {
        let scene_tex = require_texture_slot(state, "scene_color", "SpriteCompositeNode");
        let light_tex = require_texture_slot(state, "lightmap", "SpriteCompositeNode");

        let composite_out = graph.create_texture(|b| {
            b.name("composite_out")
                .size(TargetSize::Exact(
                    view.target_size()[0],
                    view.target_size()[1],
                ))
                .format(HDR_FORMAT);
        });
        graph.add_render_pass("composite", |s| {
            s.read(scene_tex.handle());
            s.read(light_tex.handle());
            s.write_color(0, composite_out);
        });
        state.set_current_color(composite_out, HDR_FORMAT);
    }

    fn execute(
        &mut self,
        pass: &CompiledPass,
        ctx: &mut GpuContext,
        resources: &PhysicalResources<'_>,
        _execution: &ViewExecutionContext<'_>,
    ) -> Result<(), RenderGraphError> {
        let scene_tex = pass_nth_read_texture(pass, 0, self.name(), "scene color");
        let light_tex = pass_nth_read_texture(pass, 1, self.name(), "lightmap");

        let scene_rt = require_render_target(resources, scene_tex, self.name(), "scene");
        let light_rt = require_render_target(resources, light_tex, self.name(), "lightmap");
        let composite_out = pass_first_write_texture(pass, self.name(), "composite output");
        let composite_out_rt =
            require_render_target(resources, composite_out, self.name(), "output");

        self.composite_pass
            .render_to_target(ctx, scene_rt, light_rt, composite_out_rt);
        Ok(())
    }

    fn draw_calls(&self, _execution: &ViewExecutionContext<'_>) -> usize {
        1
    }
}
