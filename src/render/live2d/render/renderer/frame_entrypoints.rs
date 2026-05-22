use super::composite::clear_render_target;
use super::*;

impl Live2DRenderer {
    pub fn prepare_frame_for_view(
        &mut self,
        ctx: &mut GpuContext,
        target_format: wgpu::TextureFormat,
        target_size: [u32; 2],
        model: &Live2DModel,
        textures: &[Texture],
        clipping: &mut Option<ClippingManager>,
    ) -> PreparedLive2DFrame {
        let mvp = ModelToClip::from_cols_array(
            model
                .render_matrix_for_view(target_size[0].max(1) as f32, target_size[1].max(1) as f32),
        );
        self.prepare_frame(
            ctx,
            target_format,
            target_size,
            mvp,
            model,
            textures,
            clipping,
        )
    }

    /// Prepare all Live2D draw data using a caller-provided projection matrix.
    pub fn prepare_frame_with_projection(
        &mut self,
        ctx: &mut GpuContext,
        target_format: wgpu::TextureFormat,
        target_size: [u32; 2],
        projection: &[f32; 16],
        model: &Live2DModel,
        textures: &[Texture],
        clipping: &mut Option<ClippingManager>,
    ) -> PreparedLive2DFrame {
        self.prepare_frame(
            ctx,
            target_format,
            target_size,
            ModelToClip::from_cols_array(*projection),
            model,
            textures,
            clipping,
        )
    }

    /// Prepare all Live2D draw data needed to render into `target`.
    pub fn prepare_frame_for_target(
        &mut self,
        ctx: &mut GpuContext,
        target: &RenderTarget,
        model: &Live2DModel,
        textures: &[Texture],
        clipping: &mut Option<ClippingManager>,
    ) -> PreparedLive2DFrame {
        self.prepare_frame_for_view(
            ctx,
            target.format(),
            [target.width(), target.height()],
            model,
            textures,
            clipping,
        )
    }

    /// Execute a previously prepared frame into `target`.
    pub fn execute_prepared_to_target(
        &mut self,
        ctx: &mut GpuContext,
        target: &RenderTarget,
        prepared: &PreparedLive2DFrame,
    ) {
        debug_assert_eq!(
            prepared.target_format(),
            target.format(),
            "PreparedLive2DFrame target format must match the destination render target",
        );
        self.execute_prepared_model_to_target(ctx, target, prepared);
    }

    /// Execute only the prepared mask pass.
    pub fn execute_prepared_mask_pass(
        &mut self,
        ctx: &mut GpuContext,
        prepared: &PreparedLive2DFrame,
    ) {
        for pass in prepared.passes() {
            self.render_mask_pass(ctx, &pass.mask_draws);
        }
    }

    /// Execute only the prepared model pass into the current surface.
    pub fn execute_prepared_model_to_surface(
        &mut self,
        ctx: &mut GpuContext,
        prepared: &PreparedLive2DFrame,
    ) {
        debug_assert_eq!(
            prepared.target_format(),
            ctx.surface_format(),
            "PreparedLive2DFrame target format must match the current surface format",
        );
        self.render_prepared_passes_to_surface(ctx, prepared);
    }

    /// Execute only the prepared model pass into `target`.
    pub fn execute_prepared_model_to_target(
        &mut self,
        ctx: &mut GpuContext,
        target: &RenderTarget,
        prepared: &PreparedLive2DFrame,
    ) {
        debug_assert_eq!(
            prepared.target_format(),
            target.format(),
            "PreparedLive2DFrame target format must match the destination render target",
        );
        self.render_prepared_passes_to_target(ctx, prepared, target);
    }

    // ── Mask pass ───────────────────────────────────────────────────────

    pub(super) fn render_mask_pass(&mut self, ctx: &mut GpuContext, draws: &[PreparedMaskDraw]) {
        if draws.is_empty() {
            return;
        }

        let mask_target = self.mask_texture.as_ref().unwrap();
        let mut frame = ctx.frame();
        let mut pass = frame.begin_target_pass(
            "live2d_mask_pass",
            mask_target,
            wgpu::LoadOp::Clear(wgpu::Color::WHITE),
        );

        let mut last_pipeline_kind = None;
        let mut last_texture_key = None;
        for draw in draws {
            if last_pipeline_kind != Some(draw.pipeline_kind) {
                pass.set_pipeline(&draw.pipeline);
                last_pipeline_kind = Some(draw.pipeline_kind);
            }
            pass.set_bind_group(0, self.uniforms.bind_group(), &[draw.uniform_offset]);
            if last_texture_key != Some(draw.texture_bind_group_key) {
                pass.set_bind_group(1, &draw.texture_bg, &[]);
                last_texture_key = Some(draw.texture_bind_group_key);
            }
            pass.set_vertex_buffer(0, draw.vertex_upload.slice());
            pass.set_index_buffer(draw.index_upload.slice(), wgpu::IndexFormat::Uint16);
            pass.draw_indexed(0..draw.index_count, 0, 0..1);
        }
    }

    pub(super) fn render_prepared_passes_to_surface(
        &mut self,
        ctx: &mut GpuContext,
        prepared: &PreparedLive2DFrame,
    ) {
        if prepared.is_empty() {
            return;
        }

        if prepared.has_offscreen_passes() || prepared.has_backdrop_draws() {
            let (root_texture, root_view, root_width, root_height) = {
                let root_target = self
                    .root_intermediate_target
                    .as_ref()
                    .expect("complex Live2D execution requires a prepared root target");
                clear_render_target(ctx, root_target, "live2d_root_clear");
                (
                    root_target.texture().clone(),
                    root_target.view().clone(),
                    root_target.width(),
                    root_target.height(),
                )
            };
            for pass in prepared.passes() {
                self.render_target_pass(
                    ctx,
                    pass,
                    &root_texture,
                    &root_view,
                    root_width,
                    root_height,
                );
            }
            self.blit_root_target_to_surface(ctx, prepared.final_root_color());
        } else {
            for pass in prepared.passes() {
                self.render_root_pass_to_surface(ctx, pass);
            }
        }
    }

    pub(super) fn render_prepared_passes_to_target(
        &mut self,
        ctx: &mut GpuContext,
        prepared: &PreparedLive2DFrame,
        target: &RenderTarget,
    ) {
        if prepared.is_empty() {
            return;
        }

        if prepared.has_offscreen_passes() || prepared.has_backdrop_draws() {
            let (root_texture, root_view, root_width, root_height) = {
                let root_target = self
                    .root_intermediate_target
                    .as_ref()
                    .expect("complex Live2D execution requires a prepared root target");
                clear_render_target(ctx, root_target, "live2d_root_clear");
                (
                    root_target.texture().clone(),
                    root_target.view().clone(),
                    root_target.width(),
                    root_target.height(),
                )
            };
            for pass in prepared.passes() {
                self.render_target_pass(
                    ctx,
                    pass,
                    &root_texture,
                    &root_view,
                    root_width,
                    root_height,
                );
            }
            self.blit_root_target_to_target(ctx, target, prepared.final_root_color());
        } else {
            let root_texture = target.texture().clone();
            let root_view = target.view().clone();
            let root_width = target.width();
            let root_height = target.height();
            for pass in prepared.passes() {
                self.render_target_pass(
                    ctx,
                    pass,
                    &root_texture,
                    &root_view,
                    root_width,
                    root_height,
                );
            }
        }
    }

    pub(super) fn render_root_pass_to_surface(
        &mut self,
        ctx: &mut GpuContext,
        pass: &PreparedTargetPass,
    ) {
        if pass.items.is_empty() {
            return;
        }
        debug_assert!(
            !pass.has_backdrop_draws(),
            "surface direct path cannot execute backdrop-dependent Live2D draws"
        );

        if pass.requires_mask {
            if pass.mask_draws.is_empty() {
                self.clear_mask_texture(ctx);
            } else {
                self.render_mask_pass(ctx, &pass.mask_draws);
            }
        }

        let mut frame = ctx.frame();
        let mut render_pass = frame.begin_surface_pass(
            "live2d_model_pass",
            if pass.clear {
                Some(wgpu::Color::TRANSPARENT)
            } else {
                None
            },
        );
        let mut last_pipeline_kind = None;
        let mut last_texture_key = None;
        for item in &pass.items {
            match item {
                PreparedTargetItem::Drawable(draw) => {
                    if last_pipeline_kind != Some(draw.pipeline_kind) {
                        render_pass.set_pipeline(&draw.pipeline);
                        last_pipeline_kind = Some(draw.pipeline_kind);
                    }
                    render_pass.set_bind_group(
                        0,
                        self.uniforms.bind_group(),
                        &[draw.uniform_offset],
                    );
                    if last_texture_key != Some(draw.texture_bind_group_key) {
                        render_pass.set_bind_group(1, &draw.texture_bg, &[]);
                        last_texture_key = Some(draw.texture_bind_group_key);
                    }
                    render_pass.set_vertex_buffer(0, draw.vertex_upload.slice());
                    render_pass
                        .set_index_buffer(draw.index_upload.slice(), wgpu::IndexFormat::Uint16);
                    render_pass.draw_indexed(0..draw.index_count, 0, 0..1);
                }
                PreparedTargetItem::Composite(_) => {
                    unreachable!("surface root path should not contain composite draws")
                }
            }
        }
    }
}
