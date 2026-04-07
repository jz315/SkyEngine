use crate::gpu::GpuContext;
use crate::render::core::fullscreen::{FullscreenPass, FullscreenPipeline};
use crate::render::ecs::RenderSettings2D;
use crate::render::graph::{
    CompiledPass, PhysicalResources, RenderGraph, RenderGraphError, ResourceRef, TargetSize,
};
use crate::render::pipeline::{FeatureExecutionContext2D, PipelineState2D, RenderFeature2D};

pub struct ColorResolveNode {
    pipeline: FullscreenPipeline,
    texture_bgl: wgpu::BindGroupLayout,
}

impl ColorResolveNode {
    pub fn new(ctx: &GpuContext) -> Self {
        let texture_bgl = ctx
            .device()
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("color_resolve_texture_bgl"),
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
            "color_resolve",
        );

        Self {
            pipeline,
            texture_bgl,
        }
    }
}

impl RenderFeature2D for ColorResolveNode {
    fn name(&self) -> &'static str {
        "color_resolve"
    }

    fn is_enabled(&self, settings: &RenderSettings2D, _has_surface: bool) -> bool {
        !settings.tonemap.enabled
    }

    fn setup(&mut self, graph: &mut RenderGraph, state: &mut PipelineState2D) {
        let input = state
            .current()
            .expect("ColorResolveNode requires current input");
        let output = graph.create_texture(|b| {
            b.name("color_resolve_out")
                .size(TargetSize::Exact(
                    state.view_size()[0],
                    state.view_size()[1],
                ))
                .format(state.surface_format());
        });
        graph.add_render_pass("color_resolve", |s| {
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
        let input_handle = pass
            .reads
            .iter()
            .find_map(|resource| match resource {
                ResourceRef::Texture(handle) => Some(*handle),
                _ => None,
            })
            .expect("color_resolve should have input texture");
        let output_handle = pass
            .writes
            .iter()
            .find_map(|resource| match resource {
                ResourceRef::Texture(handle) => Some(*handle),
                _ => None,
            })
            .expect("color_resolve should have output texture");

        let input_rt = resources
            .render_target(input_handle)
            .expect("color_resolve input should be allocated");
        let output_rt = resources
            .render_target(output_handle)
            .expect("color_resolve output should be allocated");
        let bind_group = ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("color_resolve_bg"),
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

        let pipeline = self.pipeline.pipeline(ctx, output_rt.format());
        ctx.with_render_pass(
            &wgpu::RenderPassDescriptor {
                label: Some("color_resolve"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: output_rt.view(),
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                ..Default::default()
            },
            |render_pass| {
                render_pass.set_pipeline(pipeline.as_ref());
                render_pass.set_bind_group(0, &bind_group, &[]);
                FullscreenPass::draw(render_pass);
            },
        );
        Ok(())
    }

    fn draw_calls(&self, _execution: &FeatureExecutionContext2D<'_>) -> usize {
        1
    }
}
