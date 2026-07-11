//! Photon-style screen-space contact shadows and horizon AO.

use crate::gpu::GpuContext;
use crate::render::gpu::{FullscreenPass, FullscreenPipeline, RenderTarget};

const CONTACT_SHADOWS_SHADER: &str = include_str!("../../shaders/postfx/contact_shadows.wgsl");

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ContactShadowsUniform {
    projection: [f32; 16],
    inverse_projection: [f32; 16],
    light_direction_view: [f32; 4],
    params0: [f32; 4],
    params1: [f32; 4],
}

#[derive(Debug, Clone, Copy)]
pub struct ContactShadowsParams {
    pub projection: [f32; 16],
    pub inverse_projection: [f32; 16],
    pub light_direction_view: [f32; 3],
    pub temporal_seed: f32,
    pub intensity: f32,
    pub max_distance: f32,
    pub thickness: f32,
    pub ray_steps: u32,
    pub ao_intensity: f32,
    pub ao_radius_pixels: f32,
    pub ao_steps: u32,
}

impl Default for ContactShadowsParams {
    fn default() -> Self {
        Self {
            projection: [
                1.0, 0.0, 0.0, 0.0, //
                0.0, 1.0, 0.0, 0.0, //
                0.0, 0.0, 1.0, 0.0, //
                0.0, 0.0, 0.0, 1.0,
            ],
            inverse_projection: [
                1.0, 0.0, 0.0, 0.0, //
                0.0, 1.0, 0.0, 0.0, //
                0.0, 0.0, 1.0, 0.0, //
                0.0, 0.0, 0.0, 1.0,
            ],
            light_direction_view: [0.0, 0.0, -1.0],
            temporal_seed: 0.0,
            intensity: 0.72,
            max_distance: 2.6,
            thickness: 0.18,
            ray_steps: 10,
            ao_intensity: 0.46,
            ao_radius_pixels: 18.0,
            ao_steps: 3,
        }
    }
}

pub struct ContactShadows {
    pipeline: FullscreenPipeline,
    texture_bgl: wgpu::BindGroupLayout,
    uniform_bgl: wgpu::BindGroupLayout,
    uniform_buffer: wgpu::Buffer,
}

impl ContactShadows {
    pub fn new(ctx: &GpuContext, target_format: wgpu::TextureFormat) -> Self {
        let texture_bgl = ctx
            .device()
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("contact_shadows_texture_bgl"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: false },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Depth,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: false },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                ],
            });

        let uniform_bgl = ctx
            .device()
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("contact_shadows_uniform_bgl"),
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

        let uniform_buffer = ctx.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("contact_shadows_uniform_buffer"),
            size: std::mem::size_of::<ContactShadowsUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let pipeline = FullscreenPipeline::new(
            ctx,
            CONTACT_SHADOWS_SHADER,
            "fs_main",
            &[&texture_bgl, &uniform_bgl],
            target_format,
            None,
            "contact_shadows_pipeline",
        );

        Self {
            pipeline,
            texture_bgl,
            uniform_bgl,
            uniform_buffer,
        }
    }

    pub fn apply_to_target(
        &mut self,
        ctx: &mut GpuContext,
        input: &RenderTarget,
        depth: &RenderTarget,
        normal: &RenderTarget,
        output: &RenderTarget,
        params: ContactShadowsParams,
    ) {
        let uniform = ContactShadowsUniform {
            projection: params.projection,
            inverse_projection: params.inverse_projection,
            light_direction_view: [
                params.light_direction_view[0],
                params.light_direction_view[1],
                params.light_direction_view[2],
                0.0,
            ],
            params0: [
                params.intensity.max(0.0),
                params.max_distance.max(0.001),
                params.thickness.max(0.001),
                params.ray_steps.max(1) as f32,
            ],
            params1: [
                params.ao_intensity.max(0.0),
                params.ao_radius_pixels.max(1.0),
                params.ao_steps.max(1) as f32,
                params.temporal_seed,
            ],
        };
        ctx.queue()
            .write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniform));

        let texture_bg = ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("contact_shadows_texture_bg"),
            layout: &self.texture_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(input.view()),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(depth.view()),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(normal.view()),
                },
            ],
        });
        let uniform_bg = ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("contact_shadows_uniform_bg"),
            layout: &self.uniform_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: self.uniform_buffer.as_entire_binding(),
            }],
        });
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
            label: Some("contact_shadows_pass"),
            color_attachments: &color_attachments,
            depth_stencil_attachment: None,
            ..Default::default()
        });
        pass.set_pipeline(pipeline.as_ref());
        pass.set_bind_group(0, &texture_bg, &[]);
        pass.set_bind_group(1, &uniform_bg, &[]);
        FullscreenPass::draw(&mut pass);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_device() -> (wgpu::Device, wgpu::Queue) {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .expect("No suitable GPU adapter found for contact-shadow tests");

        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("contact_shadows_test_device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::Performance,
            ..Default::default()
        }))
        .expect("Failed to create contact-shadow test GPU device")
    }

    #[test]
    fn contact_shadows_wgsl_source_validates() {
        let (device, _queue) = create_test_device();
        let error_scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let _module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("contact_shadows_shader_test"),
            source: wgpu::ShaderSource::Wgsl(crate::render::gpu::compose_fullscreen_shader(
                CONTACT_SHADOWS_SHADER,
            )),
        });
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
        let error = pollster::block_on(error_scope.pop());
        assert!(
            error.is_none(),
            "contact-shadow shader should validate: {error:?}"
        );
    }

    #[test]
    fn contact_shadows_use_dithered_geometric_ray_steps() {
        assert!(CONTACT_SHADOWS_SHADER.contains("const SSRT_STEP_RATIO"));
        assert!(CONTACT_SHADOWS_SHADER.contains("segment_jitter"));
        assert!(
            CONTACT_SHADOWS_SHADER.contains("segment_length = segment_length * SSRT_STEP_RATIO")
        );
        assert!(
            !CONTACT_SHADOWS_SHADER.contains("let ray_t = step_phase * step_phase;"),
            "fixed quadratic ray steps create visible contact-shadow bands"
        );
    }
}
