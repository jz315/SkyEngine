use crate::gpu::GpuContext;
use crate::render::core::fullscreen::{FullscreenPass, FullscreenPipeline};
use crate::render::ecs::RenderSettings2D;
use crate::render::graph::{
    CompiledPass, PhysicalResources, RenderGraph, RenderGraphError, ResourceRef, TargetSize,
};
use crate::render::pipeline::{FeatureExecutionContext2D, PipelineState2D, RenderFeature2D};

use super::prepared::PreparedLive2DFrameSet;
use super::renderer::Live2DRenderer;

/// Graph-backed Live2D overlay feature.
///
/// The node preserves the previous `current` texture, then composites all
/// prepared Live2D frames associated with the active view on top.
pub struct Live2DOverlayNode {
    blit_pipeline: FullscreenPipeline,
    texture_bgl: wgpu::BindGroupLayout,
    renderer: Live2DRenderer,
}

impl Live2DOverlayNode {
    pub fn new(ctx: &GpuContext) -> Self {
        let texture_bgl = ctx
            .device()
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("live2d_overlay_texture_bgl"),
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

        let blit_pipeline = FullscreenPipeline::new(
            ctx,
            "@group(0) @binding(0) var input_tex: texture_2d<f32>;\n@group(0) @binding(1) var input_sampler: sampler;\n@fragment\nfn fs_main(in: FullscreenOutput) -> @location(0) vec4<f32> {\n    return textureSample(input_tex, input_sampler, in.uv);\n}",
            "fs_main",
            &[&texture_bgl],
            ctx.surface_format(),
            None,
            "live2d_overlay_blit",
        );

        Self {
            blit_pipeline,
            texture_bgl,
            renderer: Live2DRenderer::new(ctx),
        }
    }
}

impl RenderFeature2D for Live2DOverlayNode {
    fn name(&self) -> &'static str {
        "live2d_overlay"
    }

    fn is_enabled(&self, _settings: &RenderSettings2D, _has_surface: bool) -> bool {
        true
    }

    fn setup(&mut self, graph: &mut RenderGraph, state: &mut PipelineState2D) {
        let input = state
            .current()
            .expect("Live2DOverlayNode requires current input");
        let format = state
            .current_format()
            .expect("Live2DOverlayNode requires current target format");

        let output = graph.create_texture(|b| {
            b.name("live2d_overlay_out")
                .size(TargetSize::Exact(
                    state.view_size()[0],
                    state.view_size()[1],
                ))
                .format(format);
        });
        graph.add_render_pass("live2d_overlay", |s| {
            s.read(input);
            s.write_color(0, output);
        });
        state.set_current(output, format);
    }

    fn execute(
        &mut self,
        pass: &CompiledPass,
        ctx: &mut GpuContext,
        resources: &PhysicalResources<'_>,
        execution: &FeatureExecutionContext2D<'_>,
    ) -> Result<(), RenderGraphError> {
        let input_handle = pass
            .reads
            .iter()
            .find_map(|resource| match resource {
                ResourceRef::Texture(handle) => Some(*handle),
                _ => None,
            })
            .expect("live2d_overlay should have input texture");
        let output_handle = pass
            .writes
            .iter()
            .find_map(|resource| match resource {
                ResourceRef::Texture(handle) => Some(*handle),
                _ => None,
            })
            .expect("live2d_overlay should have output texture");

        let input_rt = resources
            .render_target(input_handle)
            .expect("live2d_overlay input should be allocated");
        let output_rt = resources
            .render_target(output_handle)
            .expect("live2d_overlay output should be allocated");

        let bind_group = ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("live2d_overlay_blit_bg"),
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
        let pipeline = self.blit_pipeline.pipeline(ctx, output_rt.format());
        ctx.with_render_pass(
            &wgpu::RenderPassDescriptor {
                label: Some("live2d_overlay_blit"),
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

        if let Some(prepared_frames) = execution.payload::<PreparedLive2DFrameSet>() {
            for frame in prepared_frames.frames_for_view(execution.view_index()) {
                debug_assert_eq!(
                    frame.target_format(),
                    output_rt.format(),
                    "PreparedLive2DFrame target format must match the overlay output",
                );
                self.renderer
                    .execute_prepared_model_to_target(ctx, output_rt, frame);
            }
        }

        Ok(())
    }

    fn draw_calls(&self, execution: &FeatureExecutionContext2D<'_>) -> usize {
        let blit_draw = 1usize;
        let live2d_draws = execution
            .payload::<PreparedLive2DFrameSet>()
            .map(|frames| {
                frames
                    .frames_for_view(execution.view_index())
                    .iter()
                    .map(|frame| frame.mask_draw_count() + frame.model_draw_count())
                    .sum::<usize>()
            })
            .unwrap_or(0);
        blit_draw + live2d_draws
    }
}
