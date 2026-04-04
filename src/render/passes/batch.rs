//! GPU-instanced 2D sprite batch renderer.
//!
//! Renders thousands of 2D sprites in a small number of draw calls, with
//! explicit support for both surface and off-screen targets.

use std::sync::Arc;

use crate::gpu::GpuContext;
use crate::render::core::camera::{Camera2D, CameraUniform};
use crate::render::core::color::Color;
use crate::render::core::target::RenderTarget;
use crate::render::core::texture::Texture;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DrawCmd {
    first_instance: u32,
    instance_count: u32,
    textured: bool,
    texture_idx: usize,
}

/// GPU-instanced sprite batch renderer.
pub struct SpriteBatch {
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

    /// Texture bind group layout used by the batch.
    #[inline]
    pub fn texture_layout(&self) -> &wgpu::BindGroupLayout {
        &self.texture_bgl
    }

    /// Clear the batch for a new frame.
    pub fn begin(&mut self) {
        self.instances.clear();
        self.draw_cmds.clear();
        self.frame_textures.clear();
        self.current_textured = false;
        self.current_texture_idx = 0;
        self.overflow_warned = false;
    }

    /// Set the active texture for subsequent [`draw`] calls.
    pub fn set_texture(&mut self, texture: &Texture) {
        // Find or register texture by Arc pointer identity
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
            if !self.overflow_warned {
                eprintln!(
                    "[SkyEngine] SpriteBatch: MAX_SPRITES ({}) reached, subsequent sprites dropped",
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

    /// Draw to the presentation surface.
    ///
    /// `clear`: `Some([r,g,b,a])` clears the surface first; `None` preserves
    /// the existing surface contents (`LoadOp::Load`).
    pub fn draw_to_surface(
        &mut self,
        ctx: &mut GpuContext,
        camera: &Camera2D,
        clear: Option<[f32; 4]>,
    ) {
        let surface_format = ctx.surface_format();
        self.ensure_pipelines(ctx, surface_format);
        self.upload(ctx, camera);

        let draw_plan = self.build_draw_plan(ctx);
        let pip_color = self.pipelines_color.get(&surface_format).cloned();
        let pip_tex = self.pipelines_textured.get(&surface_format).cloned();

        let mut frame = ctx.frame();
        let mut pass = frame.begin_surface_pass("sprite_batch_pass", clear.map(Self::wgpu_color));
        self.execute_draw_plan(
            &mut *pass,
            &draw_plan,
            pip_color.as_deref(),
            pip_tex.as_deref(),
        );
    }

    /// Draw to the presentation surface while preserving the existing contents.
    pub fn draw_to_surface_loaded(&mut self, ctx: &mut GpuContext, camera: &Camera2D) {
        let surface_format = ctx.surface_format();
        self.ensure_pipelines(ctx, surface_format);
        self.upload(ctx, camera);

        let draw_plan = self.build_draw_plan(ctx);
        let pip_color = self.pipelines_color.get(&surface_format).cloned();
        let pip_tex = self.pipelines_textured.get(&surface_format).cloned();

        let mut frame = ctx.frame();
        let mut pass = frame.begin_surface_pass_loaded("sprite_batch_pass");
        self.execute_draw_plan(
            &mut *pass,
            &draw_plan,
            pip_color.as_deref(),
            pip_tex.as_deref(),
        );
    }

    /// Draw to an off-screen render target.
    ///
    /// `clear`: `Some([r,g,b,a])` clears the target first; `None` preserves
    /// the existing target contents (`LoadOp::Load`).
    pub fn draw_to_target(
        &mut self,
        ctx: &mut GpuContext,
        camera: &Camera2D,
        target: &RenderTarget,
        clear: Option<[f32; 4]>,
    ) {
        let load = match clear {
            Some(c) => wgpu::LoadOp::Clear(Self::wgpu_color(c)),
            None => wgpu::LoadOp::Load,
        };
        self.draw_to_target_with_load(ctx, camera, target, load);
    }

    /// Draw to an off-screen render target while preserving the existing contents.
    pub fn draw_to_target_loaded(
        &mut self,
        ctx: &mut GpuContext,
        camera: &Camera2D,
        target: &RenderTarget,
    ) {
        self.draw_to_target_with_load(ctx, camera, target, wgpu::LoadOp::Load);
    }

    fn draw_to_target_with_load(
        &mut self,
        ctx: &mut GpuContext,
        camera: &Camera2D,
        target: &RenderTarget,
        load: wgpu::LoadOp<wgpu::Color>,
    ) {
        let target_format = target.format();
        self.ensure_pipelines(ctx, target_format);
        self.upload(ctx, camera);

        let draw_plan = self.build_draw_plan(ctx);
        let pip_color = self.pipelines_color.get(&target_format).cloned();
        let pip_tex = self.pipelines_textured.get(&target_format).cloned();

        let mut frame = ctx.frame();
        let mut pass = frame.begin_target_pass("sprite_batch_pass", target, load);
        self.execute_draw_plan(
            &mut *pass,
            &draw_plan,
            pip_color.as_deref(),
            pip_tex.as_deref(),
        );
    }

    #[inline]
    fn wgpu_color(color: [f32; 4]) -> wgpu::Color {
        wgpu::Color {
            r: color[0] as f64,
            g: color[1] as f64,
            b: color[2] as f64,
            a: color[3] as f64,
        }
    }

    fn upload(&self, ctx: &GpuContext, camera: &Camera2D) {
        ctx.queue().write_buffer(
            &self.camera_buffer,
            0,
            bytemuck::bytes_of(&camera.uniform()),
        );
        if !self.instances.is_empty() {
            ctx.queue().write_buffer(
                &self.instance_buffer,
                0,
                bytemuck::cast_slice(&self.instances),
            );
        }
    }

    fn build_draw_plan(&self, ctx: &GpuContext) -> Vec<(bool, Option<wgpu::BindGroup>, u32, u32)> {
        self.finalized_draw_cmds()
            .iter()
            .map(|cmd| {
                let bg = if cmd.textured {
                    let tex = &self.frame_textures[cmd.texture_idx];
                    Some(ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
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
                    }))
                } else {
                    None
                };
                (cmd.textured, bg, cmd.first_instance, cmd.instance_count)
            })
            .collect()
    }

    fn finalized_draw_cmds(&self) -> Vec<DrawCmd> {
        let mut draw_cmds = self.draw_cmds.clone();
        Self::close_pending_cmd(
            &mut draw_cmds,
            self.instances.len() as u32,
            self.current_textured,
            self.current_texture_idx,
        );
        draw_cmds
    }

    fn execute_draw_plan(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        plan: &[(bool, Option<wgpu::BindGroup>, u32, u32)],
        pip_color: Option<&wgpu::RenderPipeline>,
        pip_textured: Option<&wgpu::RenderPipeline>,
    ) {
        pass.set_bind_group(0, &self.camera_bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
        pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);

        for (textured, bind_group, first_instance, instance_count) in plan {
            if *textured {
                let Some(pip) = pip_textured else {
                    continue;
                };
                pass.set_pipeline(pip);
                if let Some(bg) = bind_group {
                    pass.set_bind_group(1, bg, &[]);
                }
            } else {
                let Some(pip) = pip_color else {
                    continue;
                };
                pass.set_pipeline(pip);
            }
            pass.draw_indexed(
                0..6,
                0,
                *first_instance..(*first_instance + *instance_count),
            );
        }
    }

    fn maybe_close_cmd(&mut self) {
        Self::close_pending_cmd(
            &mut self.draw_cmds,
            self.instances.len() as u32,
            self.current_textured,
            self.current_texture_idx,
        );
    }

    fn close_pending_cmd(
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
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &[
                        // Quad vertices
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
                        // Instances
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
                    module: &shader,
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
    use super::{DrawCmd, SpriteBatch};

    #[test]
    fn close_pending_cmd_appends_unclaimed_instances() {
        let mut draw_cmds = vec![DrawCmd {
            first_instance: 0,
            instance_count: 3,
            textured: false,
            texture_idx: 0,
        }];

        SpriteBatch::close_pending_cmd(&mut draw_cmds, 5, true, 2);

        assert_eq!(
            draw_cmds,
            vec![
                DrawCmd {
                    first_instance: 0,
                    instance_count: 3,
                    textured: false,
                    texture_idx: 0,
                },
                DrawCmd {
                    first_instance: 3,
                    instance_count: 2,
                    textured: true,
                    texture_idx: 2,
                },
            ]
        );
    }

    #[test]
    fn close_pending_cmd_does_not_duplicate_completed_work() {
        let mut draw_cmds = vec![DrawCmd {
            first_instance: 0,
            instance_count: 4,
            textured: false,
            texture_idx: 0,
        }];

        SpriteBatch::close_pending_cmd(&mut draw_cmds, 4, true, 1);

        assert_eq!(draw_cmds.len(), 1);
        assert_eq!(draw_cmds[0].instance_count, 4);
    }
}
