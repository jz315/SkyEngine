//! GPU renderer for Live2D models.
//!
//! Follows the `SpriteBatch` pattern: create with `GpuContext`, own pipelines
//! and buffers, per-frame upload + draw. Ported from SakuraEngine's
//! `Live2DRendererImpl` and the two render passes (mask + model).

use crate::gpu::{DynamicUniformBuffer, GpuContext, UploadSlice};

use crate::render::core::target::RenderTarget;
use crate::render::core::texture::Texture;
use crate::render::live2d::clipping::{ClippingManager, MASK_RESOLUTION};
use crate::render::live2d::model::{BlendMode, Live2DModel};

/// Per-drawable uniform data uploaded to the GPU.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Live2DUniforms {
    projection_matrix: [f32; 16],
    clip_matrix: [f32; 16],
    base_color: [f32; 4],
    multiply_color: [f32; 4],
    screen_color: [f32; 4],
    channel_flag: [f32; 4],
    use_mask: f32,
    _pad: [f32; 3],
}

/// Live2D vertex (position + UV).
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Live2DVertex {
    position: [f32; 2],
    uv: [f32; 2],
}

struct PreparedMaskDraw {
    vertex_upload: UploadSlice,
    index_upload: UploadSlice,
    index_count: u32,
    uniform_offset: u32,
    texture_bg: wgpu::BindGroup,
}

struct PreparedModelDraw {
    pipeline: wgpu::RenderPipeline,
    vertex_upload: UploadSlice,
    index_upload: UploadSlice,
    index_count: u32,
    uniform_offset: u32,
    texture_bg: wgpu::BindGroup,
}

/// GPU renderer for Live2D models.
///
/// # Usage
/// ```no_run
/// let mut renderer = Live2DRenderer::new(&ctx);
/// // In frame loop:
/// renderer.draw_model(&mut ctx, &camera, &model, &textures, &mut clipping_mgr);
/// ```
pub struct Live2DRenderer {
    shader: wgpu::ShaderModule,
    uniforms: DynamicUniformBuffer<Live2DUniforms>,
    uniform_bgl: wgpu::BindGroupLayout,
    texture_bgl: wgpu::BindGroupLayout,
    // 1x1 dummy texture for mask pass (avoids texture usage conflict)
    dummy_texture: Texture,
    // Model pass pipelines (per blend mode × format)
    model_pipeline_normal: Option<wgpu::RenderPipeline>,
    model_pipeline_additive: Option<wgpu::RenderPipeline>,
    model_pipeline_multiplicative: Option<wgpu::RenderPipeline>,
    // Mask pass pipeline
    mask_pipeline: Option<wgpu::RenderPipeline>,
    // Mask texture (created on first use)
    mask_texture: Option<RenderTarget>,
    // Cached target format
    cached_format: Option<wgpu::TextureFormat>,
}

impl Live2DRenderer {
    /// Create a new Live2D renderer.
    pub fn new(ctx: &GpuContext) -> Self {
        let shader = ctx
            .device()
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("live2d_shader"),
                source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/live2d.wgsl").into()),
            });
        let uniforms = DynamicUniformBuffer::new(
            ctx,
            "live2d_uniform_buf",
            wgpu::ShaderStages::VERTEX_FRAGMENT,
        );
        let uniform_bgl = uniforms.bind_group_layout().clone();

        // Texture bind group layout (group 1): color_texture + mask_texture + sampler
        let texture_bgl = ctx
            .device()
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("live2d_texture_bgl"),
                entries: &[
                    // color_texture
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
                    // mask_texture
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    // sampler
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        // 1x1 white dummy texture for mask-pass bind groups
        let dummy_texture = Texture::from_rgba8_with_format(
            ctx,
            1,
            1,
            &[255, 255, 255, 255],
            wgpu::TextureFormat::Rgba8Unorm,
            "live2d_dummy",
        );

        Self {
            shader,
            uniforms,
            uniform_bgl,
            texture_bgl,
            dummy_texture,
            model_pipeline_normal: None,
            model_pipeline_additive: None,
            model_pipeline_multiplicative: None,
            mask_pipeline: None,
            mask_texture: None,
            cached_format: None,
        }
    }

    /// Draw a Live2D model to the current surface.
    ///
    /// This is the main entry point. It:
    /// 1. Renders mask pass (if model uses clipping)
    /// 2. Renders model pass (all drawables in sorted render order)
    pub fn draw_to_surface(
        &mut self,
        ctx: &mut GpuContext,
        model: &Live2DModel,
        textures: &[Texture],
        clipping: &mut Option<ClippingManager>,
    ) {
        let format = ctx.surface_format();
        self.ensure_pipelines(ctx, format);
        self.ensure_mask_texture(ctx);

        let [w, h] = ctx.surface_size();
        let projection = Self::make_live2d_projection(w as f32, h as f32, model);

        // Update clipping matrices
        if let Some(ref mut clip_mgr) = clipping {
            clip_mgr.update_matrices(model);
        }
        self.uniforms.clear();

        let mask_draws = clipping
            .as_ref()
            .filter(|clip_mgr| clip_mgr.has_masks())
            .map(|clip_mgr| self.prepare_mask_draws(ctx, model, textures, clip_mgr))
            .unwrap_or_default();
        let model_draws =
            self.prepare_model_draws(ctx, model, textures, clipping.as_ref(), &projection);

        self.render_mask_pass(ctx, &mask_draws);
        self.render_model_pass_surface(ctx, &model_draws);
    }

    /// Draw a Live2D model to an off-screen render target.
    pub fn draw_to_target(
        &mut self,
        ctx: &mut GpuContext,
        target: &RenderTarget,
        model: &Live2DModel,
        textures: &[Texture],
        clipping: &mut Option<ClippingManager>,
    ) {
        let format = target.format();
        self.ensure_pipelines(ctx, format);
        self.ensure_mask_texture(ctx);

        let projection =
            Self::make_live2d_projection(target.width() as f32, target.height() as f32, model);

        if let Some(ref mut clip_mgr) = clipping {
            clip_mgr.update_matrices(model);
        }
        self.uniforms.clear();

        let mask_draws = clipping
            .as_ref()
            .filter(|clip_mgr| clip_mgr.has_masks())
            .map(|clip_mgr| self.prepare_mask_draws(ctx, model, textures, clip_mgr))
            .unwrap_or_default();
        let model_draws =
            self.prepare_model_draws(ctx, model, textures, clipping.as_ref(), &projection);

        self.render_mask_pass(ctx, &mask_draws);
        self.render_model_pass_target(ctx, &model_draws, target);
    }

    /// Build the Live2D projection matrix.
    ///
    /// Live2D vertices are authored in model space, while clip-space X and Y
    /// scale differently on non-square viewports. SakuraEngine handles this
    /// through its render view / screen-rect path; here we apply the equivalent
    /// aspect compensation directly in the projection matrix.
    fn make_live2d_projection(screen_w: f32, screen_h: f32, _model: &Live2DModel) -> [f32; 16] {
        let (scale_x, scale_y) = if screen_w > screen_h {
            (screen_h / screen_w.max(f32::EPSILON), 1.0)
        } else {
            (1.0, screen_w / screen_h.max(f32::EPSILON))
        };
        #[rustfmt::skip]
        let mat: [f32; 16] = [
            scale_x, 0.0, 0.0, 0.0,
            0.0, scale_y, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ];
        mat
    }

    // ── Mask pass ───────────────────────────────────────────────────────

    fn render_mask_pass(&mut self, ctx: &mut GpuContext, draws: &[PreparedMaskDraw]) {
        if draws.is_empty() {
            return;
        }

        let mask_pipeline = self.mask_pipeline.as_ref().unwrap();
        let mask_target = self.mask_texture.as_ref().unwrap();
        let mut frame = ctx.frame();
        let mut pass = frame.begin_target_pass(
            "live2d_mask_pass",
            mask_target,
            wgpu::LoadOp::Clear(wgpu::Color::WHITE),
        );
        pass.set_pipeline(mask_pipeline);

        for draw in draws {
            pass.set_bind_group(0, self.uniforms.bind_group(), &[draw.uniform_offset]);
            pass.set_bind_group(1, &draw.texture_bg, &[]);
            pass.set_vertex_buffer(0, draw.vertex_upload.slice());
            pass.set_index_buffer(draw.index_upload.slice(), wgpu::IndexFormat::Uint16);
            pass.draw_indexed(0..draw.index_count, 0, 0..1);
        }
    }

    // ── Model pass (surface) ────────────────────────────────────────────

    fn render_model_pass_surface(&mut self, ctx: &mut GpuContext, draws: &[PreparedModelDraw]) {
        if draws.is_empty() {
            return;
        }

        let mut frame = ctx.frame();
        let mut pass = frame.begin_surface_pass_loaded("live2d_model_pass");
        self.execute_model_draws(&mut *pass, draws);
    }

    // ── Model pass (render target) ──────────────────────────────────────

    fn render_model_pass_target(
        &mut self,
        ctx: &mut GpuContext,
        draws: &[PreparedModelDraw],
        target: &RenderTarget,
    ) {
        if draws.is_empty() {
            return;
        }

        let mut frame = ctx.frame();
        let mut pass = frame.begin_target_pass("live2d_model_pass", target, wgpu::LoadOp::Load);
        self.execute_model_draws(&mut *pass, draws);
    }

    // ── Helpers ──────────────────────────────────────────────────────────

    fn prepare_mask_draws(
        &mut self,
        ctx: &mut GpuContext,
        model: &Live2DModel,
        textures: &[Texture],
        clipping: &ClippingManager,
    ) -> Vec<PreparedMaskDraw> {
        let mut draws = Vec::new();

        for ctx_entry in &clipping.contexts {
            for &mask_idx in &ctx_entry.mask_drawable_indices {
                if !model.drawable_is_visible(mask_idx) {
                    continue;
                }

                let positions = model.drawable_vertex_positions(mask_idx);
                let uvs = model.drawable_vertex_uvs(mask_idx);
                let indices = model.drawable_indices(mask_idx);
                if positions.is_empty() || indices.is_empty() {
                    continue;
                }

                let lb = &ctx_entry.layout_bounds; // [x, y, w, h]
                let uniforms = Live2DUniforms {
                    projection_matrix: ctx_entry.mask_matrix,
                    clip_matrix: ctx_entry.mask_matrix,
                    // Convert layout bounds [x,y,w,h] → NDC [x_min, y_min, x_max, y_max]
                    // to match the NDC-space clip_pos used in the mask_fs bounds test.
                    base_color: [
                        2.0 * lb[0] - 1.0,
                        2.0 * lb[1] - 1.0,
                        2.0 * (lb[0] + lb[2]) - 1.0,
                        2.0 * (lb[1] + lb[3]) - 1.0,
                    ],
                    multiply_color: [1.0, 1.0, 1.0, 1.0],
                    screen_color: [0.0, 0.0, 0.0, 0.0],
                    channel_flag: clipping.channel_flags[ctx_entry.channel_index],
                    use_mask: 0.0,
                    _pad: [0.0; 3],
                };
                let uniform_offset = self.uniforms.push(ctx, uniforms);
                let (vertex_upload, index_upload, index_count) =
                    self.upload_drawable(ctx, positions, uvs, indices);

                let tex_idx = model.drawable_texture_index(mask_idx) as usize;
                let color_view = if tex_idx < textures.len() {
                    textures[tex_idx].view()
                } else {
                    textures[0].view()
                };

                draws.push(PreparedMaskDraw {
                    vertex_upload,
                    index_upload,
                    index_count,
                    uniform_offset,
                    texture_bg: self.create_texture_bind_group(
                        ctx,
                        color_view,
                        self.dummy_texture.view(),
                        "live2d_mask_tex_bg",
                    ),
                });
            }
        }

        draws
    }

    fn prepare_model_draws(
        &mut self,
        ctx: &mut GpuContext,
        model: &Live2DModel,
        textures: &[Texture],
        clipping: Option<&ClippingManager>,
        projection: &[f32; 16],
    ) -> Vec<PreparedModelDraw> {
        let mut draws = Vec::new();
        let mask_view = self.mask_texture.as_ref().map(|t| t.view()).unwrap();

        for &draw_idx in model.sorted_drawable_indices() {
            if !model.drawable_is_visible(draw_idx) {
                continue;
            }

            let positions = model.drawable_vertex_positions(draw_idx);
            let uvs = model.drawable_vertex_uvs(draw_idx);
            let indices = model.drawable_indices(draw_idx);
            if positions.is_empty() || indices.is_empty() {
                continue;
            }

            let info = model.drawable_info(draw_idx);
            let (use_mask, clip_matrix, channel_flag) = self.get_clip_info(draw_idx, clipping);
            let uniforms = Live2DUniforms {
                projection_matrix: *projection,
                clip_matrix,
                base_color: [1.0, 1.0, 1.0, info.opacity],
                multiply_color: info.multiply_color,
                screen_color: info.screen_color,
                channel_flag,
                use_mask,
                _pad: [0.0; 3],
            };
            let uniform_offset = self.uniforms.push(ctx, uniforms);
            let (vertex_upload, index_upload, index_count) =
                self.upload_drawable(ctx, positions, uvs, indices);

            let tex_idx = info.texture_index as usize;
            let color_view = if tex_idx < textures.len() {
                textures[tex_idx].view()
            } else {
                textures[0].view()
            };
            let pipeline = match info.blend_mode {
                BlendMode::Normal => self.model_pipeline_normal.as_ref().unwrap().clone(),
                BlendMode::Additive => self.model_pipeline_additive.as_ref().unwrap().clone(),
                BlendMode::Multiplicative => {
                    self.model_pipeline_multiplicative.as_ref().unwrap().clone()
                }
            };

            draws.push(PreparedModelDraw {
                pipeline,
                vertex_upload,
                index_upload,
                index_count,
                uniform_offset,
                texture_bg: self.create_texture_bind_group(
                    ctx,
                    color_view,
                    mask_view,
                    "live2d_model_tex_bg",
                ),
            });
        }

        draws
    }

    fn execute_model_draws(&self, pass: &mut wgpu::RenderPass<'_>, draws: &[PreparedModelDraw]) {
        for draw in draws {
            pass.set_pipeline(&draw.pipeline);
            pass.set_bind_group(0, self.uniforms.bind_group(), &[draw.uniform_offset]);
            pass.set_bind_group(1, &draw.texture_bg, &[]);
            pass.set_vertex_buffer(0, draw.vertex_upload.slice());
            pass.set_index_buffer(draw.index_upload.slice(), wgpu::IndexFormat::Uint16);
            pass.draw_indexed(0..draw.index_count, 0, 0..1);
        }
    }

    fn create_texture_bind_group(
        &self,
        ctx: &GpuContext,
        color_view: &wgpu::TextureView,
        mask_view: &wgpu::TextureView,
        label: &str,
    ) -> wgpu::BindGroup {
        ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(label),
            layout: &self.texture_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(color_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(mask_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(ctx.sampler_linear()),
                },
            ],
        })
    }

    fn upload_drawable(
        &self,
        ctx: &mut GpuContext,
        positions: &[[f32; 2]],
        uvs: &[[f32; 2]],
        indices: &[u16],
    ) -> (UploadSlice, UploadSlice, u32) {
        let vertices: Vec<Live2DVertex> = positions
            .iter()
            .zip(uvs.iter())
            .map(|(pos, uv)| Live2DVertex {
                position: *pos,
                uv: *uv,
            })
            .collect();
        let vertex_upload = ctx.upload_vertices(&vertices);
        let index_upload = ctx.upload_indices_u16(indices);
        (vertex_upload, index_upload, indices.len() as u32)
    }

    fn get_clip_info(
        &self,
        drawable_index: usize,
        clipping: Option<&ClippingManager>,
    ) -> (f32, [f32; 16], [f32; 4]) {
        if let Some(clip_mgr) = clipping {
            if let Some(ctx_idx) = clip_mgr
                .drawable_to_context
                .get(drawable_index)
                .copied()
                .flatten()
            {
                let ctx_entry = &clip_mgr.contexts[ctx_idx];
                return (
                    1.0,
                    ctx_entry.draw_matrix,
                    clip_mgr.channel_flags[ctx_entry.channel_index],
                );
            }
        }
        (
            0.0,
            [
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
            ],
            [0.0; 4],
        )
    }

    fn ensure_mask_texture(&mut self, ctx: &GpuContext) {
        if self.mask_texture.is_some() {
            return;
        }
        self.mask_texture = Some(RenderTarget::new(
            ctx,
            MASK_RESOLUTION,
            MASK_RESOLUTION,
            wgpu::TextureFormat::Rgba8Unorm,
            "live2d_mask",
        ));
    }

    fn ensure_pipelines(&mut self, ctx: &GpuContext, format: wgpu::TextureFormat) {
        if self.cached_format == Some(format) {
            return;
        }
        self.cached_format = Some(format);

        let pipeline_layout =
            ctx.device()
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("live2d_pipeline_layout"),
                    bind_group_layouts: &[&self.uniform_bgl, &self.texture_bgl],
                    push_constant_ranges: &[],
                });

        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Live2DVertex>() as u64,
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
        };

        // Normal blend: premultiplied alpha (SrcOne, OneMinusSrcAlpha)
        let blend_normal = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
        };

        // Additive blend
        let blend_additive = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Zero,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        };

        // Multiplicative blend
        let blend_multiplicative = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Dst,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Zero,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        };

        // Mask blend: SrcZero, DstOneMinusSrcColor (inverted mask accumulation)
        let blend_mask = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Zero,
                dst_factor: wgpu::BlendFactor::OneMinusSrc,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Zero,
                dst_factor: wgpu::BlendFactor::OneMinusSrc,
                operation: wgpu::BlendOperation::Add,
            },
        };

        let make_pipeline = |label: &str,
                             blend: wgpu::BlendState,
                             fmt: wgpu::TextureFormat,
                             fragment_entry: &str|
         -> wgpu::RenderPipeline {
            ctx.device()
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some(label),
                    layout: Some(&pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &self.shader,
                        entry_point: Some("vs_main"),
                        buffers: &[vertex_layout.clone()],
                        compilation_options: Default::default(),
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &self.shader,
                        entry_point: Some(fragment_entry),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: fmt,
                            blend: Some(blend),
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                        compilation_options: Default::default(),
                    }),
                    primitive: wgpu::PrimitiveState {
                        topology: wgpu::PrimitiveTopology::TriangleList,
                        cull_mode: None, // Live2D drawables can be double-sided
                        ..Default::default()
                    },
                    depth_stencil: None,
                    multisample: wgpu::MultisampleState::default(),
                    multiview: None,
                    cache: None,
                })
        };

        self.model_pipeline_normal = Some(make_pipeline(
            "live2d_normal",
            blend_normal,
            format,
            "model_fs",
        ));
        self.model_pipeline_additive = Some(make_pipeline(
            "live2d_additive",
            blend_additive,
            format,
            "model_fs",
        ));
        self.model_pipeline_multiplicative = Some(make_pipeline(
            "live2d_multiplicative",
            blend_multiplicative,
            format,
            "model_fs",
        ));
        self.mask_pipeline = Some(make_pipeline(
            "live2d_mask",
            blend_mask,
            wgpu::TextureFormat::Rgba8Unorm,
            "mask_fs",
        ));
    }
}
