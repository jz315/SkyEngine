use super::*;
use cubism_sys::{
    CSM_ALPHA_BLEND_TYPE_OVER, CSM_COLOR_BLEND_TYPE_ADD_COMPATIBLE,
    CSM_COLOR_BLEND_TYPE_MULTIPLY_COMPATIBLE, CSM_COLOR_BLEND_TYPE_NORMAL,
};

impl Live2DRenderer {
    pub(super) fn prepare_composite_draw(
        &mut self,
        ctx: &mut GpuContext,
        model: &Live2DModel,
        offscreen_index: usize,
        offscreen_clipping: Option<&ClippingManager>,
    ) -> (PreparedCompositeDraw, Option<MaskRequest>) {
        let (use_mask, clip_matrix, channel_flag, mask_request) =
            self.get_offscreen_clip_info(offscreen_index, offscreen_clipping);
        let raw_blend = model.offscreen_blend_mode_raw(offscreen_index);
        let pipeline = match compatible_blend_mode(raw_blend) {
            Some(BlendMode::Normal) => self.composite_pipeline_normal.as_ref().unwrap().clone(),
            Some(BlendMode::Additive) => self.composite_pipeline_additive.as_ref().unwrap().clone(),
            Some(BlendMode::Multiplicative) => self
                .composite_pipeline_multiplicative
                .as_ref()
                .unwrap()
                .clone(),
            None => self.composite_pipeline_overlap.as_ref().unwrap().clone(),
        };
        let opacity = model.offscreen_opacity(offscreen_index);
        let uniforms = Live2DCompositeUniforms {
            clip_matrix,
            base_color: [opacity, opacity, opacity, opacity],
            multiply_color: model.offscreen_multiply_color(offscreen_index),
            screen_color: model.offscreen_screen_color(offscreen_index),
            channel_flag,
            use_mask,
            inverted_mask: if model.offscreen_is_inverted_mask(offscreen_index) {
                1.0
            } else {
                0.0
            },
            color_blend_type: raw_blend_to_color_blend_type(raw_blend) as u32,
            alpha_blend_type: raw_blend_to_alpha_blend_type(raw_blend) as u32,
        };
        let uniform_offset = self.composite_uniforms.push_staged(ctx, uniforms);
        let source_target = &self.offscreen_targets[offscreen_index];

        (
            PreparedCompositeDraw {
                pipeline,
                uniform_offset,
                uses_mask: use_mask > 0.5,
                source_texture_id: std::ptr::from_ref(source_target.texture()) as usize,
                source_view: source_target.view().clone(),
            },
            mask_request,
        )
    }

    pub(super) fn prepare_model_draws(
        &mut self,
        ctx: &mut GpuContext,
        model: &Live2DModel,
        textures: &[Texture],
        clipping: Option<&ClippingManager>,
        projection: ModelToClip,
        apply_model_color: bool,
    ) -> Vec<PreparedModelDraw> {
        let mut draws = Vec::new();

        for &draw_idx in model.sorted_drawable_indices() {
            if let (Some(draw), _) = self.prepare_model_draw_for_index(
                ctx,
                model,
                textures,
                draw_idx,
                clipping,
                projection,
                apply_model_color,
            ) {
                draws.push(draw);
            }
        }

        draws
    }

    pub(super) fn create_texture_bind_group(
        &mut self,
        ctx: &GpuContext,
        key: TextureBindGroupKey,
        color_view: &wgpu::TextureView,
        mask_view: &wgpu::TextureView,
        destination_view: Option<&wgpu::TextureView>,
        label: &str,
    ) -> wgpu::BindGroup {
        self.cached_texture_bind_groups
            .entry(key)
            .or_insert_with(|| {
                let layout = if destination_view.is_some() {
                    &self.blend_texture_bgl
                } else {
                    &self.texture_bgl
                };
                let mut entries = vec![
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(color_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(mask_view),
                    },
                ];
                if let Some(destination_view) = destination_view {
                    entries.push(wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(ctx.sampler_linear()),
                    });
                    entries.push(wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(destination_view),
                    });
                } else {
                    entries.push(wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(ctx.sampler_linear()),
                    });
                }
                ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some(label),
                    layout,
                    entries: &entries,
                })
            })
            .clone()
    }

    pub(super) fn create_composite_bind_group(
        &mut self,
        ctx: &GpuContext,
        key: CompositeBindGroupKey,
        source_view: &wgpu::TextureView,
        mask_view: &wgpu::TextureView,
        destination_view: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        self.cached_composite_bind_groups
            .entry(key)
            .or_insert_with(|| {
                ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("live2d_composite_tex_bg"),
                    layout: &self.composite_texture_bgl,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(source_view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(mask_view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::TextureView(destination_view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: wgpu::BindingResource::Sampler(ctx.sampler_linear()),
                        },
                    ],
                })
            })
            .clone()
    }

    pub(super) fn upload_drawable(
        &mut self,
        ctx: &mut GpuContext,
        positions: &[[f32; 2]],
        uvs: &[[f32; 2]],
        indices: &[u16],
    ) -> (UploadSlice, UploadSlice, u32) {
        self.vertex_scratch.clear();
        self.vertex_scratch
            .extend(
                positions
                    .iter()
                    .zip(uvs.iter())
                    .map(|(pos, uv)| Live2DVertex {
                        position: *pos,
                        uv: *uv,
                    }),
            );
        let vertex_upload = ctx.upload_vertices(&self.vertex_scratch);
        let index_upload = ctx.upload_indices_u16(indices);
        (vertex_upload, index_upload, indices.len() as u32)
    }

    pub(super) fn get_drawable_clip_info(
        &self,
        drawable_index: usize,
        clipping: Option<&ClippingManager>,
    ) -> (f32, [f32; 16], [f32; 4], Option<MaskRequest>) {
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
                    ctx_entry.draw_matrix.to_cols_array(),
                    clip_mgr.channel_flags[ctx_entry.channel_index],
                    Some(MaskRequest {
                        kind: ClippingObjectKind::Drawable,
                        context_index: ctx_idx,
                    }),
                );
            }
        }
        (
            0.0,
            [
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
            ],
            [0.0; 4],
            None,
        )
    }

    pub(super) fn get_offscreen_clip_info(
        &self,
        offscreen_index: usize,
        clipping: Option<&ClippingManager>,
    ) -> (f32, [f32; 16], [f32; 4], Option<MaskRequest>) {
        if let Some(clip_mgr) = clipping {
            if let Some(ctx_idx) = clip_mgr
                .offscreen_to_context
                .get(offscreen_index)
                .copied()
                .flatten()
            {
                let ctx_entry = &clip_mgr.contexts[ctx_idx];
                return (
                    1.0,
                    ctx_entry
                        .offscreen_draw_matrix
                        .expect("offscreen clipping matrices must be updated before rendering")
                        .to_cols_array(),
                    clip_mgr.channel_flags[ctx_entry.channel_index],
                    Some(MaskRequest {
                        kind: ClippingObjectKind::Offscreen,
                        context_index: ctx_idx,
                    }),
                );
            }
        }
        (
            0.0,
            [
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
            ],
            [0.0; 4],
            None,
        )
    }

    pub(super) fn ensure_mask_texture(&mut self, ctx: &GpuContext) {
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

    pub(super) fn ensure_complex_targets(
        &mut self,
        ctx: &GpuContext,
        format: wgpu::TextureFormat,
        target_size: [u32; 2],
        offscreen_count: usize,
    ) {
        let width = target_size[0].max(1);
        let height = target_size[1].max(1);
        let mut targets_changed = false;

        if let Some(target) = self.root_intermediate_target.as_mut() {
            if target.width() != width || target.height() != height || target.format() != format {
                target.resize(ctx, width, height, format);
                targets_changed = true;
            }
        } else {
            self.root_intermediate_target = Some(RenderTarget::new(
                ctx,
                width,
                height,
                format,
                "live2d_root_intermediate",
            ));
            targets_changed = true;
        }

        if let Some(target) = self.blend_backup_target.as_mut() {
            if target.width() != width || target.height() != height || target.format() != format {
                target.resize(ctx, width, height, format);
                targets_changed = true;
            }
        } else {
            self.blend_backup_target = Some(RenderTarget::new(
                ctx,
                width,
                height,
                format,
                "live2d_blend_backup",
            ));
            targets_changed = true;
        }

        if self.offscreen_targets.len() < offscreen_count {
            for index in self.offscreen_targets.len()..offscreen_count {
                self.offscreen_targets.push(RenderTarget::new(
                    ctx,
                    width,
                    height,
                    format,
                    format!("live2d_offscreen_{index}"),
                ));
            }
            targets_changed = true;
        } else if self.offscreen_targets.len() > offscreen_count {
            self.offscreen_targets.truncate(offscreen_count);
            targets_changed = true;
        }

        for target in &mut self.offscreen_targets {
            if target.width() != width || target.height() != height || target.format() != format {
                target.resize(ctx, width, height, format);
                targets_changed = true;
            }
        }

        if targets_changed {
            self.cached_composite_bind_groups.clear();
            self.cached_blit_bind_groups.clear();
            self.cached_texture_bind_groups.clear();
        }
    }

    pub(super) fn ensure_offscreen_clipping(&mut self, model: &Live2DModel) {
        let model_key = model.raw_model_ptr() as usize;
        if self.cached_offscreen_clipping_model == Some(model_key) {
            return;
        }

        let clipping = if model.offscreen_count() > 0 {
            let manager = ClippingManager::new_for_offscreens(model);
            if manager.has_masks() {
                Some(manager)
            } else {
                None
            }
        } else {
            None
        };
        self.cached_offscreen_clipping_model = Some(model_key);
        self.cached_offscreen_clipping = clipping;
    }

    pub(super) fn ensure_pipelines(&mut self, ctx: &GpuContext, format: wgpu::TextureFormat) {
        if self.cached_format == Some(format) {
            return;
        }
        self.cached_format = Some(format);

        let pipeline_layout =
            ctx.device()
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("live2d_pipeline_layout"),
                    bind_group_layouts: &[Some(&self.uniform_bgl), Some(&self.texture_bgl)],
                    immediate_size: 0,
                });
        let overlap_pipeline_layout =
            ctx.device()
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("live2d_overlap_pipeline_layout"),
                    bind_group_layouts: &[Some(&self.uniform_bgl), Some(&self.blend_texture_bgl)],
                    immediate_size: 0,
                });
        let composite_pipeline_layout =
            ctx.device()
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("live2d_composite_pipeline_layout"),
                    bind_group_layouts: &[
                        Some(&self.composite_uniform_bgl),
                        Some(&self.composite_texture_bgl),
                    ],
                    immediate_size: 0,
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
                             cull_mode: Option<wgpu::Face>,
                             fragment_entry: &str|
         -> wgpu::RenderPipeline {
            ctx.device()
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some(label),
                    layout: Some(&pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &self.shader,
                        entry_point: Some("vs_main"),
                        buffers: std::slice::from_ref(&vertex_layout),
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
                        front_face: wgpu::FrontFace::Ccw,
                        cull_mode,
                        ..Default::default()
                    },
                    depth_stencil: None,
                    multisample: wgpu::MultisampleState::default(),
                    multiview_mask: None,
                    cache: None,
                })
        };
        let make_composite_pipeline = |label: &str,
                                       blend: Option<wgpu::BlendState>,
                                       fmt: wgpu::TextureFormat,
                                       fragment_entry: &str| {
            ctx.device()
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some(label),
                    layout: Some(&composite_pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &self.composite_shader,
                        entry_point: Some("vs_main"),
                        buffers: &[],
                        compilation_options: Default::default(),
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &self.composite_shader,
                        entry_point: Some(fragment_entry),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: fmt,
                            blend,
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
        };
        let make_overlap_pipeline = |label: &str, fmt: wgpu::TextureFormat, cull_mode| {
            ctx.device()
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some(label),
                    layout: Some(&overlap_pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &self.shader,
                        entry_point: Some("vs_main"),
                        buffers: std::slice::from_ref(&vertex_layout),
                        compilation_options: Default::default(),
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &self.shader,
                        entry_point: Some("model_overlap_fs"),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: fmt,
                            blend: None,
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                        compilation_options: Default::default(),
                    }),
                    primitive: wgpu::PrimitiveState {
                        topology: wgpu::PrimitiveTopology::TriangleList,
                        front_face: wgpu::FrontFace::Ccw,
                        cull_mode,
                        ..Default::default()
                    },
                    depth_stencil: None,
                    multisample: wgpu::MultisampleState::default(),
                    multiview_mask: None,
                    cache: None,
                })
        };

        self.model_pipeline_normal = Some(make_pipeline(
            "live2d_normal",
            blend_normal,
            format,
            None,
            "model_fs",
        ));
        self.model_pipeline_normal_culled = Some(make_pipeline(
            "live2d_normal_culled",
            blend_normal,
            format,
            Some(wgpu::Face::Back),
            "model_fs",
        ));
        self.model_pipeline_additive = Some(make_pipeline(
            "live2d_additive",
            blend_additive,
            format,
            None,
            "model_fs",
        ));
        self.model_pipeline_additive_culled = Some(make_pipeline(
            "live2d_additive_culled",
            blend_additive,
            format,
            Some(wgpu::Face::Back),
            "model_fs",
        ));
        self.model_pipeline_multiplicative = Some(make_pipeline(
            "live2d_multiplicative",
            blend_multiplicative,
            format,
            None,
            "model_fs",
        ));
        self.model_pipeline_multiplicative_culled = Some(make_pipeline(
            "live2d_multiplicative_culled",
            blend_multiplicative,
            format,
            Some(wgpu::Face::Back),
            "model_fs",
        ));
        self.model_pipeline_overlap = Some(make_overlap_pipeline("live2d_overlap", format, None));
        self.model_pipeline_overlap_culled = Some(make_overlap_pipeline(
            "live2d_overlap_culled",
            format,
            Some(wgpu::Face::Back),
        ));
        self.mask_pipeline = Some(make_pipeline(
            "live2d_mask",
            blend_mask,
            wgpu::TextureFormat::Rgba8Unorm,
            None,
            "mask_fs",
        ));
        self.mask_pipeline_culled = Some(make_pipeline(
            "live2d_mask_culled",
            blend_mask,
            wgpu::TextureFormat::Rgba8Unorm,
            Some(wgpu::Face::Back),
            "mask_fs",
        ));
        self.composite_pipeline_normal = Some(make_composite_pipeline(
            "live2d_composite_normal",
            Some(blend_normal),
            format,
            "compatible_fs",
        ));
        self.composite_pipeline_additive = Some(make_composite_pipeline(
            "live2d_composite_additive",
            Some(blend_additive),
            format,
            "compatible_fs",
        ));
        self.composite_pipeline_multiplicative = Some(make_composite_pipeline(
            "live2d_composite_multiplicative",
            Some(blend_multiplicative),
            format,
            "compatible_fs",
        ));
        self.composite_pipeline_overlap = Some(make_composite_pipeline(
            "live2d_composite_overlap",
            None,
            format,
            "overlap_fs",
        ));
    }
}

pub(super) fn raw_blend_to_color_blend_type(raw_blend: i32) -> i32 {
    raw_blend & 0xFF
}

pub(super) fn raw_blend_to_alpha_blend_type(raw_blend: i32) -> i32 {
    (raw_blend >> 8) & 0xFF
}

pub(super) fn compatible_blend_mode(raw_blend: i32) -> Option<BlendMode> {
    match (
        raw_blend_to_color_blend_type(raw_blend),
        raw_blend_to_alpha_blend_type(raw_blend),
    ) {
        (CSM_COLOR_BLEND_TYPE_NORMAL, CSM_ALPHA_BLEND_TYPE_OVER) => Some(BlendMode::Normal),
        (CSM_COLOR_BLEND_TYPE_ADD_COMPATIBLE, _) => Some(BlendMode::Additive),
        (CSM_COLOR_BLEND_TYPE_MULTIPLY_COMPATIBLE, _) => Some(BlendMode::Multiplicative),
        _ => None,
    }
}

pub(super) fn clear_render_target(ctx: &mut GpuContext, target: &RenderTarget, label: &str) {
    let mut frame = ctx.frame();
    let _pass =
        frame.begin_target_pass(label, target, wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT));
}

#[cfg(test)]
mod tests {
    use super::*;
    use cubism_sys::{
        CSM_ALPHA_BLEND_TYPE_ATOP, CSM_ALPHA_BLEND_TYPE_CONJOINT_OVER,
        CSM_ALPHA_BLEND_TYPE_DISJOINT_OVER, CSM_COLOR_BLEND_TYPE_ADD,
        CSM_COLOR_BLEND_TYPE_ADD_GLOW,
    };

    fn create_test_device() -> (wgpu::Device, wgpu::Queue) {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .expect("No suitable GPU adapter found for Live2D renderer tests");

        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("live2d_renderer_test_device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::Performance,
            ..Default::default()
        }))
        .expect("Failed to create test GPU device")
    }

    fn sample_model() -> Live2DModel {
        let moc_bytes = std::fs::read(
            "CubismSdkForNative/CubismSdkForNative-5-r.5/Samples/Resources/Haru/Haru.moc3",
        )
        .expect("sample moc3 should exist");
        Live2DModel::from_moc3_bytes(&moc_bytes).expect("sample moc3 should load")
    }

    #[test]
    fn compatible_blend_only_accepts_framework_compatible_modes() {
        assert_eq!(
            compatible_blend_mode(CSM_COLOR_BLEND_TYPE_NORMAL),
            Some(BlendMode::Normal)
        );
        assert_eq!(
            compatible_blend_mode(CSM_COLOR_BLEND_TYPE_ADD_COMPATIBLE),
            Some(BlendMode::Additive)
        );
        assert_eq!(
            compatible_blend_mode(CSM_COLOR_BLEND_TYPE_MULTIPLY_COMPATIBLE),
            Some(BlendMode::Multiplicative)
        );

        let normal_atop = (CSM_ALPHA_BLEND_TYPE_ATOP << 8) | CSM_COLOR_BLEND_TYPE_NORMAL;
        let add = CSM_COLOR_BLEND_TYPE_ADD;
        let add_glow = CSM_COLOR_BLEND_TYPE_ADD_GLOW;
        let conjoint_over = (CSM_ALPHA_BLEND_TYPE_CONJOINT_OVER << 8) | CSM_COLOR_BLEND_TYPE_NORMAL;

        assert_eq!(compatible_blend_mode(normal_atop), None);
        assert_eq!(compatible_blend_mode(add), None);
        assert_eq!(compatible_blend_mode(add_glow), None);
        assert_eq!(compatible_blend_mode(conjoint_over), None);
    }

    #[test]
    fn raw_blend_splits_color_and_alpha_channels() {
        let raw = (CSM_ALPHA_BLEND_TYPE_DISJOINT_OVER << 8) | 15;
        assert_eq!(raw_blend_to_color_blend_type(raw), 15);
        assert_eq!(
            raw_blend_to_alpha_blend_type(raw),
            CSM_ALPHA_BLEND_TYPE_DISJOINT_OVER
        );
    }

    #[test]
    fn compatible_blend_constants_match_official_core_values() {
        assert_eq!(CSM_COLOR_BLEND_TYPE_NORMAL, 0);
        assert_eq!(CSM_COLOR_BLEND_TYPE_ADD_COMPATIBLE, 1);
        assert_eq!(CSM_COLOR_BLEND_TYPE_MULTIPLY_COMPATIBLE, 2);
        assert_eq!(CSM_COLOR_BLEND_TYPE_ADD, 3);
        assert_eq!(CSM_COLOR_BLEND_TYPE_ADD_GLOW, 4);
        assert_eq!(CSM_ALPHA_BLEND_TYPE_OVER, 0);
        assert_eq!(CSM_ALPHA_BLEND_TYPE_ATOP, 1);
        assert_eq!(CSM_ALPHA_BLEND_TYPE_CONJOINT_OVER, 3);
        assert_eq!(CSM_ALPHA_BLEND_TYPE_DISJOINT_OVER, 4);
    }

    #[test]
    fn zero_opacity_drawables_are_skipped_in_model_prepare_path() {
        let (device, queue) = create_test_device();
        let mut ctx =
            GpuContext::new_headless(device, queue, wgpu::TextureFormat::Rgba8Unorm, [64, 64]);
        let mut renderer = Live2DRenderer::new(&ctx);
        let mut model = sample_model();
        let format = ctx.surface_format();

        renderer.ensure_pipelines(&ctx, format);
        renderer.ensure_mask_texture(&ctx);

        let drawable_index = (0..model.drawable_count())
            .find(|&index| {
                model.drawable_is_visible(index)
                    && model.drawable_opacity(index) > f32::EPSILON
                    && !model.drawable_vertex_positions(index).is_empty()
                    && !model.drawable_indices(index).is_empty()
                    && model.drawable_parent_part_index(index) >= 0
            })
            .expect("sample model should have a visible drawable with a parent part");
        let parent_part_index = model.drawable_parent_part_index(drawable_index) as usize;

        model.set_part_opacity(parent_part_index, 0.0);
        model.update();
        assert!(
            model.drawable_opacity(drawable_index) <= f32::EPSILON,
            "test expects part opacity to zero out drawable opacity"
        );

        let projection = model.render_matrix_for_view(64.0, 64.0);
        let (draw, mask_request) = renderer.prepare_model_draw_for_index(
            &mut ctx,
            &model,
            &[],
            drawable_index,
            None,
            ModelToClip::from_cols_array(projection),
            false,
        );

        assert!(draw.is_none());
        assert!(mask_request.is_none());
    }

    #[test]
    fn missing_texture_drawables_are_skipped_in_model_prepare_path() {
        let (device, queue) = create_test_device();
        let mut ctx =
            GpuContext::new_headless(device, queue, wgpu::TextureFormat::Rgba8Unorm, [64, 64]);
        let mut renderer = Live2DRenderer::new(&ctx);
        let model = sample_model();
        let format = ctx.surface_format();

        renderer.ensure_pipelines(&ctx, format);
        renderer.ensure_mask_texture(&ctx);

        let drawable_index = (0..model.drawable_count())
            .find(|&index| {
                model.drawable_is_visible(index)
                    && model.drawable_opacity(index) > f32::EPSILON
                    && !model.drawable_vertex_positions(index).is_empty()
                    && !model.drawable_indices(index).is_empty()
            })
            .expect("sample model should have a drawable that can render");

        let projection = model.render_matrix_for_view(64.0, 64.0);
        let (draw, mask_request) = renderer.prepare_model_draw_for_index(
            &mut ctx,
            &model,
            &[],
            drawable_index,
            None,
            ModelToClip::from_cols_array(projection),
            false,
        );

        assert!(draw.is_none());
        assert!(mask_request.is_none());
    }

    #[test]
    fn missing_texture_mask_drawables_are_skipped() {
        let (device, queue) = create_test_device();
        let mut ctx =
            GpuContext::new_headless(device, queue, wgpu::TextureFormat::Rgba8Unorm, [64, 64]);
        let mut renderer = Live2DRenderer::new(&ctx);
        let model = sample_model();
        let clipping = ClippingManager::new(&model);

        renderer.ensure_pipelines(&ctx, ctx.surface_format());
        renderer.ensure_mask_texture(&ctx);

        assert!(
            clipping.has_masks(),
            "sample model should exercise mask rendering"
        );
        let draws = renderer.prepare_mask_draws(&mut ctx, &model, &[], &clipping);
        assert!(draws.is_empty());
    }
}
