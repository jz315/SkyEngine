use super::composite::{
    compatible_blend_mode, raw_blend_to_alpha_blend_type, raw_blend_to_color_blend_type,
};
use super::*;

impl Live2DRenderer {
    pub(super) fn prepare_frame(
        &mut self,
        ctx: &mut GpuContext,
        format: wgpu::TextureFormat,
        target_size: [u32; 2],
        projection: &[f32; 16],
        model: &Live2DModel,
        textures: &[Texture],
        clipping: &mut Option<ClippingManager>,
    ) -> PreparedLive2DFrame {
        self.ensure_pipelines(ctx, format);
        self.ensure_mask_texture(ctx);
        let uses_complex_root_composite =
            model.offscreen_count() > 0 || model_uses_advanced_blend(model);
        if uses_complex_root_composite {
            self.ensure_complex_targets(ctx, format, target_size, model.offscreen_count());
        }

        if let Some(ref mut clip_mgr) = clipping {
            clip_mgr.update_matrices(model);
        }
        self.uniforms.clear();
        self.composite_uniforms.clear();

        let prepared = if model.offscreen_count() > 0 {
            self.prepare_frame_with_offscreens(
                ctx,
                format,
                target_size,
                projection,
                model,
                textures,
                clipping,
            )
        } else {
            let mask_draws = clipping
                .as_ref()
                .filter(|clip_mgr| clip_mgr.has_masks())
                .map(|clip_mgr| self.prepare_mask_draws(ctx, model, textures, clip_mgr))
                .unwrap_or_default();
            let model_draws = self.prepare_model_draws(
                ctx,
                model,
                textures,
                clipping.as_ref(),
                projection,
                !uses_complex_root_composite,
            );

            PreparedLive2DFrame::new(
                format,
                vec![PreparedTargetPass {
                    target: PreparedPassTarget::Root,
                    clear: false,
                    requires_mask: clipping.as_ref().is_some_and(ClippingManager::has_masks),
                    mask_draws,
                    items: model_draws
                        .into_iter()
                        .map(PreparedTargetItem::Drawable)
                        .collect(),
                }],
                if uses_complex_root_composite {
                    model.model_opacity()
                } else {
                    1.0
                },
            )
        };

        self.uniforms.upload_all(ctx);
        self.composite_uniforms.upload_all(ctx);
        prepared
    }

    pub(super) fn prepare_frame_with_offscreens(
        &mut self,
        ctx: &mut GpuContext,
        format: wgpu::TextureFormat,
        _target_size: [u32; 2],
        projection: &[f32; 16],
        model: &Live2DModel,
        textures: &[Texture],
        clipping: &mut Option<ClippingManager>,
    ) -> PreparedLive2DFrame {
        self.ensure_offscreen_clipping(model);

        let mut offscreen_clipping = self.cached_offscreen_clipping.take();
        if let Some(offscreen_clipping) = offscreen_clipping.as_mut() {
            offscreen_clipping.update_matrices(model);
        }

        let drawable_clipping = clipping.as_ref();
        let offscreen_clipping_ref = offscreen_clipping.as_ref();

        let mut passes = Vec::new();
        let mut current = SegmentBuilder::new(PreparedPassTarget::Root, false);
        let mut open_offscreens: Vec<usize> = Vec::new();
        let mut started_offscreens = vec![false; model.offscreen_count()];

        for &object in model.sorted_render_objects() {
            self.close_completed_offscreens(
                ctx,
                model,
                textures,
                drawable_clipping,
                offscreen_clipping_ref,
                &mut open_offscreens,
                &mut current,
                &mut passes,
                object,
            );

            match object {
                Live2DRenderObject::Drawable(drawable_index) => {
                    let (draw, mask_request) = self.prepare_model_draw_for_index(
                        ctx,
                        model,
                        textures,
                        drawable_index,
                        drawable_clipping,
                        projection,
                        false,
                    );
                    if let Some(draw) = draw {
                        current.push_mask_request(mask_request);
                        current.items.push(PreparedTargetItem::Drawable(draw));
                    }
                }
                Live2DRenderObject::Offscreen(offscreen_index) => {
                    if !current.is_empty() {
                        passes.push(self.finalize_segment(
                            ctx,
                            model,
                            textures,
                            drawable_clipping,
                            offscreen_clipping_ref,
                            current,
                        ));
                    }
                    let clear = !started_offscreens[offscreen_index];
                    started_offscreens[offscreen_index] = true;
                    current =
                        SegmentBuilder::new(PreparedPassTarget::Offscreen(offscreen_index), clear);
                    open_offscreens.push(offscreen_index);
                }
            }
        }

        while let Some(closed_offscreen) = open_offscreens.pop() {
            if !current.is_empty() {
                passes.push(self.finalize_segment(
                    ctx,
                    model,
                    textures,
                    drawable_clipping,
                    offscreen_clipping_ref,
                    current,
                ));
            }
            current = SegmentBuilder::new(
                open_offscreens
                    .last()
                    .copied()
                    .map(PreparedPassTarget::Offscreen)
                    .unwrap_or(PreparedPassTarget::Root),
                false,
            );
            let (draw, mask_request) =
                self.prepare_composite_draw(ctx, model, closed_offscreen, offscreen_clipping_ref);
            current.push_mask_request(mask_request);
            current.items.push(PreparedTargetItem::Composite(draw));
        }

        if !current.is_empty() {
            passes.push(self.finalize_segment(
                ctx,
                model,
                textures,
                drawable_clipping,
                offscreen_clipping_ref,
                current,
            ));
        }

        self.cached_offscreen_clipping = offscreen_clipping;

        PreparedLive2DFrame::new(format, passes, model.model_opacity())
    }

    pub(super) fn close_completed_offscreens(
        &mut self,
        ctx: &mut GpuContext,
        model: &Live2DModel,
        textures: &[Texture],
        drawable_clipping: Option<&ClippingManager>,
        offscreen_clipping: Option<&ClippingManager>,
        open_offscreens: &mut Vec<usize>,
        current: &mut SegmentBuilder,
        passes: &mut Vec<PreparedTargetPass>,
        next_object: Live2DRenderObject,
    ) {
        while let Some(&current_offscreen) = open_offscreens.last() {
            if self.object_is_inside_offscreen(model, next_object, current_offscreen) {
                break;
            }

            let closed_offscreen = open_offscreens.pop().unwrap();
            if !current.is_empty() {
                passes.push(self.finalize_segment(
                    ctx,
                    model,
                    textures,
                    drawable_clipping,
                    offscreen_clipping,
                    std::mem::replace(
                        current,
                        SegmentBuilder::new(PreparedPassTarget::Root, false),
                    ),
                ));
            }

            let parent_target = open_offscreens
                .last()
                .copied()
                .map(PreparedPassTarget::Offscreen)
                .unwrap_or(PreparedPassTarget::Root);
            *current = SegmentBuilder::new(parent_target, false);
            let (draw, mask_request) =
                self.prepare_composite_draw(ctx, model, closed_offscreen, offscreen_clipping);
            current.push_mask_request(mask_request);
            current.items.push(PreparedTargetItem::Composite(draw));
            let _ = current_offscreen;
        }
    }

    pub(super) fn object_is_inside_offscreen(
        &self,
        model: &Live2DModel,
        object: Live2DRenderObject,
        current_offscreen: usize,
    ) -> bool {
        let current_owner = model.offscreen_owner_index(current_offscreen);
        if current_owner < 0 {
            return false;
        }

        let mut target_parent = match object {
            Live2DRenderObject::Drawable(drawable_index) => {
                model.drawable_parent_part_index(drawable_index)
            }
            Live2DRenderObject::Offscreen(offscreen_index) => {
                let owner = model.offscreen_owner_index(offscreen_index);
                if owner < 0 {
                    -1
                } else {
                    model.part_parent_part_index(owner as usize)
                }
            }
        };

        while target_parent >= 0 {
            if target_parent == current_owner {
                return true;
            }
            target_parent = model.part_parent_part_index(target_parent as usize);
        }

        false
    }

    pub(super) fn finalize_segment(
        &mut self,
        ctx: &mut GpuContext,
        model: &Live2DModel,
        textures: &[Texture],
        drawable_clipping: Option<&ClippingManager>,
        offscreen_clipping: Option<&ClippingManager>,
        segment: SegmentBuilder,
    ) -> PreparedTargetPass {
        let mask_draws = self.prepare_mask_draws_for_requests(
            ctx,
            model,
            textures,
            drawable_clipping,
            offscreen_clipping,
            &segment.mask_requests,
        );

        PreparedTargetPass {
            target: segment.target,
            clear: segment.clear,
            requires_mask: !segment.mask_requests.is_empty()
                || segment.items.iter().any(PreparedTargetItem::uses_mask),
            mask_draws,
            items: segment.items,
        }
    }

    pub(super) fn prepare_mask_draws(
        &mut self,
        ctx: &mut GpuContext,
        model: &Live2DModel,
        textures: &[Texture],
        clipping: &ClippingManager,
    ) -> Vec<PreparedMaskDraw> {
        let requests: Vec<MaskRequest> = clipping
            .contexts
            .iter()
            .enumerate()
            .map(|(context_index, _)| MaskRequest {
                kind: clipping.kind(),
                context_index,
            })
            .collect();
        self.prepare_mask_draws_for_requests(
            ctx,
            model,
            textures,
            Some(clipping),
            Some(clipping),
            &requests,
        )
    }

    pub(super) fn prepare_mask_draws_for_requests(
        &mut self,
        ctx: &mut GpuContext,
        model: &Live2DModel,
        textures: &[Texture],
        drawable_clipping: Option<&ClippingManager>,
        offscreen_clipping: Option<&ClippingManager>,
        requests: &[MaskRequest],
    ) -> Vec<PreparedMaskDraw> {
        let mut draws = Vec::new();
        let dummy_mask_view = self.dummy_texture.view().clone();
        let dummy_mask_texture_id = std::ptr::from_ref(self.dummy_texture.texture()) as usize;

        for &request in requests {
            let (clipping, ctx_entry) = match request.kind {
                ClippingObjectKind::Drawable => {
                    let clipping = drawable_clipping.expect("drawable clipping must exist");
                    (clipping, &clipping.contexts[request.context_index])
                }
                ClippingObjectKind::Offscreen => {
                    let clipping = offscreen_clipping.expect("offscreen clipping must exist");
                    (clipping, &clipping.contexts[request.context_index])
                }
            };

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

                let lb = &ctx_entry.layout_bounds;
                let (pipeline_kind, pipeline) = if model.drawable_is_double_sided(mask_idx) {
                    (
                        PreparedMaskPipelineKind::Unculled,
                        self.mask_pipeline.as_ref().unwrap().clone(),
                    )
                } else {
                    (
                        PreparedMaskPipelineKind::Culled,
                        self.mask_pipeline_culled.as_ref().unwrap().clone(),
                    )
                };
                let uniforms = Live2DUniforms {
                    projection_matrix: ctx_entry.mask_matrix,
                    clip_matrix: ctx_entry.mask_matrix,
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
                    inverted_mask: 0.0,
                    color_blend_type: 0,
                    alpha_blend_type: 0,
                };
                let uniform_offset = self.uniforms.push_staged(ctx, uniforms);
                let (vertex_upload, index_upload, index_count) =
                    self.upload_drawable(ctx, positions, uvs, indices);

                let tex_idx = model.drawable_texture_index(mask_idx) as usize;
                let (color_view, color_texture_id) = if let Some(texture) = textures.get(tex_idx) {
                    (
                        texture.view().clone(),
                        std::ptr::from_ref(texture.texture()) as usize,
                    )
                } else {
                    (
                        self.dummy_texture.view().clone(),
                        std::ptr::from_ref(self.dummy_texture.texture()) as usize,
                    )
                };
                let texture_bind_group_key = [color_texture_id, dummy_mask_texture_id];

                draws.push(PreparedMaskDraw {
                    pipeline_kind,
                    pipeline,
                    vertex_upload,
                    index_upload,
                    index_count,
                    uniform_offset,
                    texture_bind_group_key,
                    texture_bg: self.create_texture_bind_group(
                        ctx,
                        TextureBindGroupKey {
                            color_texture: texture_bind_group_key[0],
                            mask_texture: texture_bind_group_key[1],
                            destination_texture: 0,
                        },
                        &color_view,
                        &dummy_mask_view,
                        None,
                        "live2d_mask_tex_bg",
                    ),
                });
            }
        }

        draws
    }

    pub(super) fn prepare_model_draw_for_index(
        &mut self,
        ctx: &mut GpuContext,
        model: &Live2DModel,
        textures: &[Texture],
        draw_idx: usize,
        clipping: Option<&ClippingManager>,
        projection: &[f32; 16],
        apply_model_opacity: bool,
    ) -> (Option<PreparedModelDraw>, Option<MaskRequest>) {
        if !model.drawable_is_visible(draw_idx) {
            return (None, None);
        }

        let positions = model.drawable_vertex_positions(draw_idx);
        let uvs = model.drawable_vertex_uvs(draw_idx);
        let indices = model.drawable_indices(draw_idx);
        if positions.is_empty() || indices.is_empty() {
            return (None, None);
        }

        let (use_mask, clip_matrix, channel_flag, mask_request) =
            self.get_drawable_clip_info(draw_idx, clipping);
        let raw_blend = model.drawable_blend_mode_raw(draw_idx);
        let uniforms = Live2DUniforms {
            projection_matrix: *projection,
            clip_matrix,
            base_color: {
                let opacity = model.drawable_opacity(draw_idx)
                    * if apply_model_opacity {
                        model.model_opacity()
                    } else {
                        1.0
                    };
                [opacity, opacity, opacity, opacity]
            },
            multiply_color: model.drawable_multiply_color(draw_idx),
            screen_color: model.drawable_screen_color(draw_idx),
            channel_flag,
            use_mask,
            inverted_mask: if model.drawable_is_inverted_mask(draw_idx) {
                1.0
            } else {
                0.0
            },
            color_blend_type: raw_blend_to_color_blend_type(raw_blend),
            alpha_blend_type: raw_blend_to_alpha_blend_type(raw_blend),
        };
        let uniform_offset = self.uniforms.push_staged(ctx, uniforms);
        let (vertex_upload, index_upload, index_count) =
            self.upload_drawable(ctx, positions, uvs, indices);

        let mask_texture = self.mask_texture.as_ref().unwrap();
        let mask_view = mask_texture.view().clone();
        let (destination_texture_id, destination_view) =
            if compatible_blend_mode(raw_blend).is_none() {
                let backup_target = self
                    .blend_backup_target
                    .as_ref()
                    .expect("advanced blend drawables require blend backup target");
                (
                    std::ptr::from_ref(backup_target.texture()) as usize,
                    Some(backup_target.view().clone()),
                )
            } else {
                (0usize, None)
            };
        let tex_idx = model.drawable_texture_index(draw_idx) as usize;
        let (color_view, color_texture_id) = if let Some(texture) = textures.get(tex_idx) {
            (
                texture.view().clone(),
                std::ptr::from_ref(texture.texture()) as usize,
            )
        } else {
            (
                self.dummy_texture.view().clone(),
                std::ptr::from_ref(self.dummy_texture.texture()) as usize,
            )
        };
        let texture_bind_group_key = [
            color_texture_id,
            std::ptr::from_ref(mask_texture.texture()) as usize,
            destination_texture_id,
        ];
        let is_double_sided = model.drawable_is_double_sided(draw_idx);
        let (pipeline_kind, pipeline, requires_backdrop) = match compatible_blend_mode(raw_blend) {
            Some(BlendMode::Normal) => (
                if is_double_sided {
                    PreparedModelPipelineKind::Normal
                } else {
                    PreparedModelPipelineKind::NormalCulled
                },
                if is_double_sided {
                    self.model_pipeline_normal.as_ref().unwrap().clone()
                } else {
                    self.model_pipeline_normal_culled.as_ref().unwrap().clone()
                },
                false,
            ),
            Some(BlendMode::Additive) => (
                if is_double_sided {
                    PreparedModelPipelineKind::Additive
                } else {
                    PreparedModelPipelineKind::AdditiveCulled
                },
                if is_double_sided {
                    self.model_pipeline_additive.as_ref().unwrap().clone()
                } else {
                    self.model_pipeline_additive_culled
                        .as_ref()
                        .unwrap()
                        .clone()
                },
                false,
            ),
            Some(BlendMode::Multiplicative) => (
                if is_double_sided {
                    PreparedModelPipelineKind::Multiplicative
                } else {
                    PreparedModelPipelineKind::MultiplicativeCulled
                },
                if is_double_sided {
                    self.model_pipeline_multiplicative.as_ref().unwrap().clone()
                } else {
                    self.model_pipeline_multiplicative_culled
                        .as_ref()
                        .unwrap()
                        .clone()
                },
                false,
            ),
            None => (
                if is_double_sided {
                    PreparedModelPipelineKind::Overlap
                } else {
                    PreparedModelPipelineKind::OverlapCulled
                },
                if is_double_sided {
                    self.model_pipeline_overlap.as_ref().unwrap().clone()
                } else {
                    self.model_pipeline_overlap_culled.as_ref().unwrap().clone()
                },
                true,
            ),
        };

        (
            Some(PreparedModelDraw {
                pipeline_kind,
                pipeline,
                vertex_upload,
                index_upload,
                index_count,
                uniform_offset,
                uses_mask: use_mask > 0.5,
                requires_backdrop,
                texture_bind_group_key,
                texture_bg: self.create_texture_bind_group(
                    ctx,
                    TextureBindGroupKey {
                        color_texture: texture_bind_group_key[0],
                        mask_texture: texture_bind_group_key[1],
                        destination_texture: texture_bind_group_key[2],
                    },
                    &color_view,
                    &mask_view,
                    destination_view.as_ref(),
                    "live2d_model_tex_bg",
                ),
            }),
            mask_request,
        )
    }
}

fn model_uses_advanced_blend(model: &Live2DModel) -> bool {
    (0..model.drawable_count()).any(|drawable_index| {
        compatible_blend_mode(model.drawable_blend_mode_raw(drawable_index)).is_none()
    })
}
