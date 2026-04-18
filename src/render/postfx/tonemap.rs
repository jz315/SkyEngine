//! HDR tonemapping.

use crate::gpu::GpuContext;
use crate::render::gpu::RenderTarget;
use crate::render::gpu::{FullscreenPass, FullscreenPipeline};
use crate::render::postfx::PostFx;

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ToneMapUniform {
    params: [f32; 4], // exposure, gamma, _, _
}

const TONEMAP_SHADER: &str = include_str!("../shaders/tonemap.wgsl");

pub struct ToneMap {
    pipeline: FullscreenPipeline,
    texture_bgl: wgpu::BindGroupLayout,
    params_buffer: wgpu::Buffer,
    params_bind_group: wgpu::BindGroup,
    pub exposure: f32,
    pub gamma: f32,
}

impl ToneMap {
    pub fn new(ctx: &GpuContext, target_format: wgpu::TextureFormat) -> Self {
        let texture_bgl = ctx
            .device()
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("tonemap_texture_bgl"),
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

        let params_bgl = ctx
            .device()
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("tonemap_params_bgl"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let params_buffer = ctx.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("tonemap_params_buf"),
            size: std::mem::size_of::<ToneMapUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let params_bind_group = ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("tonemap_params_bg"),
            layout: &params_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: params_buffer.as_entire_binding(),
            }],
        });

        let pipeline = FullscreenPipeline::new(
            ctx,
            TONEMAP_SHADER,
            "fs_main",
            &[&texture_bgl, &params_bgl],
            target_format,
            None,
            "tonemap_pipeline",
        );

        Self {
            pipeline,
            texture_bgl,
            params_buffer,
            params_bind_group,
            exposure: 1.0,
            gamma: 2.2,
        }
    }

    pub fn apply_to_surface(&mut self, ctx: &mut GpuContext, input: &RenderTarget) {
        let bind_group = self.create_texture_bg(ctx, input);
        let pipeline = self.pipeline.pipeline(ctx, ctx.surface_format());
        let uniform = ToneMapUniform {
            params: [self.exposure, self.gamma.max(0.001), 0.0, 0.0],
        };
        ctx.queue()
            .write_buffer(&self.params_buffer, 0, bytemuck::bytes_of(&uniform));

        let mut frame = ctx.frame();
        let mut pass = frame.begin_surface_pass("tonemap_pass", Some(wgpu::Color::BLACK));
        pass.set_pipeline(pipeline.as_ref());
        pass.set_bind_group(0, &bind_group, &[]);
        pass.set_bind_group(1, &self.params_bind_group, &[]);
        FullscreenPass::draw(&mut pass);
    }

    pub fn apply_to_target(
        &mut self,
        ctx: &mut GpuContext,
        input: &RenderTarget,
        output: &RenderTarget,
    ) {
        let bind_group = self.create_texture_bg(ctx, input);
        let pipeline = self.pipeline.pipeline(ctx, output.format());
        let uniform = ToneMapUniform {
            params: [self.exposure, self.gamma.max(0.001), 0.0, 0.0],
        };
        ctx.queue()
            .write_buffer(&self.params_buffer, 0, bytemuck::bytes_of(&uniform));

        let color_attachments = [Some(wgpu::RenderPassColorAttachment {
            view: output.view(),
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                store: wgpu::StoreOp::Store,
            },
        })];
        let mut frame = ctx.frame();
        let mut pass = frame.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("tonemap_pass"),
            color_attachments: &color_attachments,
            depth_stencil_attachment: None,
            ..Default::default()
        });
        pass.set_pipeline(pipeline.as_ref());
        pass.set_bind_group(0, &bind_group, &[]);
        pass.set_bind_group(1, &self.params_bind_group, &[]);
        FullscreenPass::draw(&mut pass);
    }

    fn create_texture_bg(&self, ctx: &GpuContext, input: &RenderTarget) -> wgpu::BindGroup {
        ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("tonemap_texture_bg"),
            layout: &self.texture_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(input.view()),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(ctx.sampler_linear()),
                },
            ],
        })
    }
}

impl PostFx for ToneMap {
    fn apply_to_target(
        &mut self,
        ctx: &mut GpuContext,
        input: &RenderTarget,
        output: &RenderTarget,
    ) {
        ToneMap::apply_to_target(self, ctx, input, output);
    }
}
