//! GPU-instanced 2D sprite batch renderer.
//!
//! Renders thousands of 2D sprites in a small number of draw calls, with
//! explicit support for both surface and off-screen targets.

use std::borrow::Cow;

use crate::gpu::{
    BindGroup, BindGroupDesc, BindGroupEntry, BindGroupLayout, BindGroupLayoutDesc, BindingType,
    Buffer, BufferDesc, BufferUsage, ColorAttachment, ColorTarget, ColorTargetState, Gpu, Image,
    IndexFormat, Pipeline, PrimitiveState, RenderPassDesc, RenderPipelineDesc, Sampler, ShaderDesc,
    ShaderStages, TextureFormat, VertexAttribute, VertexBufferLayout, VertexFormat, VertexStepMode,
};
use crate::render::camera::{Camera2D, CameraUniform};
use crate::render::color::Color;
use crate::render::target::RenderTarget;
use crate::render::texture::Texture;

use rustc_hash::FxHashMap;

/// Per-instance data uploaded to the GPU.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct SpriteInstance {
    transform: [f32; 4], // x, y, w, h
    rotation: [f32; 4],  // sin, cos, 0, 0
    color: [f32; 4],     // r, g, b, a
    uv_rect: [f32; 4],   // u_min, v_min, u_max, v_max
}

/// Quad vertex.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct QuadVertex {
    position: [f32; 2],
    uv: [f32; 2],
}

const QUAD_VERTICES: [QuadVertex; 4] = [
    QuadVertex {
        position: [0.0, 0.0],
        uv: [0.0, 1.0],
    },
    QuadVertex {
        position: [1.0, 0.0],
        uv: [1.0, 1.0],
    },
    QuadVertex {
        position: [1.0, 1.0],
        uv: [1.0, 0.0],
    },
    QuadVertex {
        position: [0.0, 1.0],
        uv: [0.0, 0.0],
    },
];

const QUAD_INDICES: [u16; 6] = [0, 1, 2, 0, 2, 3];

/// Maximum sprites per batch before flushing.
const MAX_SPRITES: usize = 262_144;

/// A sprite to be drawn this frame.
pub struct Sprite {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub rotation: f32,
    pub color: Color,
    pub uv_rect: [f32; 4],
}

impl Sprite {
    /// Create a new sprite at the given position with the given size.
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
            rotation: 0.0,
            color: Color::WHITE,
            uv_rect: [0.0, 0.0, 1.0, 1.0],
        }
    }

    /// Set the rotation angle in radians.
    #[inline]
    pub fn rotation(mut self, radians: f32) -> Self {
        self.rotation = radians;
        self
    }

    /// Set the colour tint.
    #[inline]
    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// Set the UV sub-rectangle (for sprite sheets / atlases).
    #[inline]
    pub fn uv(mut self, u_min: f32, v_min: f32, u_max: f32, v_max: f32) -> Self {
        self.uv_rect = [u_min, v_min, u_max, v_max];
        self
    }
}

struct DrawCmd {
    first_instance: u32,
    instance_count: u32,
    textured: bool,
    texture_idx: usize,
}

/// GPU-instanced sprite batch renderer.
pub struct SpriteBatch {
    shader: crate::gpu::Shader,
    vertex_buffer: Buffer,
    index_buffer: Buffer,
    instance_buffer: Buffer,
    camera_buffer: Buffer,
    camera_bind_group: BindGroup,
    camera_bgl: BindGroupLayout,
    texture_bgl: BindGroupLayout,
    pipelines_color: FxHashMap<TextureFormat, Pipeline>,
    pipelines_textured: FxHashMap<TextureFormat, Pipeline>,
    texture_bind_groups: FxHashMap<(Image, Sampler), BindGroup>,
    instances: Vec<SpriteInstance>,
    draw_cmds: Vec<DrawCmd>,
    current_textured: bool,
    current_texture_idx: usize,
    frame_textures: Vec<(Image, Sampler)>,
}

impl SpriteBatch {
    /// Create a new sprite batch renderer.
    pub fn new(gpu: &mut impl Gpu) -> Self {
        let shader = gpu.create_shader(&ShaderDesc {
            label: Cow::Borrowed("sprite_shader"),
            source: Cow::Borrowed(include_str!("shaders/sprite.wgsl")),
        });

        let vertex_buffer = gpu.create_buffer(&BufferDesc {
            label: Cow::Borrowed("sprite_quad_vb"),
            size: std::mem::size_of_val(&QUAD_VERTICES) as u64,
            usage: BufferUsage::VERTEX,
        });
        gpu.write_buffer(vertex_buffer, 0, bytemuck::cast_slice(&QUAD_VERTICES));

        let index_buffer = gpu.create_buffer(&BufferDesc {
            label: Cow::Borrowed("sprite_quad_ib"),
            size: std::mem::size_of_val(&QUAD_INDICES) as u64,
            usage: BufferUsage::INDEX,
        });
        gpu.write_buffer(index_buffer, 0, bytemuck::cast_slice(&QUAD_INDICES));

        let instance_buffer = gpu.create_buffer(&BufferDesc {
            label: Cow::Borrowed("sprite_instance_buf"),
            size: (MAX_SPRITES * std::mem::size_of::<SpriteInstance>()) as u64,
            usage: BufferUsage::VERTEX | BufferUsage::COPY_DST,
        });

        let camera_buffer = gpu.create_buffer(&BufferDesc {
            label: Cow::Borrowed("sprite_camera_buf"),
            size: std::mem::size_of::<CameraUniform>() as u64,
            usage: BufferUsage::UNIFORM | BufferUsage::COPY_DST,
        });

        let camera_bgl = gpu.create_bind_group_layout(&BindGroupLayoutDesc {
            label: Cow::Borrowed("sprite_camera_bgl"),
            entries: vec![crate::gpu::BindGroupLayoutEntry {
                binding: 0,
                ty: BindingType::UniformBuffer,
                visibility: ShaderStages::VERTEX_FRAGMENT,
            }],
        });

        let texture_bgl = gpu.create_bind_group_layout(&BindGroupLayoutDesc {
            label: Cow::Borrowed("sprite_texture_bgl"),
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
            label: Cow::Borrowed("sprite_camera_bg"),
            layout: camera_bgl,
            entries: vec![BindGroupEntry::Buffer {
                binding: 0,
                buffer: camera_buffer,
                offset: 0,
                size: std::mem::size_of::<CameraUniform>() as u64,
            }],
        });

        Self {
            shader,
            vertex_buffer,
            index_buffer,
            instance_buffer,
            camera_buffer,
            camera_bind_group,
            camera_bgl,
            texture_bgl,
            pipelines_color: FxHashMap::default(),
            pipelines_textured: FxHashMap::default(),
            texture_bind_groups: FxHashMap::default(),
            instances: Vec::with_capacity(1024),
            draw_cmds: Vec::with_capacity(16),
            current_textured: false,
            current_texture_idx: 0,
            frame_textures: Vec::with_capacity(16),
        }
    }

    /// Texture bind group layout used by the batch.
    #[inline]
    pub fn texture_layout(&self) -> BindGroupLayout {
        self.texture_bgl
    }

    /// Clear the batch for a new frame.
    pub fn begin(&mut self) {
        self.instances.clear();
        self.draw_cmds.clear();
        self.frame_textures.clear();
        self.current_textured = false;
        self.current_texture_idx = 0;
    }

    /// Set the active texture for subsequent [`draw`] calls.
    pub fn set_texture(&mut self, texture: &Texture) {
        let key = (texture.image(), texture.sampler());
        let idx = self
            .frame_textures
            .iter()
            .position(|existing| *existing == key)
            .unwrap_or_else(|| {
                let i = self.frame_textures.len();
                self.frame_textures.push(key);
                i
            });

        if self.current_textured && self.current_texture_idx == idx {
            return;
        }

        self.maybe_close_cmd();
        self.current_textured = true;
        self.current_texture_idx = idx;
    }

    /// Switch back to untextured (colour-only) mode.
    pub fn unset_texture(&mut self) {
        if !self.current_textured {
            return;
        }
        self.maybe_close_cmd();
        self.current_textured = false;
    }

    /// Add a sprite to the batch.
    pub fn draw(&mut self, sprite: Sprite) {
        if self.instances.len() >= MAX_SPRITES {
            return;
        }

        let (sin_a, cos_a) = sprite.rotation.sin_cos();
        self.instances.push(SpriteInstance {
            transform: [sprite.x, sprite.y, sprite.width, sprite.height],
            rotation: [sin_a, cos_a, 0.0, 0.0],
            color: sprite.color.to_array(),
            uv_rect: sprite.uv_rect,
        });
    }

    /// Draw to the presentation surface.
    pub fn draw_to_surface(
        &mut self,
        gpu: &mut impl Gpu,
        camera: &Camera2D,
        clear: Option<[f32; 4]>,
    ) {
        let surface_format = gpu.surface_format();
        self.draw_inner(gpu, camera, ColorTarget::Surface, surface_format, clear);
    }

    /// Draw to an off-screen render target.
    pub fn draw_to_target(
        &mut self,
        gpu: &mut impl Gpu,
        camera: &Camera2D,
        target: &RenderTarget,
        clear: Option<[f32; 4]>,
    ) {
        self.draw_inner(
            gpu,
            camera,
            ColorTarget::Image(target.image()),
            target.format(),
            clear,
        );
    }

    fn draw_inner(
        &mut self,
        gpu: &mut impl Gpu,
        camera: &Camera2D,
        color_target: ColorTarget,
        target_format: TextureFormat,
        clear: Option<[f32; 4]>,
    ) {
        self.maybe_close_cmd();

        if self.instances.is_empty() && clear.is_none() {
            return;
        }

        gpu.write_buffer(self.camera_buffer, 0, bytemuck::bytes_of(&camera.uniform()));

        if !self.instances.is_empty() {
            gpu.write_buffer(
                self.instance_buffer,
                0,
                bytemuck::cast_slice(&self.instances),
            );
        }

        let draw_cmds: Vec<(bool, usize, u32, u32)> = self
            .draw_cmds
            .iter()
            .map(|cmd| {
                (
                    cmd.textured,
                    cmd.texture_idx,
                    cmd.first_instance,
                    cmd.instance_count,
                )
            })
            .collect();
        let mut draw_plan = Vec::with_capacity(draw_cmds.len());
        for (textured, texture_idx, first_instance, instance_count) in draw_cmds {
            let bind_group = if textured {
                let (image, sampler) = self.frame_textures[texture_idx];
                Some(self.texture_bind_group(gpu, image, sampler))
            } else {
                None
            };
            draw_plan.push((textured, bind_group, first_instance, instance_count));
        }

        let pipeline_color = self.pipeline_for_format(gpu, target_format, false);
        let pipeline_textured = self.pipeline_for_format(gpu, target_format, true);

        gpu.with_render_pass(
            &RenderPassDesc {
                label: Cow::Borrowed("sprite_batch_pass"),
                color_attachments: vec![ColorAttachment {
                    target: color_target,
                    clear,
                }],
                depth_stencil: None,
            },
            |pass| {
                pass.set_bind_group(0, self.camera_bind_group);
                pass.set_vertex_buffer(0, self.vertex_buffer);
                pass.set_vertex_buffer(1, self.instance_buffer);
                pass.set_index_buffer(self.index_buffer, IndexFormat::Uint16);

                for (textured, bind_group, first_instance, instance_count) in &draw_plan {
                    if *textured {
                        pass.set_pipeline(pipeline_textured);
                        pass.set_bind_group(1, bind_group.expect("missing texture bind group"));
                    } else {
                        pass.set_pipeline(pipeline_color);
                    }
                    pass.draw_indexed(
                        0..6,
                        0,
                        *first_instance..(*first_instance + *instance_count),
                    );
                }
            },
        );
    }

    fn maybe_close_cmd(&mut self) {
        let total = self.instances.len() as u32;
        let already_claimed: u32 = self.draw_cmds.iter().map(|c| c.instance_count).sum();
        let pending = total - already_claimed;

        if pending > 0 {
            self.draw_cmds.push(DrawCmd {
                first_instance: already_claimed,
                instance_count: pending,
                textured: self.current_textured,
                texture_idx: self.current_texture_idx,
            });
        }
    }

    fn texture_bind_group(
        &mut self,
        gpu: &mut impl Gpu,
        image: Image,
        sampler: Sampler,
    ) -> BindGroup {
        if let Some(bg) = self.texture_bind_groups.get(&(image, sampler)) {
            return *bg;
        }

        let bind_group = gpu.create_bind_group(&BindGroupDesc {
            label: Cow::Borrowed("sprite_texture_bg"),
            layout: self.texture_bgl,
            entries: vec![
                BindGroupEntry::Texture { binding: 0, image },
                BindGroupEntry::Sampler {
                    binding: 1,
                    sampler,
                },
            ],
        });
        self.texture_bind_groups
            .insert((image, sampler), bind_group);
        bind_group
    }

    fn pipeline_for_format(
        &mut self,
        gpu: &mut impl Gpu,
        format: TextureFormat,
        textured: bool,
    ) -> Pipeline {
        let cache = if textured {
            &mut self.pipelines_textured
        } else {
            &mut self.pipelines_color
        };

        if let Some(pipeline) = cache.get(&format) {
            return *pipeline;
        }

        let vertex_layout = VertexBufferLayout {
            stride: std::mem::size_of::<QuadVertex>() as u64,
            step_mode: VertexStepMode::Vertex,
            attributes: vec![
                VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: VertexFormat::Float32x2,
                },
                VertexAttribute {
                    offset: 8,
                    shader_location: 1,
                    format: VertexFormat::Float32x2,
                },
            ],
        };
        let instance_layout = VertexBufferLayout {
            stride: std::mem::size_of::<SpriteInstance>() as u64,
            step_mode: VertexStepMode::Instance,
            attributes: vec![
                VertexAttribute {
                    offset: 0,
                    shader_location: 2,
                    format: VertexFormat::Float32x4,
                },
                VertexAttribute {
                    offset: 16,
                    shader_location: 3,
                    format: VertexFormat::Float32x4,
                },
                VertexAttribute {
                    offset: 32,
                    shader_location: 4,
                    format: VertexFormat::Float32x4,
                },
                VertexAttribute {
                    offset: 48,
                    shader_location: 5,
                    format: VertexFormat::Float32x4,
                },
            ],
        };

        let pipeline = gpu.create_render_pipeline(&RenderPipelineDesc {
            label: Cow::Owned(format!(
                "sprite_{}_pipeline_{:?}",
                if textured { "textured" } else { "color" },
                format
            )),
            shader: self.shader,
            vs_entry: "vs_main",
            fs_entry: if textured { "fs_main" } else { "fs_color_only" },
            vertex_layouts: vec![vertex_layout, instance_layout],
            bind_group_layouts: if textured {
                vec![self.camera_bgl, self.texture_bgl]
            } else {
                vec![self.camera_bgl]
            },
            color_targets: vec![ColorTargetState {
                format,
                blend: Some(crate::gpu::BlendState::ALPHA_BLEND),
            }],
            depth_stencil: None,
            primitive: PrimitiveState::default(),
        });

        cache.insert(format, pipeline);
        pipeline
    }

    pub fn invalidate_cache(&mut self, gpu: &mut impl Gpu) {
        for bind_group in self
            .texture_bind_groups
            .drain()
            .map(|(_, bind_group)| bind_group)
        {
            gpu.destroy_bind_group(bind_group);
        }
    }

    pub fn destroy(&mut self, gpu: &mut impl Gpu) {
        self.invalidate_cache(gpu);
        for pipeline in self.pipelines_color.drain().map(|(_, pipeline)| pipeline) {
            gpu.destroy_pipeline(pipeline);
        }
        for pipeline in self
            .pipelines_textured
            .drain()
            .map(|(_, pipeline)| pipeline)
        {
            gpu.destroy_pipeline(pipeline);
        }
        gpu.destroy_bind_group(self.camera_bind_group);
        gpu.destroy_bind_group_layout(self.texture_bgl);
        gpu.destroy_bind_group_layout(self.camera_bgl);
        gpu.destroy_buffer(self.camera_buffer);
        gpu.destroy_buffer(self.instance_buffer);
        gpu.destroy_buffer(self.index_buffer);
        gpu.destroy_buffer(self.vertex_buffer);
        gpu.destroy_shader(self.shader);
    }
}
