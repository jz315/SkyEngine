use crate::gpu::GpuContext;
use crate::render::core::fullscreen::{FullscreenPass, FullscreenPipeline};
use crate::render::ecs::RenderSettings2D;
use crate::render::graph::{
    CompiledPass, PhysicalResources, RenderGraph, RenderGraphError, ResourceRef,
};
use crate::render::pipeline::{FeatureExecutionContext2D, PipelineState2D, RenderFeature2D};

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

impl RenderFeature2D for ViewportBlitNode {
    fn name(&self) -> &'static str {
        "viewport_blit"
    }

    fn is_enabled(&self, _settings: &RenderSettings2D, has_surface: bool) -> bool {
        has_surface
    }

    fn setup(&mut self, graph: &mut RenderGraph, state: &mut PipelineState2D) {
        let input = state
            .current()
            .expect("ViewportBlitNode requires current input");
        graph.add_render_pass("viewport_blit", |s| {
            s.read(input);
            if state.clear_surface() {
                s.write_surface_color(
                    0,
                    crate::render::graph::LoadOp::Clear(state.clear_color().to_array()),
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
        execution: &FeatureExecutionContext2D<'_>,
    ) -> Result<(), RenderGraphError> {
        let _surface_write = pass
            .writes
            .iter()
            .any(|resource| matches!(resource, ResourceRef::Surface));
        let Some(input) = pass.reads.iter().find_map(|resource| match resource {
            ResourceRef::Texture(handle) => Some(*handle),
            _ => None,
        }) else {
            return Ok(());
        };
        let input_rt = resources
            .render_target(input)
            .expect("viewport blit input should exist");
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
        let viewport = execution.viewport();
        if execution.state().clear_surface() {
            ctx.with_surface_pass(
                "viewport_blit",
                Some(execution.state().clear_color().to_wgpu()),
                |render_pass| {
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
                    FullscreenPass::draw(render_pass);
                },
            );
        } else {
            ctx.with_surface_pass_loaded("viewport_blit", |render_pass| {
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
                FullscreenPass::draw(render_pass);
            });
        }
        Ok(())
    }

    fn draw_calls(&self, _execution: &FeatureExecutionContext2D<'_>) -> usize {
        1
    }
}
