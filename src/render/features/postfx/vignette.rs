//! Vignette post effect.

use crate::gpu::GpuContext;
use crate::render::gpu::RenderTarget;
use crate::render::gpu::{FullscreenPass, FullscreenPipeline};
use crate::render::postfx::PostFx;

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct VignetteUniform {
    params: [f32; 4], // intensity, smoothness, _, _
}

const VIGNETTE_SHADER: &str = include_str!("../../shaders/postfx/vignette.wgsl");

pub struct Vignette {
    pipeline: FullscreenPipeline,
    texture_bgl: wgpu::BindGroupLayout,
    params_buffer: wgpu::Buffer,
    params_bind_group: wgpu::BindGroup,
    pub intensity: f32,
    pub smoothness: f32,
}

impl Vignette {
    pub fn new(ctx: &GpuContext, target_format: wgpu::TextureFormat) -> Self {
        let texture_bgl = ctx
            .device()
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("vignette_texture_bgl"),
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
                label: Some("vignette_params_bgl"),
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
            label: Some("vignette_params_buf"),
            size: std::mem::size_of::<VignetteUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let params_bind_group = ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("vignette_params_bg"),
            layout: &params_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: params_buffer.as_entire_binding(),
            }],
        });

        let pipeline = FullscreenPipeline::new(
            ctx,
            VIGNETTE_SHADER,
            "fs_main",
            &[&texture_bgl, &params_bgl],
            target_format,
            None,
            "vignette_pipeline",
        );

        Self {
            pipeline,
            texture_bgl,
            params_buffer,
            params_bind_group,
            intensity: 0.3,
            smoothness: 0.5,
        }
    }

    pub fn apply_to_surface(&mut self, ctx: &mut GpuContext, input: &RenderTarget) {
        let bind_group = self.create_texture_bg(ctx, input);
        let pipeline = self.pipeline.pipeline(ctx, ctx.surface_format());
        let uniform = VignetteUniform {
            params: [self.intensity, self.smoothness, 0.0, 0.0],
        };
        ctx.queue()
            .write_buffer(&self.params_buffer, 0, bytemuck::bytes_of(&uniform));

        let mut frame = ctx.frame();
        let mut pass = frame.begin_surface_pass("vignette_pass", Some(wgpu::Color::BLACK));
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
        let uniform = VignetteUniform {
            params: [self.intensity, self.smoothness, 0.0, 0.0],
        };
        ctx.queue()
            .write_buffer(&self.params_buffer, 0, bytemuck::bytes_of(&uniform));

        let color_attachments = [Some(wgpu::RenderPassColorAttachment {
            view: output.view(),
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                store: wgpu::StoreOp::Store,
            },
        })];
        let mut frame = ctx.frame();
        let mut pass = frame.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("vignette_pass"),
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
            label: Some("vignette_texture_bg"),
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

impl PostFx for Vignette {
    fn apply_to_target(
        &mut self,
        ctx: &mut GpuContext,
        input: &RenderTarget,
        output: &RenderTarget,
    ) {
        Vignette::apply_to_target(self, ctx, input, output);
    }
}
