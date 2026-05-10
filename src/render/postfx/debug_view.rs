//! Intermediate-buffer debug view.

use crate::gpu::GpuContext;
use crate::render::gpu::{FullscreenPass, FullscreenPipeline, RenderTarget};

const DEBUG_VIEW_SHADER: &str = include_str!("../shaders/postfx/debug_view.wgsl");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum DebugViewMode {
    SceneDepth = 0,
    SceneNormal = 1,
    SourceRgb = 2,
    Roughness = 3,
    Metallic = 4,
    Velocity = 5,
    ShadowDepth = 6,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct DebugViewUniform {
    params: [u32; 4],
    atlas_mul_add: [f32; 4],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DebugViewParams {
    pub atlas_slice: u32,
    pub atlas_slice_count: u32,
    pub atlas_mul_add: [f32; 4],
}

impl DebugViewParams {
    #[inline]
    pub const fn full() -> Self {
        Self {
            atlas_slice: 0,
            atlas_slice_count: 0,
            atlas_mul_add: [1.0, 1.0, 0.0, 0.0],
        }
    }

    #[inline]
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn atlas_slice(slice: u32, slice_count: u32) -> Self {
        let slice_count = slice_count.max(1);
        Self::atlas_slice_with_mul_add(
            slice,
            slice_count,
            [(slice_count as f32).recip(), 1.0, 0.0, 0.0],
        )
    }

    #[inline]
    pub fn atlas_slice_with_mul_add(slice: u32, slice_count: u32, atlas_mul_add: [f32; 4]) -> Self {
        let slice_count = slice_count.max(1);
        Self {
            atlas_slice: slice.min(slice_count - 1),
            atlas_slice_count: slice_count,
            atlas_mul_add,
        }
    }
}

impl Default for DebugViewParams {
    fn default() -> Self {
        Self::full()
    }
}

pub struct DebugView {
    pipeline: FullscreenPipeline,
    texture_bgl: wgpu::BindGroupLayout,
    uniform_bgl: wgpu::BindGroupLayout,
    uniform_buffer: wgpu::Buffer,
}

impl DebugView {
    pub fn new(ctx: &GpuContext, target_format: wgpu::TextureFormat) -> Self {
        let texture_bgl = ctx
            .device()
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("debug_view_texture_bgl"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Depth,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
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
                label: Some("debug_view_uniform_bgl"),
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
            label: Some("debug_view_uniform_buffer"),
            size: std::mem::size_of::<DebugViewUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let pipeline = FullscreenPipeline::new(
            ctx,
            DEBUG_VIEW_SHADER,
            "fs_main",
            &[&texture_bgl, &uniform_bgl],
            target_format,
            None,
            "debug_view_pipeline",
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
        depth: &wgpu::TextureView,
        source: &wgpu::TextureView,
        output: &RenderTarget,
        mode: DebugViewMode,
        params: DebugViewParams,
    ) {
        let uniform = DebugViewUniform {
            params: [mode as u32, params.atlas_slice, params.atlas_slice_count, 0],
            atlas_mul_add: params.atlas_mul_add,
        };
        ctx.queue()
            .write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniform));

        let textures_bg = ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("debug_view_texture_bg"),
            layout: &self.texture_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(depth),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(source),
                },
            ],
        });
        let uniform_bg = ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("debug_view_uniform_bg"),
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
            label: Some("debug_view_pass"),
            color_attachments: &color_attachments,
            depth_stencil_attachment: None,
            ..Default::default()
        });
        pass.set_pipeline(pipeline.as_ref());
        pass.set_bind_group(0, &textures_bg, &[]);
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
        .expect("No suitable GPU adapter found for debug-view tests");

        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("debug_view_test_device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::Performance,
            ..Default::default()
        }))
        .expect("Failed to create test GPU device")
    }

    #[test]
    fn debug_view_wgsl_source_validates() {
        let (device, _queue) = create_test_device();
        let error_scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let _module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("debug_view_shader_test"),
            source: wgpu::ShaderSource::Wgsl(crate::render::gpu::compose_fullscreen_shader(
                DEBUG_VIEW_SHADER,
            )),
        });
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
        let error = pollster::block_on(error_scope.pop());
        assert!(
            error.is_none(),
            "debug-view shader should validate: {error:?}"
        );
    }

    #[test]
    fn debug_view_params_clamp_to_wicked_atlas_slice_range() {
        assert_eq!(
            DebugViewParams::atlas_slice(5, 4),
            DebugViewParams {
                atlas_slice: 3,
                atlas_slice_count: 4,
                atlas_mul_add: [0.25, 1.0, 0.0, 0.0],
            }
        );
        assert_eq!(
            DebugViewParams::atlas_slice(0, 0),
            DebugViewParams {
                atlas_slice: 0,
                atlas_slice_count: 1,
                atlas_mul_add: [1.0, 1.0, 0.0, 0.0],
            }
        );
    }

    #[test]
    fn debug_view_params_can_crop_packed_shadow_rect() {
        assert_eq!(
            DebugViewParams::atlas_slice_with_mul_add(2, 4, [0.125, 0.5, 0.25, 0.5]),
            DebugViewParams {
                atlas_slice: 2,
                atlas_slice_count: 4,
                atlas_mul_add: [0.125, 0.5, 0.25, 0.5],
            }
        );
    }
}
