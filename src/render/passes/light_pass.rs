//! 2D instanced light accumulation pass.

use std::sync::Arc;

use rustc_hash::FxHashMap;

use crate::gpu::GpuContext;
use crate::render::core::camera::{CameraUniform, RenderView};
use crate::render::core::target::RenderTarget;
use crate::render::core::texture::Texture;
use crate::render::light::Light2D;

const MAX_LIGHTS: usize = 4096;

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct LightInstance {
    pub(crate) pos_radius: [f32; 4],
    pub(crate) color: [f32; 4],
    pub(crate) falloff: [f32; 4],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct QuadVertex {
    pos: [f32; 2],
}

const QUAD_VERTICES: [QuadVertex; 4] = [
    QuadVertex { pos: [0.0, 0.0] },
    QuadVertex { pos: [1.0, 0.0] },
    QuadVertex { pos: [1.0, 1.0] },
    QuadVertex { pos: [0.0, 1.0] },
];

const QUAD_INDICES: [u16; 6] = [0, 1, 2, 0, 2, 3];

/// Instanced additive light accumulation.
pub struct LightPass {
    shader: Arc<wgpu::ShaderModule>,
    pipelines: FxHashMap<wgpu::TextureFormat, Arc<wgpu::RenderPipeline>>,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    instance_buffer: wgpu::Buffer,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    camera_bgl: wgpu::BindGroupLayout,
    normal_bgl: wgpu::BindGroupLayout,
    flat_normal: Texture,
}

impl LightPass {
    fn targets_alias(lhs: &RenderTarget, rhs: &RenderTarget) -> bool {
        std::ptr::eq(lhs.texture(), rhs.texture())
    }

    fn validate_targets(normal_target: Option<&RenderTarget>, lightmap: &RenderTarget) {
        if let Some(normal_target) = normal_target {
            assert!(
                !Self::targets_alias(normal_target, lightmap),
                "LightPass requires distinct normal and lightmap targets",
            );
        }
    }

    fn create_pipeline(
        &self,
        ctx: &GpuContext,
        target_format: wgpu::TextureFormat,
    ) -> wgpu::RenderPipeline {
        let pipeline_layout =
            ctx.device()
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("light_pass_layout"),
                    bind_group_layouts: &[&self.camera_bgl, &self.normal_bgl],
                    push_constant_ranges: &[],
                });

        ctx.device()
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(&format!("light_pass_pipeline_{target_format:?}")),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &self.shader,
                    entry_point: Some("vs_main"),
                    buffers: &[
                        wgpu::VertexBufferLayout {
                            array_stride: std::mem::size_of::<QuadVertex>() as u64,
                            step_mode: wgpu::VertexStepMode::Vertex,
                            attributes: &[wgpu::VertexAttribute {
                                offset: 0,
                                shader_location: 0,
                                format: wgpu::VertexFormat::Float32x2,
                            }],
                        },
                        wgpu::VertexBufferLayout {
                            array_stride: std::mem::size_of::<LightInstance>() as u64,
                            step_mode: wgpu::VertexStepMode::Instance,
                            attributes: &[
                                wgpu::VertexAttribute {
                                    offset: 0,
                                    shader_location: 1,
                                    format: wgpu::VertexFormat::Float32x4,
                                },
                                wgpu::VertexAttribute {
                                    offset: 16,
                                    shader_location: 2,
                                    format: wgpu::VertexFormat::Float32x4,
                                },
                                wgpu::VertexAttribute {
                                    offset: 32,
                                    shader_location: 3,
                                    format: wgpu::VertexFormat::Float32x4,
                                },
                            ],
                        },
                    ],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &self.shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: target_format,
                        blend: Some(wgpu::BlendState {
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
                        }),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            })
    }

    fn pipeline_for(
        &mut self,
        ctx: &GpuContext,
        target_format: wgpu::TextureFormat,
    ) -> Arc<wgpu::RenderPipeline> {
        if let Some(existing) = self.pipelines.get(&target_format) {
            return existing.clone();
        }
        let pipeline = Arc::new(self.create_pipeline(ctx, target_format));
        self.pipelines.insert(target_format, pipeline.clone());
        pipeline
    }

    pub fn new(ctx: &GpuContext, target_format: wgpu::TextureFormat) -> Self {
        let shader = ctx
            .device()
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("light_pass_shader"),
                source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/light.wgsl").into()),
            });

        // Buffers
        let vertex_buffer = ctx.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("light_quad_vb"),
            size: std::mem::size_of_val(&QUAD_VERTICES) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        ctx.queue()
            .write_buffer(&vertex_buffer, 0, bytemuck::cast_slice(&QUAD_VERTICES));

        let index_buffer = ctx.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("light_quad_ib"),
            size: std::mem::size_of_val(&QUAD_INDICES) as u64,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        ctx.queue()
            .write_buffer(&index_buffer, 0, bytemuck::cast_slice(&QUAD_INDICES));

        let instance_buffer = ctx.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("light_instance_buf"),
            size: (MAX_LIGHTS * std::mem::size_of::<LightInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let camera_buffer = ctx.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("light_camera_buf"),
            size: std::mem::size_of::<CameraUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Bind group layouts
        let camera_bgl = ctx
            .device()
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("light_camera_bgl"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let normal_bgl = ctx
            .device()
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("light_normal_bgl"),
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

        let camera_bind_group = ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("light_camera_bg"),
            layout: &camera_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });
        let mut this = Self {
            shader: Arc::new(shader),
            pipelines: FxHashMap::default(),
            vertex_buffer,
            index_buffer,
            instance_buffer,
            camera_buffer,
            camera_bind_group,
            camera_bgl,
            normal_bgl,
            flat_normal: Texture::flat_normal(ctx),
        };
        let pipeline = Arc::new(this.create_pipeline(ctx, target_format));
        this.pipelines.insert(target_format, pipeline);
        this
    }

    pub fn render(
        &mut self,
        ctx: &mut GpuContext,
        lights: &[Light2D],
        normal_target: Option<&RenderTarget>,
        lightmap: &RenderTarget,
        view: &impl RenderView,
        ambient: [f32; 4],
    ) {
        Self::validate_targets(normal_target, lightmap);

        let pipeline = self.pipeline_for(ctx, lightmap.format());
        ctx.queue().write_buffer(
            &self.camera_buffer,
            0,
            bytemuck::bytes_of(&view.view_uniform()),
        );

        let capped_lights = if lights.len() > MAX_LIGHTS {
            eprintln!(
                "[SkyEngine] LightPass: {} lights exceed MAX_LIGHTS ({}), truncating",
                lights.len(),
                MAX_LIGHTS
            );
            &lights[..MAX_LIGHTS]
        } else {
            lights
        };

        let instances: Vec<LightInstance> = capped_lights
            .iter()
            .map(|light| LightInstance {
                pos_radius: [light.position[0], light.position[1], light.radius, 0.0],
                color: light.effective_color(),
                falloff: [light.falloff.max(0.001), 50.0, 0.0, 0.0],
            })
            .collect();

        if !instances.is_empty() {
            ctx.queue()
                .write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(&instances));
        }

        // Create normal bind group
        let normal_view = match normal_target {
            Some(target) => target.view(),
            None => self.flat_normal.view(),
        };
        let normal_bg = ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("light_normal_bg"),
            layout: &self.normal_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(normal_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(ctx.sampler_linear()),
                },
            ],
        });

        let instance_count = instances.len() as u32;
        ctx.with_render_pass(
            &wgpu::RenderPassDescriptor {
                label: Some("light_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: lightmap.view(),
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: ambient[0] as f64,
                            g: ambient[1] as f64,
                            b: ambient[2] as f64,
                            a: ambient[3] as f64,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                ..Default::default()
            },
            |pass| {
                pass.set_pipeline(pipeline.as_ref());
                pass.set_bind_group(0, &self.camera_bind_group, &[]);
                pass.set_bind_group(1, &normal_bg, &[]);
                pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
                pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
                pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                if instance_count > 0 {
                    pass.draw_indexed(0..6, 0, 0..instance_count);
                }
            },
        );
    }

    #[cfg(test)]
    fn pipeline_count(&self) -> usize {
        self.pipelines.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::core::target::RenderTarget;
    use std::panic::{self, AssertUnwindSafe};

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
    fn light_pass_caches_pipelines_per_target_format() {
        let (device, queue) = create_test_device();
        let ctx = crate::gpu::GpuContext::new_headless(
            device,
            queue,
            wgpu::TextureFormat::Bgra8Unorm,
            [4, 4],
        );
        let mut pass = LightPass::new(&ctx, wgpu::TextureFormat::Rgba16Float);

        assert_eq!(pass.pipeline_count(), 1);
        let _ = pass.pipeline_for(&ctx, wgpu::TextureFormat::Rgba8Unorm);
        assert_eq!(pass.pipeline_count(), 2);
    }

    #[test]
    fn light_pass_rejects_aliasing_normal_and_lightmap_targets() {
        let (device, queue) = create_test_device();
        let mut ctx = crate::gpu::GpuContext::new_headless(
            device,
            queue,
            wgpu::TextureFormat::Bgra8Unorm,
            [4, 4],
        );
        let mut pass = LightPass::new(&ctx, wgpu::TextureFormat::Rgba16Float);
        let target = RenderTarget::new(&ctx, 4, 4, wgpu::TextureFormat::Rgba16Float, "shared");
        let camera = crate::render::core::camera::Camera2D::new(4.0, 4.0);

        ctx.begin_frame()
            .expect("headless begin_frame should succeed");
        let result = panic::catch_unwind(AssertUnwindSafe(|| {
            pass.render(
                &mut ctx,
                &[],
                Some(&target),
                &target,
                &camera,
                [0.0, 0.0, 0.0, 1.0],
            );
        }));
        ctx.end_frame();

        assert!(result.is_err());
    }
}
