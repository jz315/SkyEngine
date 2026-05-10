//! 2D instanced light accumulation pass.

use std::sync::Arc;

use crate::gpu::GpuContext;
use crate::render::gpu::helpers::{
    create_position_quad_geometry, BindGroupCache, CameraBinding, QuadGeometry, RenderPipelineCache,
};
use crate::render::gpu::RenderTarget;
use crate::render::gpu::Texture;
use crate::render::lighting::Light2D;
use crate::render::view::RenderView;

const MAX_LIGHTS: usize = 4096;

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct LightInstance {
    pub(crate) pos_radius: [f32; 4],
    pub(crate) color: [f32; 4],
    pub(crate) falloff: [f32; 4],
}

/// Instanced additive light accumulation.
pub struct LightPass {
    shader: Arc<wgpu::ShaderModule>,
    pipelines: RenderPipelineCache<wgpu::TextureFormat>,
    quad: QuadGeometry,
    instance_buffer: wgpu::Buffer,
    camera: CameraBinding,
    normal_bgl: wgpu::BindGroupLayout,
    _flat_normal: Texture,
    flat_normal_bind_group: wgpu::BindGroup,
    normal_bind_group_cache: BindGroupCache<usize>,
    instances_scratch: Vec<LightInstance>,
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

    pub fn new(ctx: &GpuContext, target_format: wgpu::TextureFormat) -> Self {
        let shader = Arc::new(
            ctx.device()
                .create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("light_pass_shader"),
                    source: wgpu::ShaderSource::Wgsl(
                        include_str!("../shaders/lighting/light.wgsl").into(),
                    ),
                }),
        );

        let quad = create_position_quad_geometry(ctx, "light");
        let instance_buffer = ctx.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("light_instance_buf"),
            size: (MAX_LIGHTS * std::mem::size_of::<LightInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let camera = CameraBinding::new(ctx, "light");

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

        let flat_normal = Texture::flat_normal(ctx);
        let flat_normal_bind_group = ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("light_normal_bg"),
            layout: &normal_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(flat_normal.view()),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(ctx.sampler_linear()),
                },
            ],
        });

        let mut this = Self {
            shader,
            pipelines: RenderPipelineCache::new(),
            quad,
            instance_buffer,
            camera,
            normal_bgl,
            _flat_normal: flat_normal,
            flat_normal_bind_group,
            normal_bind_group_cache: BindGroupCache::new(),
            instances_scratch: Vec::with_capacity(256),
        };
        let _ = this.pipeline_for(ctx, target_format);
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
        self.camera.upload_view(ctx, view);

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

        self.instances_scratch.clear();
        self.instances_scratch
            .extend(capped_lights.iter().map(|light| LightInstance {
                pos_radius: [light.position[0], light.position[1], light.radius, 0.0],
                color: light.effective_color(),
                falloff: [light.falloff.max(0.001), 50.0, 0.0, 0.0],
            }));

        if !self.instances_scratch.is_empty() {
            ctx.queue().write_buffer(
                &self.instance_buffer,
                0,
                bytemuck::cast_slice(&self.instances_scratch),
            );
        }

        let normal_bg = self.resolve_normal_bind_group(ctx, normal_target).clone();
        let instance_count = self.instances_scratch.len() as u32;
        let color_attachments = [Some(wgpu::RenderPassColorAttachment {
            view: lightmap.view(),
            depth_slice: None,
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
        })];
        let mut frame = ctx.frame();
        let mut pass = frame.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("light_pass"),
            color_attachments: &color_attachments,
            depth_stencil_attachment: None,
            ..Default::default()
        });
        pass.set_pipeline(pipeline.as_ref());
        pass.set_bind_group(0, &self.camera.bind_group, &[]);
        pass.set_bind_group(1, &normal_bg, &[]);
        pass.set_vertex_buffer(0, self.quad.vertex_buffer.slice(..));
        pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
        pass.set_index_buffer(self.quad.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        if instance_count > 0 {
            pass.draw_indexed(0..6, 0, 0..instance_count);
        }
    }

    fn resolve_normal_bind_group(
        &mut self,
        ctx: &GpuContext,
        normal_target: Option<&RenderTarget>,
    ) -> &wgpu::BindGroup {
        let Some(target) = normal_target else {
            return &self.flat_normal_bind_group;
        };

        let key = std::ptr::from_ref(target.texture()) as usize;
        let normal_bgl = self.normal_bgl.clone();
        let target_view = target.view();
        let sampler = ctx.sampler_linear();
        self.normal_bind_group_cache.get_or_create(key, || {
            ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("light_normal_bg"),
                layout: &normal_bgl,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(target_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(sampler),
                    },
                ],
            })
        })
    }

    fn pipeline_for(
        &mut self,
        ctx: &GpuContext,
        target_format: wgpu::TextureFormat,
    ) -> Arc<wgpu::RenderPipeline> {
        let shader = self.shader.clone();
        let camera_bgl = self.camera.layout.clone();
        let normal_bgl = self.normal_bgl.clone();
        self.pipelines.get_or_create(target_format, || {
            let pipeline_layout =
                ctx.device()
                    .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                        label: Some("light_pass_layout"),
                        bind_group_layouts: &[Some(&camera_bgl), Some(&normal_bgl)],
                        immediate_size: 0,
                    });

            ctx.device()
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some(&format!("light_pass_pipeline_{target_format:?}")),
                    layout: Some(&pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: shader.as_ref(),
                        entry_point: Some("vs_main"),
                        buffers: &[
                            wgpu::VertexBufferLayout {
                                array_stride: 8,
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
                        module: shader.as_ref(),
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
                    multiview_mask: None,
                    cache: None,
                })
        })
    }

    #[cfg(test)]
    fn pipeline_count(&self) -> usize {
        self.pipelines.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::gpu::RenderTarget;
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
        let camera = crate::render::view::Camera::new(4.0, 4.0);

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
