use crate::gpu::GpuContext;
use crate::render::core::fullscreen::{FullscreenPass, FullscreenPipeline};
use crate::render::frame_pipeline::{
    FrameViewNode, PhaseState, PreparedFrame, PreparedView, ViewExecutionContext,
};
use crate::render::graph::{
    CompiledPass, PhysicalResources, RenderGraph, RenderGraphError, ResourceRef, TargetSize,
};
use std::sync::{Arc, Mutex};

use super::prepared::PreparedLive2DFrameSet;
use super::renderer::Live2DRenderer;

/// Graph-backed Live2D overlay feature.
///
/// The node preserves the previous `current` texture, then composites all
/// prepared Live2D frames associated with the active view on top.
pub struct Live2DOverlayNode {
    blit_pipeline: FullscreenPipeline,
    texture_bgl: wgpu::BindGroupLayout,
    renderer: Arc<Mutex<Live2DRenderer>>,
}

impl Live2DOverlayNode {
    pub fn new(ctx: &GpuContext) -> Self {
        Self::with_shared_renderer(ctx, Arc::new(Mutex::new(Live2DRenderer::new(ctx))))
    }

    pub fn with_shared_renderer(ctx: &GpuContext, renderer: Arc<Mutex<Live2DRenderer>>) -> Self {
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
            renderer,
        }
    }
}

impl FrameViewNode for Live2DOverlayNode {
    fn name(&self) -> &'static str {
        "live2d_overlay"
    }

    fn is_enabled(&self, _frame: &PreparedFrame<'_>) -> bool {
        true
    }

    fn setup(
        &mut self,
        graph: &mut RenderGraph,
        state: &mut PhaseState,
        _frame: &PreparedFrame<'_>,
        view: &PreparedView<'_>,
    ) {
        let input = state
            .current_color()
            .expect("Live2DOverlayNode requires current input");
        let format = input.format();

        let output = graph.create_texture(|b| {
            b.name("live2d_overlay_out")
                .size(TargetSize::Exact(
                    view.target_size()[0],
                    view.target_size()[1],
                ))
                .format(format);
        });
        graph.add_render_pass("live2d_overlay", |s| {
            s.read(input.handle());
            s.write_color(0, output);
        });
        state.set_current_color(output, format);
    }

    fn execute(
        &mut self,
        pass: &CompiledPass,
        ctx: &mut GpuContext,
        resources: &PhysicalResources<'_>,
        execution: &ViewExecutionContext<'_>,
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

        if let Some(prepared_frames) = execution.frame_payload::<PreparedLive2DFrameSet>() {
            let mut renderer = self
                .renderer
                .lock()
                .expect("Live2D overlay renderer lock poisoned");
            for frame in prepared_frames.frames_for_view(execution.view_index()) {
                debug_assert_eq!(
                    frame.target_format(),
                    output_rt.format(),
                    "PreparedLive2DFrame target format must match the overlay output",
                );
                renderer.execute_prepared_model_to_target(ctx, output_rt, frame);
            }
        }

        Ok(())
    }

    fn draw_calls(&self, execution: &ViewExecutionContext<'_>) -> usize {
        let blit_draw = 1usize;
        let live2d_draws = execution
            .frame_payload::<PreparedLive2DFrameSet>()
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
