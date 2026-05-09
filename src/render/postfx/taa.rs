//! WickedEngine-inspired temporal anti-aliasing resolve.

use crate::gpu::GpuContext;
use crate::render::gpu::{ComputePipelineCache, RenderTarget};

const TAA_SHADER: &str = include_str!("../shaders/postfx/taa.wgsl");
pub const TAA_WORKGROUP_SIZE: u32 = 8;

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct TaaUniform {
    resolution: [f32; 4],
    params0: [f32; 4],
    params1: [f32; 4],
}

#[derive(Debug, Clone, Copy)]
pub struct TemporalAntiAliasingParams {
    pub reset: bool,
    pub feedback: f32,
    pub history_clamp: f32,
    pub jitter: [f32; 2],
    pub previous_jitter: [f32; 2],
    pub near: f32,
    pub far: f32,
}

impl Default for TemporalAntiAliasingParams {
    fn default() -> Self {
        Self {
            reset: true,
            feedback: 0.05,
            history_clamp: 0.0,
            jitter: [0.0, 0.0],
            previous_jitter: [0.0, 0.0],
            near: 0.1,
            far: 1000.0,
        }
    }
}

pub struct TemporalAntiAliasing {
    input_bgl: wgpu::BindGroupLayout,
    uniform_bgl: wgpu::BindGroupLayout,
    output_bgl: wgpu::BindGroupLayout,
    uniform_buffer: wgpu::Buffer,
    pipeline: ComputePipelineCache,
}

impl TemporalAntiAliasing {
    pub fn new(ctx: &GpuContext) -> Self {
        let input_bgl = ctx
            .device()
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("taa_input_bgl"),
                entries: &[
                    sampled_texture_entry(0, wgpu::TextureSampleType::Float { filterable: false }),
                    sampled_texture_entry(1, wgpu::TextureSampleType::Float { filterable: true }),
                    sampled_texture_entry(2, wgpu::TextureSampleType::Depth),
                    sampled_texture_entry(3, wgpu::TextureSampleType::Depth),
                    sampled_texture_entry(4, wgpu::TextureSampleType::Float { filterable: false }),
                    wgpu::BindGroupLayoutEntry {
                        binding: 5,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });
        let uniform_bgl = ctx
            .device()
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("taa_uniform_bgl"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });
        let output_bgl = ctx
            .device()
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("taa_output_bgl"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba16Float,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                }],
            });
        let uniform_buffer = ctx.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("taa_uniform_buffer"),
            size: std::mem::size_of::<TaaUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let pipeline = ComputePipelineCache::new(
            ctx,
            TAA_SHADER,
            "cs_main",
            &[&input_bgl, &uniform_bgl, &output_bgl],
            "taa_pipeline",
        );

        Self {
            input_bgl,
            uniform_bgl,
            output_bgl,
            uniform_buffer,
            pipeline,
        }
    }

    pub fn apply_to_target(
        &mut self,
        ctx: &mut GpuContext,
        input: &RenderTarget,
        history: &wgpu::TextureView,
        depth: &RenderTarget,
        depth_history: &wgpu::TextureView,
        velocity: &RenderTarget,
        output: &RenderTarget,
        params: TemporalAntiAliasingParams,
    ) {
        let width = output.width().max(1);
        let height = output.height().max(1);
        let jitter_velocity_uv = [
            0.5 * (params.previous_jitter[0] - params.jitter[0]),
            -0.5 * (params.previous_jitter[1] - params.jitter[1]),
        ];
        let uniform = TaaUniform {
            resolution: [
                width as f32,
                height as f32,
                1.0 / width as f32,
                1.0 / height as f32,
            ],
            params0: [
                if params.reset { 1.0 } else { 0.0 },
                params.feedback.clamp(0.0, 1.0),
                0.95,
                params.history_clamp.max(0.0),
            ],
            params1: [
                jitter_velocity_uv[0],
                params.near.max(0.0001),
                params.far.max(params.near + 0.0001),
                jitter_velocity_uv[1],
            ],
        };
        ctx.queue()
            .write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniform));

        let input_bg = ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("taa_input_bg"),
            layout: &self.input_bgl,
            entries: &[
                texture_view_binding(0, input.view()),
                texture_view_binding(1, history),
                texture_view_binding(2, depth.view()),
                texture_view_binding(3, depth_history),
                texture_view_binding(4, velocity.view()),
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::Sampler(ctx.sampler_linear()),
                },
            ],
        });
        let uniform_bg = ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("taa_uniform_bg"),
            layout: &self.uniform_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: self.uniform_buffer.as_entire_binding(),
            }],
        });
        let output_bg = ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("taa_output_bg"),
            layout: &self.output_bgl,
            entries: &[texture_view_binding(0, output.view())],
        });
        let pipeline = self.pipeline.pipeline(ctx);

        let mut frame = ctx.frame();
        let mut pass = frame.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("taa_pass"),
            ..Default::default()
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &input_bg, &[]);
        pass.set_bind_group(1, &uniform_bg, &[]);
        pass.set_bind_group(2, &output_bg, &[]);
        pass.dispatch_workgroups(
            width.div_ceil(TAA_WORKGROUP_SIZE),
            height.div_ceil(TAA_WORKGROUP_SIZE),
            1,
        );
    }
}

fn sampled_texture_entry(
    binding: u32,
    sample_type: wgpu::TextureSampleType,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Texture {
            sample_type,
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

fn texture_view_binding<'a>(binding: u32, view: &'a wgpu::TextureView) -> wgpu::BindGroupEntry<'a> {
    wgpu::BindGroupEntry {
        binding,
        resource: wgpu::BindingResource::TextureView(view),
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
        .expect("No suitable GPU adapter found for TAA tests");

        pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("taa_test_device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
            },
            None,
        ))
        .expect("Failed to create test GPU device")
    }

    #[test]
    fn taa_wgsl_source_validates() {
        let (device, _queue) = create_test_device();
        device.push_error_scope(wgpu::ErrorFilter::Validation);
        let _module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("taa_shader_test"),
            source: wgpu::ShaderSource::Wgsl(TAA_SHADER.into()),
        });
        device.poll(wgpu::Maintain::Wait);
        let error = pollster::block_on(device.pop_error_scope());
        assert!(error.is_none(), "TAA shader should validate: {error:?}");
    }

    #[test]
    fn taa_tile_cache_loader_covers_full_neighborhood() {
        const TILE_SIZE: usize = 10;
        const THREADCOUNT: usize = TAA_WORKGROUP_SIZE as usize;

        let mut covered = [[false; TILE_SIZE]; TILE_SIZE];
        for lid_y in 0..THREADCOUNT {
            for lid_x in 0..THREADCOUNT {
                let mut y = lid_y;
                while y < TILE_SIZE {
                    let mut x = lid_x;
                    while x < TILE_SIZE {
                        covered[y][x] = true;
                        x += THREADCOUNT;
                    }
                    y += THREADCOUNT;
                }
            }
        }

        for (y, row) in covered.iter().enumerate() {
            for (x, is_covered) in row.iter().copied().enumerate() {
                assert!(is_covered, "TAA tile cache missed ({x}, {y})");
            }
        }
    }

    #[test]
    fn taa_shader_uses_pixel_space_disocclusion_threshold() {
        assert!(
            TAA_SHADER.contains("stable_velocity_pixels > 0.25"),
            "TAA disocclusion should use pixel-space velocity, not raw UV length"
        );
        assert!(
            !TAA_SHADER.contains("length(stable_velocity) > 0.01"),
            "raw UV velocity threshold is resolution-dependent and causes ghosting"
        );
    }
}
