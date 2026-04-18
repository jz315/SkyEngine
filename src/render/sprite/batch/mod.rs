//! GPU-instanced 2D sprite batch renderer.
//!
//! Renders thousands of 2D sprites in a small number of draw calls, with
//! support for both surface and off-screen targets.
//!
//! # Usage
//!
//! ```rust,ignore
//! let mut batch = SpriteBatch::new(gpu);
//!
//! // Submit sprites (no begin() needed)
//! batch.set_texture(&tex);
//! batch.draw(Sprite::new(x, y, w, h).rotation(angle).color(Color::RED));
//! batch.clear_texture();
//! batch.draw(Sprite::new(x, y, w, h).color(Color::BLUE));
//!
//! // Flush to surface (auto-resets batch for next frame)
//! batch.flush_to_surface(gpu, &camera, Some(Color::rgb(0.02, 0.02, 0.06)));
//!
//! // Or flush to off-screen target (None = preserve existing contents)
//! batch.flush_to_target(gpu, &camera, &render_target, None);
//! ```

mod commands;
#[cfg(test)]
mod tests;

use std::sync::Arc;

use crate::gpu::GpuContext;
use crate::render::gpu::helpers::{
    create_textured_quad_geometry, BindGroupCache, CameraBinding, QuadGeometry, RenderPipelineCache,
};
use crate::render::gpu::RenderTarget;
use crate::render::gpu::Texture;
use crate::render::view::Color;
use crate::render::view::RenderView;

use self::commands::{close_pending_draw_cmd, DrawCmd};

/// Maximum sprites per batch before flushing.
const MAX_SPRITES: usize = 262_144;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct BatchPipelineKey {
    format: wgpu::TextureFormat,
    textured: bool,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct SpriteInstance {
    pub(crate) transform: [f32; 4], // x, y, w, h
    pub(crate) rotation: [f32; 4],  // sin, cos, z, 0
    pub(crate) color: [f32; 4],     // r, g, b, a
    pub(crate) uv_rect: [f32; 4],   // u_min, v_min, u_max, v_max
}

/// A sprite to be drawn this frame.
pub struct Sprite {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub width: f32,
    pub height: f32,
    pub rotation: f32,
    pub color: Color,
    pub uv_rect: [f32; 4],
}

impl Sprite {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            z: 0.0,
            width,
            height,
            rotation: 0.0,
            color: Color::WHITE,
            uv_rect: [0.0, 0.0, 1.0, 1.0],
        }
    }

    #[inline]
    pub fn z(mut self, z: f32) -> Self {
        self.z = z;
        self
    }

    #[inline]
    pub fn rotation(mut self, radians: f32) -> Self {
        self.rotation = radians;
        self
    }

    #[inline]
    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    #[inline]
    pub fn uv(mut self, u_min: f32, v_min: f32, u_max: f32, v_max: f32) -> Self {
        self.uv_rect = [u_min, v_min, u_max, v_max];
        self
    }
}

/// GPU-instanced sprite batch renderer.
pub struct SpriteBatch {
    shader: Arc<wgpu::ShaderModule>,
    quad: QuadGeometry,
    instance_buffer: wgpu::Buffer,
    camera: CameraBinding,
    texture_bgl: wgpu::BindGroupLayout,
    pipelines: RenderPipelineCache<BatchPipelineKey>,
    texture_bind_group_cache: BindGroupCache<usize>,
    texture_bind_group_scratch: Vec<wgpu::BindGroup>,
    instances: Vec<SpriteInstance>,
    draw_cmds: Vec<DrawCmd>,
    current_textured: bool,
    current_texture_idx: usize,
    frame_textures: Vec<Texture>,
    overflow_warned: bool,
}

impl SpriteBatch {
    pub fn new(ctx: &GpuContext) -> Self {
        let shader = Arc::new(
            ctx.device()
                .create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("sprite_shader"),
                    source: wgpu::ShaderSource::Wgsl(
                        include_str!("../../shaders/sprite.wgsl").into(),
                    ),
                }),
        );

        let quad = create_textured_quad_geometry(ctx, "sprite");
        let instance_buffer = ctx.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("sprite_instance_buf"),
            size: (MAX_SPRITES * std::mem::size_of::<SpriteInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let camera = CameraBinding::new(ctx, "sprite");

        let texture_bgl = ctx
            .device()
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("sprite_texture_bgl"),
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

        Self {
            shader,
            quad,
            instance_buffer,
            camera,
            texture_bgl,
            pipelines: RenderPipelineCache::new(),
            texture_bind_group_cache: BindGroupCache::new(),
            texture_bind_group_scratch: Vec::with_capacity(16),
            instances: Vec::with_capacity(1024),
            draw_cmds: Vec::with_capacity(16),
            current_textured: false,
            current_texture_idx: 0,
            frame_textures: Vec::with_capacity(16),
            overflow_warned: false,
        }
    }

    /// Texture bind group layout used by the batch.
    #[inline]
    pub fn texture_layout(&self) -> &wgpu::BindGroupLayout {
        &self.texture_bgl
    }

    /// Set the active texture for subsequent [`draw`] calls.
    pub fn set_texture(&mut self, texture: &Texture) {
        let idx = self
            .frame_textures
            .iter()
            .position(|t| t.ptr_eq(texture))
            .unwrap_or_else(|| {
                let i = self.frame_textures.len();
                self.frame_textures.push(texture.clone());
                i
            });

        if self.current_textured && self.current_texture_idx == idx {
            return;
        }

        self.close_pending_cmd();
        self.current_textured = true;
        self.current_texture_idx = idx;
    }

    /// Switch back to untextured (colour-only) mode.
    pub fn clear_texture(&mut self) {
        if !self.current_textured {
            return;
        }
        self.close_pending_cmd();
        self.current_textured = false;
    }

    /// Add a sprite to the batch.
    pub fn draw(&mut self, sprite: Sprite) {
        if self.instances.len() >= MAX_SPRITES {
            if !self.overflow_warned {
                eprintln!(
                    "[SkyEngine] SpriteBatch: MAX_SPRITES ({}) reached, sprites dropped",
                    MAX_SPRITES
                );
                self.overflow_warned = true;
            }
            return;
        }
        let (sin_a, cos_a) = sprite.rotation.sin_cos();
        self.instances.push(SpriteInstance {
            transform: [sprite.x, sprite.y, sprite.width, sprite.height],
            rotation: [sin_a, cos_a, sprite.z, 0.0],
            color: sprite.color.to_array(),
            uv_rect: sprite.uv_rect,
        });
    }

    /// Flush the batch to the presentation surface.
    pub fn flush_to_surface(
        &mut self,
        ctx: &mut GpuContext,
        view: &impl RenderView,
        clear: Option<Color>,
    ) {
        let format = ctx.surface_format();
        self.upload(ctx, view);
        let pip_color = self.pipeline_for(ctx, format, false);
        let pip_tex = self.pipeline_for(ctx, format, true);
        self.close_pending_cmd();
        self.prepare_texture_bind_groups(ctx);

        {
            let mut frame = ctx.frame();
            let mut pass =
                frame.begin_surface_pass("sprite_batch_pass", clear.map(|c| c.to_wgpu()));
            Self::execute(
                &mut *pass,
                &self.draw_cmds,
                &self.texture_bind_group_scratch,
                &self.camera.bind_group,
                &self.quad.vertex_buffer,
                &self.quad.index_buffer,
                &self.instance_buffer,
                Some(pip_color.as_ref()),
                Some(pip_tex.as_ref()),
            );
        }

        self.reset();
    }

    /// Flush the batch to an off-screen render target.
    pub fn flush_to_target(
        &mut self,
        ctx: &mut GpuContext,
        view: &impl RenderView,
        target: &RenderTarget,
        clear: Option<Color>,
    ) {
        let format = target.format();
        self.upload(ctx, view);

        let load = match clear {
            Some(c) => wgpu::LoadOp::Clear(c.to_wgpu()),
            None => wgpu::LoadOp::Load,
        };

        let pip_color = self.pipeline_for(ctx, format, false);
        let pip_tex = self.pipeline_for(ctx, format, true);
        self.close_pending_cmd();
        self.prepare_texture_bind_groups(ctx);

        {
            let mut frame = ctx.frame();
            let mut pass = frame.begin_target_pass("sprite_batch_pass", target, load);
            Self::execute(
                &mut *pass,
                &self.draw_cmds,
                &self.texture_bind_group_scratch,
                &self.camera.bind_group,
                &self.quad.vertex_buffer,
                &self.quad.index_buffer,
                &self.instance_buffer,
                Some(pip_color.as_ref()),
                Some(pip_tex.as_ref()),
            );
        }

        self.reset();
    }

    fn reset(&mut self) {
        self.instances.clear();
        self.draw_cmds.clear();
        self.frame_textures.clear();
        self.texture_bind_group_scratch.clear();
        self.current_textured = false;
        self.current_texture_idx = 0;
        self.overflow_warned = false;
    }

    fn upload(&self, ctx: &GpuContext, view: &impl RenderView) {
        self.camera.upload_view(ctx, view);
        if !self.instances.is_empty() {
            ctx.queue().write_buffer(
                &self.instance_buffer,
                0,
                bytemuck::cast_slice(&self.instances),
            );
        }
    }

    fn close_pending_cmd(&mut self) {
        close_pending_draw_cmd(
            &mut self.draw_cmds,
            self.instances.len() as u32,
            self.current_textured,
            self.current_texture_idx,
        );
    }

    fn prepare_texture_bind_groups(&mut self, ctx: &GpuContext) {
        let texture_bgl = self.texture_bgl.clone();
        let sampler = ctx.sampler_nearest();
        self.texture_bind_group_scratch.clear();
        for texture in &self.frame_textures {
            let key = std::ptr::from_ref(texture.texture()) as usize;
            let bind_group = self
                .texture_bind_group_cache
                .get_or_create(key, || {
                    ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("sprite_texture_bg"),
                        layout: &texture_bgl,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::TextureView(texture.view()),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::Sampler(sampler),
                            },
                        ],
                    })
                })
                .clone();
            self.texture_bind_group_scratch.push(bind_group);
        }
    }

    fn execute(
        pass: &mut wgpu::RenderPass<'_>,
        cmds: &[DrawCmd],
        texture_bind_groups: &[wgpu::BindGroup],
        camera_bg: &wgpu::BindGroup,
        vertex_buf: &wgpu::Buffer,
        index_buf: &wgpu::Buffer,
        instance_buf: &wgpu::Buffer,
        pip_color: Option<&wgpu::RenderPipeline>,
        pip_textured: Option<&wgpu::RenderPipeline>,
    ) {
        pass.set_bind_group(0, camera_bg, &[]);
        pass.set_vertex_buffer(0, vertex_buf.slice(..));
        pass.set_vertex_buffer(1, instance_buf.slice(..));
        pass.set_index_buffer(index_buf.slice(..), wgpu::IndexFormat::Uint16);

        for cmd in cmds {
            if cmd.textured {
                let Some(pip) = pip_textured else { continue };
                let Some(bg) = texture_bind_groups.get(cmd.texture_idx) else {
                    continue;
                };
                pass.set_pipeline(pip);
                pass.set_bind_group(1, bg, &[]);
            } else {
                let Some(pip) = pip_color else { continue };
                pass.set_pipeline(pip);
            }
            pass.draw_indexed(
                0..6,
                0,
                cmd.first_instance..(cmd.first_instance + cmd.instance_count),
            );
        }
    }

    fn pipeline_for(
        &mut self,
        ctx: &GpuContext,
        format: wgpu::TextureFormat,
        textured: bool,
    ) -> Arc<wgpu::RenderPipeline> {
        let key = BatchPipelineKey { format, textured };
        let shader = self.shader.clone();
        let camera_bgl = self.camera.layout.clone();
        let texture_bgl = self.texture_bgl.clone();
        self.pipelines.get_or_create(key, || {
            let layouts: Vec<&wgpu::BindGroupLayout> = if textured {
                vec![&camera_bgl, &texture_bgl]
            } else {
                vec![&camera_bgl]
            };

            let pipeline_layout =
                ctx.device()
                    .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                        label: Some("sprite_pipeline_layout"),
                        bind_group_layouts: &layouts,
                        push_constant_ranges: &[],
                    });

            let alpha_blend = wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::SrcAlpha,
                    dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                    operation: wgpu::BlendOperation::Add,
                },
                alpha: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                    operation: wgpu::BlendOperation::Add,
                },
            };

            ctx.device()
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some(&format!(
                        "sprite_{}_pipeline_{format:?}",
                        if textured { "textured" } else { "color" }
                    )),
                    layout: Some(&pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: shader.as_ref(),
                        entry_point: Some("vs_main"),
                        buffers: &[
                            wgpu::VertexBufferLayout {
                                array_stride: 16,
                                step_mode: wgpu::VertexStepMode::Vertex,
                                attributes: &[
                                    wgpu::VertexAttribute {
                                        offset: 0,
                                        shader_location: 0,
                                        format: wgpu::VertexFormat::Float32x2,
                                    },
                                    wgpu::VertexAttribute {
                                        offset: 8,
                                        shader_location: 1,
                                        format: wgpu::VertexFormat::Float32x2,
                                    },
                                ],
                            },
                            wgpu::VertexBufferLayout {
                                array_stride: std::mem::size_of::<SpriteInstance>() as u64,
                                step_mode: wgpu::VertexStepMode::Instance,
                                attributes: &[
                                    wgpu::VertexAttribute {
                                        offset: 0,
                                        shader_location: 2,
                                        format: wgpu::VertexFormat::Float32x4,
                                    },
                                    wgpu::VertexAttribute {
                                        offset: 16,
                                        shader_location: 3,
                                        format: wgpu::VertexFormat::Float32x4,
                                    },
                                    wgpu::VertexAttribute {
                                        offset: 32,
                                        shader_location: 4,
                                        format: wgpu::VertexFormat::Float32x4,
                                    },
                                    wgpu::VertexAttribute {
                                        offset: 48,
                                        shader_location: 5,
                                        format: wgpu::VertexFormat::Float32x4,
                                    },
                                ],
                            },
                        ],
                        compilation_options: Default::default(),
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: shader.as_ref(),
                        entry_point: Some(if textured { "fs_main" } else { "fs_color_only" }),
                        targets: &[Some(wgpu::ColorTargetState {
                            format,
                            blend: Some(alpha_blend),
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
        })
    }
}
