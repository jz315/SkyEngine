//! Scene/light composite pass.

use crate::gpu::GpuContext;
use crate::render::core::fullscreen::{FullscreenPass, FullscreenPipeline};
use crate::render::core::target::RenderTarget;

const COMPOSITE_SHADER: &str = include_str!("../shaders/composite.wgsl");

/// scene_color * lightmap_color -> output
pub struct CompositePass {
    pipeline: FullscreenPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
}

impl CompositePass {
    pub fn new(ctx: &GpuContext, target_format: wgpu::TextureFormat) -> Self {
        let bind_group_layout =
            ctx.device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("composite_bgl"),
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
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 3,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                            count: None,
                        },
                    ],
                });

        let pipeline = FullscreenPipeline::new(
            ctx,
            COMPOSITE_SHADER,
            "fs_main",
            &[&bind_group_layout],
            target_format,
            None,
            "composite_pipeline",
        );

        Self {
            pipeline,
            bind_group_layout,
        }
    }

    pub fn render_to_target(
        &mut self,
        ctx: &mut GpuContext,
        scene: &RenderTarget,
        lightmap: &RenderTarget,
        output: &RenderTarget,
    ) {
        let bind_group = self.create_bind_group(ctx, scene, lightmap);
        let pipeline = self.pipeline.pipeline(ctx, output.format());
        ctx.with_render_pass(
            &wgpu::RenderPassDescriptor {
                label: Some("composite_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: output.view(),
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                ..Default::default()
            },
            |pass| {
                pass.set_pipeline(pipeline.as_ref());
                pass.set_bind_group(0, &bind_group, &[]);
                FullscreenPass::draw(pass);
            },
        );
    }

    pub fn render_to_surface(
        &mut self,
        ctx: &mut GpuContext,
        scene: &RenderTarget,
        lightmap: &RenderTarget,
    ) {
        let bind_group = self.create_bind_group(ctx, scene, lightmap);
        let pipeline = self.pipeline.pipeline(ctx, ctx.surface_format());
        ctx.with_surface_pass("composite_pass", Some(wgpu::Color::BLACK), |pass| {
            pass.set_pipeline(pipeline.as_ref());
            pass.set_bind_group(0, &bind_group, &[]);
            FullscreenPass::draw(pass);
        });
    }

    fn create_bind_group(
        &self,
        ctx: &GpuContext,
        scene: &RenderTarget,
        lightmap: &RenderTarget,
    ) -> wgpu::BindGroup {
        ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("composite_bg"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(scene.view()),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(ctx.sampler_linear()),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(lightmap.view()),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(ctx.sampler_linear()),
                },
            ],
        })
    }
}
