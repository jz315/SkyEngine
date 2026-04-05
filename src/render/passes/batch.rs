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

use std::sync::Arc;

use crate::gpu::GpuContext;
use crate::render::core::camera::{CameraUniform, RenderView};
use crate::render::core::color::Color;
use crate::render::core::target::RenderTarget;
use crate::render::core::texture::Texture;

use rustc_hash::FxHashMap;

// ── GPU data types ──────────────────────────────────────────────────────────

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct SpriteInstance {
    transform: [f32; 4], // x, y, w, h
    rotation: [f32; 4],  // sin, cos, 0, 0
    color: [f32; 4],     // r, g, b, a
    uv_rect: [f32; 4],   // u_min, v_min, u_max, v_max
}

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

// ── Sprite ──────────────────────────────────────────────────────────────────

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

// ── Internal draw command ───────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DrawCmd {
    first_instance: u32,
    instance_count: u32,
    textured: bool,
    texture_idx: usize,
}

// ── SpriteBatch ─────────────────────────────────────────────────────────────

/// GPU-instanced sprite batch renderer.
pub struct SpriteBatch {
    // GPU resources
    _shader: wgpu::ShaderModule,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    instance_buffer: wgpu::Buffer,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    camera_bgl: wgpu::BindGroupLayout,
    texture_bgl: wgpu::BindGroupLayout,
    pipelines_color: FxHashMap<wgpu::TextureFormat, Arc<wgpu::RenderPipeline>>,
    pipelines_textured: FxHashMap<wgpu::TextureFormat, Arc<wgpu::RenderPipeline>>,

    // Per-frame state
    instances: Vec<SpriteInstance>,
    draw_cmds: Vec<DrawCmd>,
    current_textured: bool,
    current_texture_idx: usize,
    frame_textures: Vec<Texture>,
    overflow_warned: bool,
}

impl SpriteBatch {
    pub fn new(ctx: &GpuContext) -> Self {
        let shader = ctx
            .device()
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("sprite_shader"),
                source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/sprite.wgsl").into()),
            });

        let vertex_buffer = ctx.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("sprite_quad_vb"),
            size: std::mem::size_of_val(&QUAD_VERTICES) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        ctx.queue()
            .write_buffer(&vertex_buffer, 0, bytemuck::cast_slice(&QUAD_VERTICES));

        let index_buffer = ctx.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("sprite_quad_ib"),
            size: std::mem::size_of_val(&QUAD_INDICES) as u64,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        ctx.queue()
            .write_buffer(&index_buffer, 0, bytemuck::cast_slice(&QUAD_INDICES));

        let instance_buffer = ctx.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("sprite_instance_buf"),
            size: (MAX_SPRITES * std::mem::size_of::<SpriteInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let camera_buffer = ctx.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("sprite_camera_buf"),
            size: std::mem::size_of::<CameraUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let camera_bgl = ctx
            .device()
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("sprite_camera_bgl"),
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

        let camera_bind_group = ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sprite_camera_bg"),
            layout: &camera_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });

        Self {
            _shader: shader,
            vertex_buffer,
            index_buffer,
            instance_buffer,
            camera_buffer,
            camera_bind_group,
            camera_bgl,
            texture_bgl,
            pipelines_color: FxHashMap::default(),
            pipelines_textured: FxHashMap::default(),
            instances: Vec::with_capacity(1024),
            draw_cmds: Vec::with_capacity(16),
            current_textured: false,
            current_texture_idx: 0,
            frame_textures: Vec::with_capacity(16),
            overflow_warned: false,
        }
    }

    // ── Public API: sprite submission ────────────────────────────────────

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
            rotation: [sin_a, cos_a, 0.0, 0.0],
            color: sprite.color.to_array(),
            uv_rect: sprite.uv_rect,
        });
    }

    // ── Public API: flush / render ───────────────────────────────────────

    /// Flush the batch to the presentation surface.
    ///
    /// `clear`: `Some(color)` clears first; `None` preserves existing contents.
    ///
    /// The batch is automatically reset after flushing.
    pub fn flush_to_surface(
        &mut self,
        ctx: &mut GpuContext,
        view: &impl RenderView,
        clear: Option<Color>,
    ) {
        let format = ctx.surface_format();
        self.ensure_pipelines(ctx, format);
        self.upload(ctx, view);

        let cmds = self.finalize_draw_cmds();
        let bind_groups = self.create_texture_bind_groups(ctx);
        let pip_color = self.pipelines_color.get(&format).cloned();
        let pip_tex = self.pipelines_textured.get(&format).cloned();

        {
            let mut frame = ctx.frame();
            let mut pass =
                frame.begin_surface_pass("sprite_batch_pass", clear.map(|c| c.to_wgpu()));
            Self::execute(
                &mut *pass,
                &cmds,
                &bind_groups,
                &self.camera_bind_group,
                &self.vertex_buffer,
                &self.index_buffer,
                &self.instance_buffer,
                pip_color.as_deref(),
                pip_tex.as_deref(),
            );
        }

        self.reset();
    }

    /// Flush the batch to an off-screen render target.
    ///
    /// `clear`: `Some(color)` clears first; `None` preserves existing contents.
    ///
    /// The batch is automatically reset after flushing.
    pub fn flush_to_target(
        &mut self,
        ctx: &mut GpuContext,
        view: &impl RenderView,
        target: &RenderTarget,
        clear: Option<Color>,
    ) {
        let format = target.format();
        self.ensure_pipelines(ctx, format);
        self.upload(ctx, view);

        let load = match clear {
            Some(c) => wgpu::LoadOp::Clear(c.to_wgpu()),
            None => wgpu::LoadOp::Load,
        };

        let cmds = self.finalize_draw_cmds();
        let bind_groups = self.create_texture_bind_groups(ctx);
        let pip_color = self.pipelines_color.get(&format).cloned();
        let pip_tex = self.pipelines_textured.get(&format).cloned();

        {
            let mut frame = ctx.frame();
            let mut pass = frame.begin_target_pass("sprite_batch_pass", target, load);
            Self::execute(
                &mut *pass,
                &cmds,
                &bind_groups,
                &self.camera_bind_group,
                &self.vertex_buffer,
                &self.index_buffer,
                &self.instance_buffer,
                pip_color.as_deref(),
                pip_tex.as_deref(),
            );
        }

        self.reset();
    }

    // ── Internal ────────────────────────────────────────────────────────

    fn reset(&mut self) {
        self.instances.clear();
        self.draw_cmds.clear();
        self.frame_textures.clear();
        self.current_textured = false;
        self.current_texture_idx = 0;
        self.overflow_warned = false;
    }

    fn upload(&self, ctx: &GpuContext, view: &impl RenderView) {
        ctx.queue().write_buffer(
            &self.camera_buffer,
            0,
            bytemuck::bytes_of(&view.view_uniform()),
        );
        if !self.instances.is_empty() {
            ctx.queue().write_buffer(
                &self.instance_buffer,
                0,
                bytemuck::cast_slice(&self.instances),
            );
        }
    }

    fn close_pending_cmd(&mut self) {
        let already_claimed: u32 = self.draw_cmds.iter().map(|c| c.instance_count).sum();
        let pending = (self.instances.len() as u32).saturating_sub(already_claimed);
        if pending > 0 {
            self.draw_cmds.push(DrawCmd {
                first_instance: already_claimed,
                instance_count: pending,
                textured: self.current_textured,
                texture_idx: self.current_texture_idx,
            });
        }
    }

    fn finalize_draw_cmds(&mut self) -> Vec<DrawCmd> {
        self.close_pending_cmd();
        self.draw_cmds.clone()
    }

    fn create_texture_bind_groups(&self, ctx: &GpuContext) -> Vec<wgpu::BindGroup> {
        self.frame_textures
            .iter()
            .map(|tex| {
                ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("sprite_texture_bg"),
                    layout: &self.texture_bgl,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(tex.view()),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(ctx.sampler_nearest()),
                        },
                    ],
                })
            })
            .collect()
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
                pass.set_pipeline(pip);
                pass.set_bind_group(1, &texture_bind_groups[cmd.texture_idx], &[]);
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

    fn ensure_pipelines(&mut self, ctx: &GpuContext, format: wgpu::TextureFormat) {
        if !self.pipelines_color.contains_key(&format) {
            self.pipelines_color
                .insert(format, Arc::new(self.create_pipeline(ctx, format, false)));
        }
        if !self.pipelines_textured.contains_key(&format) {
            self.pipelines_textured
                .insert(format, Arc::new(self.create_pipeline(ctx, format, true)));
        }
    }

    fn create_pipeline(
        &self,
        ctx: &GpuContext,
        format: wgpu::TextureFormat,
        textured: bool,
    ) -> wgpu::RenderPipeline {
        let shader = &self._shader;

        let layouts: Vec<&wgpu::BindGroupLayout> = if textured {
            vec![&self.camera_bgl, &self.texture_bgl]
        } else {
            vec![&self.camera_bgl]
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
                    "sprite_{}_pipeline_{:?}",
                    if textured { "textured" } else { "color" },
                    format
                )),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: shader,
                    entry_point: Some("vs_main"),
                    buffers: &[
                        wgpu::VertexBufferLayout {
                            array_stride: std::mem::size_of::<QuadVertex>() as u64,
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
                    module: shader,
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
    }
}

#[cfg(test)]
mod tests {
    use super::DrawCmd;

    /// Mirror of `SpriteBatch::close_pending_cmd` logic for testing.
    fn close_pending(
        draw_cmds: &mut Vec<DrawCmd>,
        total_instances: u32,
        textured: bool,
        texture_idx: usize,
    ) {
        let already_claimed: u32 = draw_cmds.iter().map(|c| c.instance_count).sum();
        let pending = total_instances.saturating_sub(already_claimed);
        if pending > 0 {
            draw_cmds.push(DrawCmd {
                first_instance: already_claimed,
                instance_count: pending,
                textured,
                texture_idx,
            });
        }
    }

    #[test]
    fn close_pending_appends_unclaimed_instances() {
        let mut cmds = vec![DrawCmd {
            first_instance: 0,
            instance_count: 3,
            textured: false,
            texture_idx: 0,
        }];
        close_pending(&mut cmds, 5, true, 2);

        assert_eq!(cmds.len(), 2);
        assert_eq!(
            cmds[1],
            DrawCmd {
                first_instance: 3,
                instance_count: 2,
                textured: true,
                texture_idx: 2,
            }
        );
    }

    #[test]
    fn close_pending_does_not_duplicate_completed_work() {
        let mut cmds = vec![DrawCmd {
            first_instance: 0,
            instance_count: 4,
            textured: false,
            texture_idx: 0,
        }];
        close_pending(&mut cmds, 4, true, 1);

        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0].instance_count, 4);
    }
}
