//! Bloom post-processing.

use crate::gpu::GpuContext;
use crate::render::gpu::RenderTarget;
use crate::render::gpu::{FullscreenPass, FullscreenPipeline};
use crate::render::postfx::PostFx;

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct BloomUniform {
    params: [f32; 4],    // threshold, intensity, radius, _
    texel_dir: [f32; 4], // texel_x, texel_y, dir_x, dir_y
}

const BLOOM_SHADER: &str = include_str!("../shaders/postfx/bloom.wgsl");
const BLOOM_LEVELS: usize = 4;
pub(crate) const DRAW_CALLS_PER_APPLY: usize = 4 * BLOOM_LEVELS;

pub struct Bloom {
    bright_pipeline: FullscreenPipeline,
    downsample_pipeline: FullscreenPipeline,
    blur_pipeline: FullscreenPipeline,
    upsample_pipeline: FullscreenPipeline,
    combine_pipeline: FullscreenPipeline,
    sample_bgl: wgpu::BindGroupLayout,
    dual_bgl: wgpu::BindGroupLayout,
    params_buffer: wgpu::Buffer,
    params_bind_group: wgpu::BindGroup,
    mip_chain: Vec<RenderTarget>,
    temp_targets: Vec<RenderTarget>,
    pub threshold: f32,
    pub intensity: f32,
    pub radius: f32,
}

fn texture_sampler_bgl(device: &wgpu::Device, label: &str) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some(label),
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
    })
}

fn dual_texture_bgl(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("bloom_dual_bgl"),
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
    })
}

impl Bloom {
    pub fn new(
        ctx: &GpuContext,
        width: u32,
        height: u32,
        target_format: wgpu::TextureFormat,
    ) -> Self {
        let sample_bgl = texture_sampler_bgl(ctx.device(), "bloom_sample_bgl");
        let dual_bgl = dual_texture_bgl(ctx.device());

        let params_bgl = ctx
            .device()
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("bloom_params_bgl"),
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
            label: Some("bloom_params_buf"),
            size: std::mem::size_of::<BloomUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let params_bind_group = ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bloom_params_bg"),
            layout: &params_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: params_buffer.as_entire_binding(),
            }],
        });

        let additive_blend = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        };

        let bright_pipeline = FullscreenPipeline::new(
            ctx,
            BLOOM_SHADER,
            "fs_bright",
            &[&sample_bgl, &params_bgl],
            target_format,
            None,
            "bloom_bright",
        );
        let downsample_pipeline = FullscreenPipeline::new(
            ctx,
            BLOOM_SHADER,
            "fs_downsample",
            &[&sample_bgl, &params_bgl],
            target_format,
            None,
            "bloom_downsample",
        );
        let blur_pipeline = FullscreenPipeline::new(
            ctx,
            BLOOM_SHADER,
            "fs_blur",
            &[&sample_bgl, &params_bgl],
            target_format,
            None,
            "bloom_blur",
        );
        let upsample_pipeline = FullscreenPipeline::new(
            ctx,
            BLOOM_SHADER,
            "fs_upsample",
            &[&sample_bgl, &params_bgl],
            target_format,
            Some(additive_blend),
            "bloom_upsample",
        );
        let combine_pipeline = FullscreenPipeline::new(
            ctx,
            BLOOM_SHADER,
            "fs_combine",
            &[&dual_bgl, &params_bgl],
            target_format,
            None,
            "bloom_combine",
        );

        // Build mip chain
        let mut mip_chain = Vec::new();
        let mut temp_targets = Vec::new();
        for level in 0..BLOOM_LEVELS {
            let scale = 1u32 << (level as u32 + 1);
            let lw = (width / scale).max(1);
            let lh = (height / scale).max(1);
            mip_chain.push(RenderTarget::new(
                ctx,
                lw,
                lh,
                target_format,
                format!("bloom_mip_{level}"),
            ));
            temp_targets.push(RenderTarget::new(
                ctx,
                lw,
                lh,
                target_format,
                format!("bloom_tmp_{level}"),
            ));
        }

        Self {
            bright_pipeline,
            downsample_pipeline,
            blur_pipeline,
            upsample_pipeline,
            combine_pipeline,
            sample_bgl,
            dual_bgl,
            params_buffer,
            params_bind_group,
            mip_chain,
            temp_targets,
            threshold: 0.8,
            intensity: 0.3,
            radius: 1.0,
        }
    }

    pub fn resize(
        &mut self,
        ctx: &GpuContext,
        width: u32,
        height: u32,
        target_format: wgpu::TextureFormat,
    ) {
        for level in 0..BLOOM_LEVELS {
            let scale = 1u32 << (level as u32 + 1);
            let lw = (width / scale).max(1);
            let lh = (height / scale).max(1);
            self.mip_chain[level].resize(ctx, lw, lh, target_format);
            self.temp_targets[level].resize(ctx, lw, lh, target_format);
        }
    }

    fn update_uniform(
        &self,
        ctx: &GpuContext,
        threshold: f32,
        intensity: f32,
        texel: [f32; 2],
        dir: [f32; 2],
    ) {
        let uniform = BloomUniform {
            params: [threshold, intensity, self.radius, 0.0],
            texel_dir: [texel[0], texel[1], dir[0], dir[1]],
        };
        ctx.queue()
            .write_buffer(&self.params_buffer, 0, bytemuck::bytes_of(&uniform));
    }

    fn create_sample_bg(&self, ctx: &GpuContext, target: &RenderTarget) -> wgpu::BindGroup {
        ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bloom_sample_bg"),
            layout: &self.sample_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(target.view()),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(ctx.sampler_linear()),
                },
            ],
        })
    }

    fn run_single_input(
        &self,
        ctx: &mut GpuContext,
        pipeline: &wgpu::RenderPipeline,
        input_bg: &wgpu::BindGroup,
        output: &RenderTarget,
        clear: bool,
    ) {
        let load = if clear {
            wgpu::LoadOp::Clear(wgpu::Color::BLACK)
        } else {
            wgpu::LoadOp::Load
        };
        let color_attachments = [Some(wgpu::RenderPassColorAttachment {
            view: output.view(),
            resolve_target: None,
            ops: wgpu::Operations {
                load,
                store: wgpu::StoreOp::Store,
            },
        })];
        let mut frame = ctx.frame();
        let mut pass = frame.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("bloom_pass"),
            color_attachments: &color_attachments,
            depth_stencil_attachment: None,
            ..Default::default()
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, input_bg, &[]);
        pass.set_bind_group(1, &self.params_bind_group, &[]);
        FullscreenPass::draw(&mut pass);
    }

    pub fn apply(&mut self, ctx: &mut GpuContext, input: &RenderTarget, output: &RenderTarget) {
        self.resize(ctx, input.width(), input.height(), output.format());
        let bright_pipeline = self.bright_pipeline.pipeline(ctx, output.format());
        let downsample_pipeline = self.downsample_pipeline.pipeline(ctx, output.format());
        let blur_pipeline = self.blur_pipeline.pipeline(ctx, output.format());
        let upsample_pipeline = self.upsample_pipeline.pipeline(ctx, output.format());
        let combine_pipeline = self.combine_pipeline.pipeline(ctx, output.format());

        // Bright pass
        self.update_uniform(
            ctx,
            self.threshold,
            self.intensity,
            [1.0 / input.width() as f32, 1.0 / input.height() as f32],
            [0.0, 0.0],
        );
        let bright_bg = ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bloom_bright_bg"),
            layout: &self.sample_bgl,
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
        });
        {
            let mip0 = &self.mip_chain[0];
            self.run_single_input(ctx, bright_pipeline.as_ref(), &bright_bg, mip0, true);
        }

        // Downsample
        for level in 1..BLOOM_LEVELS {
            let bg = self.create_sample_bg(ctx, &self.mip_chain[level - 1]);
            let src = &self.mip_chain[level - 1];
            self.update_uniform(
                ctx,
                self.threshold,
                self.intensity,
                [1.0 / src.width() as f32, 1.0 / src.height() as f32],
                [0.0, 0.0],
            );
            let target = &self.mip_chain[level];
            self.run_single_input(ctx, downsample_pipeline.as_ref(), &bg, target, true);
        }

        // Blur (horizontal then vertical)
        for level in 0..BLOOM_LEVELS {
            let texel = [
                1.0 / self.mip_chain[level].width() as f32,
                1.0 / self.mip_chain[level].height() as f32,
            ];

            // Horizontal
            self.update_uniform(ctx, self.threshold, self.intensity, texel, [1.0, 0.0]);
            let h_bg = self.create_sample_bg(ctx, &self.mip_chain[level]);
            let temp = &self.temp_targets[level];
            self.run_single_input(ctx, blur_pipeline.as_ref(), &h_bg, temp, true);

            // Vertical
            self.update_uniform(ctx, self.threshold, self.intensity, texel, [0.0, 1.0]);
            let v_bg = self.create_sample_bg(ctx, &self.temp_targets[level]);
            let mip = &self.mip_chain[level];
            self.run_single_input(ctx, blur_pipeline.as_ref(), &v_bg, mip, true);
        }

        // Upsample
        for level in (1..BLOOM_LEVELS).rev() {
            let src = &self.mip_chain[level];
            self.update_uniform(
                ctx,
                self.threshold,
                self.intensity,
                [1.0 / src.width() as f32, 1.0 / src.height() as f32],
                [0.0, 0.0],
            );
            let bg = self.create_sample_bg(ctx, &self.mip_chain[level]);
            let target = &self.mip_chain[level - 1];
            self.run_single_input(ctx, upsample_pipeline.as_ref(), &bg, target, false);
        }

        // Combine
        self.update_uniform(ctx, self.threshold, self.intensity, [0.0, 0.0], [0.0, 0.0]);
        let combine_bg = ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bloom_combine_bg"),
            layout: &self.dual_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(input.view()),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(ctx.sampler_linear()),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(self.mip_chain[0].view()),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(ctx.sampler_linear()),
                },
            ],
        });
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
            label: Some("bloom_combine_pass"),
            color_attachments: &color_attachments,
            depth_stencil_attachment: None,
            ..Default::default()
        });
        pass.set_pipeline(combine_pipeline.as_ref());
        pass.set_bind_group(0, &combine_bg, &[]);
        pass.set_bind_group(1, &self.params_bind_group, &[]);
        FullscreenPass::draw(&mut pass);
    }
}

impl PostFx for Bloom {
    fn apply_to_target(
        &mut self,
        ctx: &mut GpuContext,
        input: &RenderTarget,
        output: &RenderTarget,
    ) {
        Bloom::apply(self, ctx, input, output);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_device() -> (wgpu::Device, wgpu::Queue) {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .expect("No suitable GPU adapter found for render tests");

        pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("render_test_device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
            },
            None,
        ))
        .expect("Failed to create test GPU device")
    }

    #[test]
    fn resize_reformats_internal_targets() {
        let (device, queue) = create_test_device();
        let ctx = crate::gpu::GpuContext::new_headless(
            device,
            queue,
            wgpu::TextureFormat::Bgra8Unorm,
            [64, 64],
        );
        let mut bloom = Bloom::new(&ctx, 64, 64, wgpu::TextureFormat::Rgba16Float);

        bloom.resize(&ctx, 64, 64, wgpu::TextureFormat::Rgba8Unorm);

        assert!(bloom
            .mip_chain
            .iter()
            .all(|target| target.format() == wgpu::TextureFormat::Rgba8Unorm));
        assert!(bloom
            .temp_targets
            .iter()
            .all(|target| target.format() == wgpu::TextureFormat::Rgba8Unorm));
    }
}
