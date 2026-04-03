//! 2D instanced light accumulation pass.

use std::borrow::Cow;

use rustc_hash::FxHashMap;

use crate::gpu::{
    BindGroup, BindGroupDesc, BindGroupEntry, BindGroupLayout, BindGroupLayoutDesc, BindingType,
    Buffer, BufferDesc, BufferUsage, ColorAttachment, ColorTarget, ColorTargetState, Gpu, Pipeline,
    PrimitiveState, RenderPassDesc, RenderPipelineDesc, ShaderDesc, ShaderStages, TextureFormat,
    VertexAttribute, VertexBufferLayout, VertexFormat, VertexStepMode,
};
use crate::render::camera::{Camera2D, CameraUniform};
use crate::render::light::Light2D;
use crate::render::target::RenderTarget;
use crate::render::Texture;

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct LightInstance {
    pos_radius: [f32; 4],
    color: [f32; 4],
    falloff: [f32; 4],
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
    _shader: crate::gpu::Shader,
    pipeline: Pipeline,
    vertex_buffer: Buffer,
    index_buffer: Buffer,
    instance_buffer: Buffer,
    camera_buffer: Buffer,
    camera_bind_group: BindGroup,
    camera_bgl: BindGroupLayout,
    normal_bgl: BindGroupLayout,
    normal_bind_groups: FxHashMap<(crate::gpu::Image, crate::gpu::Sampler), BindGroup>,
    flat_normal: Texture,
}

impl LightPass {
    pub fn new(gpu: &mut impl Gpu, target_format: TextureFormat) -> Self {
        let shader = gpu.create_shader(&ShaderDesc {
            label: Cow::Borrowed("light_pass_shader"),
            source: Cow::Borrowed(include_str!("shaders/light.wgsl")),
        });

        let vertex_buffer = gpu.create_buffer(&BufferDesc {
            label: Cow::Borrowed("light_quad_vb"),
            size: std::mem::size_of_val(&QUAD_VERTICES) as u64,
            usage: BufferUsage::VERTEX,
        });
        gpu.write_buffer(vertex_buffer, 0, bytemuck::cast_slice(&QUAD_VERTICES));

        let index_buffer = gpu.create_buffer(&BufferDesc {
            label: Cow::Borrowed("light_quad_ib"),
            size: std::mem::size_of_val(&QUAD_INDICES) as u64,
            usage: BufferUsage::INDEX,
        });
        gpu.write_buffer(index_buffer, 0, bytemuck::cast_slice(&QUAD_INDICES));

        let instance_buffer = gpu.create_buffer(&BufferDesc {
            label: Cow::Borrowed("light_instance_buf"),
            size: (4096 * std::mem::size_of::<LightInstance>()) as u64,
            usage: BufferUsage::VERTEX | BufferUsage::COPY_DST,
        });

        let camera_buffer = gpu.create_buffer(&BufferDesc {
            label: Cow::Borrowed("light_camera_buf"),
            size: std::mem::size_of::<CameraUniform>() as u64,
            usage: BufferUsage::UNIFORM | BufferUsage::COPY_DST,
        });

        let camera_bgl = gpu.create_bind_group_layout(&BindGroupLayoutDesc {
            label: Cow::Borrowed("light_camera_bgl"),
            entries: vec![crate::gpu::BindGroupLayoutEntry {
                binding: 0,
                ty: BindingType::UniformBuffer,
                visibility: ShaderStages::VERTEX_FRAGMENT,
            }],
        });
        let normal_bgl = gpu.create_bind_group_layout(&BindGroupLayoutDesc {
            label: Cow::Borrowed("light_normal_bgl"),
            entries: vec![
                crate::gpu::BindGroupLayoutEntry {
                    binding: 0,
                    ty: BindingType::Texture,
                    visibility: ShaderStages::FRAGMENT,
                },
                crate::gpu::BindGroupLayoutEntry {
                    binding: 1,
                    ty: BindingType::Sampler,
                    visibility: ShaderStages::FRAGMENT,
                },
            ],
        });
        let camera_bind_group = gpu.create_bind_group(&BindGroupDesc {
            label: Cow::Borrowed("light_camera_bg"),
            layout: camera_bgl,
            entries: vec![BindGroupEntry::Buffer {
                binding: 0,
                buffer: camera_buffer,
                offset: 0,
                size: std::mem::size_of::<CameraUniform>() as u64,
            }],
        });

        let pipeline = gpu.create_render_pipeline(&RenderPipelineDesc {
            label: Cow::Borrowed("light_pass_pipeline"),
            shader,
            vs_entry: "vs_main",
            fs_entry: "fs_main",
            vertex_layouts: vec![
                VertexBufferLayout {
                    stride: std::mem::size_of::<QuadVertex>() as u64,
                    step_mode: VertexStepMode::Vertex,
                    attributes: vec![VertexAttribute {
                        offset: 0,
                        shader_location: 0,
                        format: VertexFormat::Float32x2,
                    }],
                },
                VertexBufferLayout {
                    stride: std::mem::size_of::<LightInstance>() as u64,
                    step_mode: VertexStepMode::Instance,
                    attributes: vec![
                        VertexAttribute {
                            offset: 0,
                            shader_location: 1,
                            format: VertexFormat::Float32x4,
                        },
                        VertexAttribute {
                            offset: 16,
                            shader_location: 2,
                            format: VertexFormat::Float32x4,
                        },
                        VertexAttribute {
                            offset: 32,
                            shader_location: 3,
                            format: VertexFormat::Float32x4,
                        },
                    ],
                },
            ],
            bind_group_layouts: vec![camera_bgl, normal_bgl],
            color_targets: vec![ColorTargetState {
                format: target_format,
                blend: Some(crate::gpu::BlendState::ADDITIVE),
            }],
            depth_stencil: None,
            primitive: PrimitiveState::default(),
        });

        Self {
            _shader: shader,
            pipeline,
            vertex_buffer,
            index_buffer,
            instance_buffer,
            camera_buffer,
            camera_bind_group,
            camera_bgl,
            normal_bgl,
            normal_bind_groups: FxHashMap::default(),
            flat_normal: Texture::flat_normal(gpu),
        }
    }

    pub fn render(
        &mut self,
        gpu: &mut impl Gpu,
        lights: &[Light2D],
        normal_target: Option<&RenderTarget>,
        lightmap: &RenderTarget,
        camera: &Camera2D,
        ambient: [f32; 4],
    ) {
        gpu.write_buffer(self.camera_buffer, 0, bytemuck::bytes_of(&camera.uniform()));

        let instances: Vec<LightInstance> = lights
            .iter()
            .map(|light| LightInstance {
                pos_radius: [light.position[0], light.position[1], light.radius, 0.0],
                color: light.effective_color(),
                falloff: [light.falloff.max(0.001), 50.0, 0.0, 0.0],
            })
            .collect();

        if !instances.is_empty() {
            gpu.write_buffer(self.instance_buffer, 0, bytemuck::cast_slice(&instances));
        }

        let (normal_image, normal_sampler) = normal_target
            .map(|target| (target.image(), target.sampler()))
            .unwrap_or((self.flat_normal.image(), self.flat_normal.sampler()));
        let normal_bind_group = self.normal_bind_group(gpu, normal_image, normal_sampler);

        gpu.with_render_pass(
            &RenderPassDesc {
                label: Cow::Borrowed("light_pass"),
                color_attachments: vec![ColorAttachment {
                    target: ColorTarget::Image(lightmap.image()),
                    clear: Some(ambient),
                }],
                depth_stencil: None,
            },
            |pass| {
                pass.set_pipeline(self.pipeline);
                pass.set_bind_group(0, self.camera_bind_group);
                pass.set_bind_group(1, normal_bind_group);
                pass.set_vertex_buffer(0, self.vertex_buffer);
                pass.set_vertex_buffer(1, self.instance_buffer);
                pass.set_index_buffer(self.index_buffer, crate::gpu::IndexFormat::Uint16);
                if !instances.is_empty() {
                    pass.draw_indexed(0..6, 0, 0..instances.len() as u32);
                }
            },
        );
    }

    fn normal_bind_group(
        &mut self,
        gpu: &mut impl Gpu,
        image: crate::gpu::Image,
        sampler: crate::gpu::Sampler,
    ) -> BindGroup {
        if let Some(bg) = self.normal_bind_groups.get(&(image, sampler)) {
            return *bg;
        }

        let bg = gpu.create_bind_group(&BindGroupDesc {
            label: Cow::Borrowed("light_normal_bg"),
            layout: self.normal_bgl,
            entries: vec![
                BindGroupEntry::Texture { binding: 0, image },
                BindGroupEntry::Sampler {
                    binding: 1,
                    sampler,
                },
            ],
        });
        self.normal_bind_groups.insert((image, sampler), bg);
        bg
    }

    pub fn invalidate_cache(&mut self, gpu: &mut impl Gpu) {
        for bind_group in self
            .normal_bind_groups
            .drain()
            .map(|(_, bind_group)| bind_group)
        {
            gpu.destroy_bind_group(bind_group);
        }
    }

    pub fn destroy(&mut self, gpu: &mut impl Gpu) {
        self.invalidate_cache(gpu);
        self.flat_normal.destroy(gpu);
        gpu.destroy_bind_group(self.camera_bind_group);
        gpu.destroy_bind_group_layout(self.camera_bgl);
        gpu.destroy_bind_group_layout(self.normal_bgl);
        gpu.destroy_buffer(self.camera_buffer);
        gpu.destroy_buffer(self.instance_buffer);
        gpu.destroy_buffer(self.index_buffer);
        gpu.destroy_buffer(self.vertex_buffer);
        gpu.destroy_pipeline(self.pipeline);
        gpu.destroy_shader(self._shader);
    }
}
