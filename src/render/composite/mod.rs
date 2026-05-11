//! Scene/light composite pass.

use crate::gpu::GpuContext;
use crate::render::gpu::RenderTarget;
use crate::render::gpu::{FullscreenPass, FullscreenPipeline};

const COMPOSITE_SHADER: &str = include_str!("../shaders/composite/composite.wgsl");

/// scene_color * lightmap_color with ambient expected in the lightmap.
pub struct CompositePass {
    pipeline: FullscreenPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
}

impl CompositePass {
    fn targets_alias(lhs: &RenderTarget, rhs: &RenderTarget) -> bool {
        std::ptr::eq(lhs.texture(), rhs.texture())
    }

    fn validate_targets(scene: &RenderTarget, lightmap: &RenderTarget, output: &RenderTarget) {
        assert!(
            !Self::targets_alias(scene, output),
            "CompositePass requires distinct scene and output targets",
        );
        assert!(
            !Self::targets_alias(lightmap, output),
            "CompositePass requires distinct lightmap and output targets",
        );
    }

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
        Self::validate_targets(scene, lightmap, output);

        let bind_group = self.bind_group(ctx, scene, lightmap);
        let pipeline = self.pipeline.pipeline(ctx, output.format());
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
            label: Some("composite_pass"),
            color_attachments: &color_attachments,
            depth_stencil_attachment: None,
            ..Default::default()
        });
        pass.set_pipeline(pipeline.as_ref());
        pass.set_bind_group(0, &bind_group, &[]);
        FullscreenPass::draw(&mut pass);
    }

    pub fn render_to_surface(
        &mut self,
        ctx: &mut GpuContext,
        scene: &RenderTarget,
        lightmap: &RenderTarget,
    ) {
        let bind_group = self.bind_group(ctx, scene, lightmap);
        let pipeline = self.pipeline.pipeline(ctx, ctx.surface_format());
        let mut frame = ctx.frame();
        let mut pass = frame.begin_surface_pass("composite_pass", Some(wgpu::Color::BLACK));
        pass.set_pipeline(pipeline.as_ref());
        pass.set_bind_group(0, &bind_group, &[]);
        FullscreenPass::draw(&mut pass);
    }

    fn bind_group(
        &self,
        ctx: &GpuContext,
        scene: &RenderTarget,
        lightmap: &RenderTarget,
    ) -> wgpu::BindGroup {
        let scene_view = scene.view();
        let lightmap_view = lightmap.view();
        let sampler = ctx.sampler_linear();
        ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("composite_bg"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(scene_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(lightmap_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::panic::{self, AssertUnwindSafe};

    fn create_test_device() -> (wgpu::Device, wgpu::Queue) {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .expect("No suitable GPU adapter found for render tests");

        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("render_test_device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::Performance,
            ..Default::default()
        }))
        .expect("Failed to create test GPU device")
    }

    #[test]
    fn composite_pass_rejects_aliasing_output_targets() {
        let (device, queue) = create_test_device();
        let mut ctx = crate::gpu::GpuContext::new_headless(
            device,
            queue,
            wgpu::TextureFormat::Bgra8Unorm,
            [4, 4],
        );
        let mut pass = CompositePass::new(&ctx, wgpu::TextureFormat::Rgba16Float);
        let shared = RenderTarget::new(&ctx, 4, 4, wgpu::TextureFormat::Rgba16Float, "shared");
        let lightmap = RenderTarget::new(&ctx, 4, 4, wgpu::TextureFormat::Rgba16Float, "light");

        ctx.begin_frame()
            .expect("headless begin_frame should succeed");
        let result = panic::catch_unwind(AssertUnwindSafe(|| {
            pass.render_to_target(&mut ctx, &shared, &lightmap, &shared);
        }));
        ctx.end_frame();

        assert!(result.is_err());
    }
}
