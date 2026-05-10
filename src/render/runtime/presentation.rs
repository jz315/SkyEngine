use crate::gpu::GpuContext;
use crate::render::component::RenderSettings;
use crate::render::execution::{
    pass_first_read_texture, require_current_color, require_render_target, FrameViewNode,
    PhaseState, PreparedFrame, PreparedView, RuntimeRenderServices, ViewExecutionContext,
};
use crate::render::gpu::{FullscreenPass, FullscreenPipeline};
use crate::render::graph::{CompiledPass, PhysicalResources, RenderGraph, RenderGraphError};
use crate::render::view::SceneView;

pub struct ViewportBlitNode {
    pipeline: FullscreenPipeline,
    texture_bgl: wgpu::BindGroupLayout,
}

impl ViewportBlitNode {
    pub fn new(ctx: &GpuContext) -> Self {
        let texture_bgl = ctx
            .device()
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("viewport_blit_texture_bgl"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        let pipeline = FullscreenPipeline::new(
            ctx,
            "@group(0) @binding(0) var input_tex: texture_2d<f32>;\n@group(0) @binding(1) var input_sampler: sampler;\n@fragment\nfn fs_main(in: FullscreenOutput) -> @location(0) vec4<f32> {\n    return textureSample(input_tex, input_sampler, in.uv);\n}",
            "fs_main",
            &[&texture_bgl],
            ctx.surface_format(),
            None,
            "viewport_blit",
        );

        Self {
            pipeline,
            texture_bgl,
        }
    }
}

impl FrameViewNode<dyn RuntimeRenderServices + '_> for ViewportBlitNode {
    fn name(&self) -> &'static str {
        "viewport_blit"
    }

    fn is_enabled(&self, frame: &PreparedFrame<'_>) -> bool {
        frame.has_surface()
    }

    fn setup(
        &mut self,
        graph: &mut RenderGraph,
        state: &mut PhaseState,
        frame: &PreparedFrame<'_>,
        view: &PreparedView<'_>,
    ) {
        if view
            .payload::<SceneView>()
            .is_some_and(|scene_view| !scene_view.presents_to_surface())
        {
            return;
        }
        let settings = frame
            .payload::<RenderSettings>()
            .cloned()
            .unwrap_or_default();
        let input = require_current_color(state, self.name());
        graph.add_render_pass("viewport_blit", |s| {
            s.read(input.handle());
            if view.clear_surface() {
                s.write_surface_color(
                    0,
                    crate::render::graph::LoadOp::Clear(settings.clear_color.to_array()),
                );
            } else {
                s.write_surface_color(0, crate::render::graph::LoadOp::Load);
            }
        });
    }

    fn execute(
        &mut self,
        pass: &CompiledPass,
        ctx: &mut GpuContext,
        resources: &PhysicalResources<'_>,
        execution: &ViewExecutionContext<'_>,
        _services: &mut (dyn RuntimeRenderServices + '_),
    ) -> Result<(), RenderGraphError> {
        if execution
            .view_payload::<SceneView>()
            .is_some_and(|scene_view| !scene_view.presents_to_surface())
        {
            return Ok(());
        }
        let settings = execution
            .frame()
            .payload::<RenderSettings>()
            .cloned()
            .unwrap_or_default();
        let input = pass_first_read_texture(pass, self.name(), "input");
        let input_rt = require_render_target(resources, input, self.name(), "input");
        let bind_group = ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("viewport_blit_bg"),
            layout: &self.texture_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(input_rt.view()),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(ctx.sampler_linear()),
                },
            ],
        });

        let pipeline = self.pipeline.pipeline(ctx, ctx.surface_format());
        let viewport = execution.view().viewport();
        if execution.view().clear_surface() {
            let mut frame = ctx.frame();
            let mut render_pass =
                frame.begin_surface_pass("viewport_blit", Some(settings.clear_color.to_wgpu()));
            render_pass.set_viewport(
                viewport.x as f32,
                viewport.y as f32,
                viewport.width as f32,
                viewport.height as f32,
                0.0,
                1.0,
            );
            render_pass.set_pipeline(pipeline.as_ref());
            render_pass.set_bind_group(0, &bind_group, &[]);
            FullscreenPass::draw(&mut render_pass);
        } else {
            let mut frame = ctx.frame();
            let mut render_pass = frame.begin_surface_pass_loaded("viewport_blit");
            render_pass.set_viewport(
                viewport.x as f32,
                viewport.y as f32,
                viewport.width as f32,
                viewport.height as f32,
                0.0,
                1.0,
            );
            render_pass.set_pipeline(pipeline.as_ref());
            render_pass.set_bind_group(0, &bind_group, &[]);
            FullscreenPass::draw(&mut render_pass);
        }
        Ok(())
    }

    fn draw_calls(
        &self,
        _execution: &ViewExecutionContext<'_>,
        _services: &(dyn RuntimeRenderServices + '_),
    ) -> usize {
        if _execution
            .view_payload::<SceneView>()
            .is_some_and(|scene_view| !scene_view.presents_to_surface())
        {
            0
        } else {
            1
        }
    }
}
