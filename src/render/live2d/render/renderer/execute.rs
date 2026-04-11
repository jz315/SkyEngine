use super::composite::clear_render_target;
use super::*;

impl Live2DRenderer {
    pub(super) fn render_target_pass(
        &mut self,
        ctx: &mut GpuContext,
        pass: &PreparedTargetPass,
        root_texture: &wgpu::Texture,
        root_view: &wgpu::TextureView,
        root_width: u32,
        root_height: u32,
    ) {
        if pass.items.is_empty() {
            return;
        }

        if pass.requires_mask {
            if pass.mask_draws.is_empty() {
                self.clear_mask_texture(ctx);
            } else {
                self.render_mask_pass(ctx, &pass.mask_draws);
            }
        }

        let (target_texture, target_view, width, height) = match pass.target {
            PreparedPassTarget::Root => (
                root_texture.clone(),
                root_view.clone(),
                root_width,
                root_height,
            ),
            PreparedPassTarget::Offscreen(index) => {
                let target = &self.offscreen_targets[index];
                (
                    target.texture().clone(),
                    target.view().clone(),
                    target.width(),
                    target.height(),
                )
            }
        };

        let mut needs_clear = pass.clear;
        let mut drawable_cursor = 0usize;
        while drawable_cursor < pass.items.len() {
            if let PreparedTargetItem::Drawable(draw) = &pass.items[drawable_cursor] {
                if draw.requires_backdrop {
                    if needs_clear {
                        self.clear_blend_backup(ctx);
                    } else {
                        self.copy_texture_to_blend_backup(ctx, &target_texture, width, height);
                    }

                    let mut frame = ctx.frame();
                    let color_attachments = [Some(wgpu::RenderPassColorAttachment {
                        view: &target_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: if needs_clear {
                                wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)
                            } else {
                                wgpu::LoadOp::Load
                            },
                            store: wgpu::StoreOp::Store,
                        },
                    })];
                    let mut render_pass = frame.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("live2d_target_drawable_overlap"),
                        color_attachments: &color_attachments,
                        depth_stencil_attachment: None,
                        ..Default::default()
                    });
                    render_pass.set_pipeline(&draw.pipeline);
                    render_pass.set_bind_group(
                        0,
                        self.uniforms.bind_group(),
                        &[draw.uniform_offset],
                    );
                    render_pass.set_bind_group(1, &draw.texture_bg, &[]);
                    render_pass.set_vertex_buffer(0, draw.vertex_upload.slice());
                    render_pass
                        .set_index_buffer(draw.index_upload.slice(), wgpu::IndexFormat::Uint16);
                    render_pass.draw_indexed(0..draw.index_count, 0, 0..1);

                    needs_clear = false;
                    drawable_cursor += 1;
                    continue;
                }
            }

            let mut drawable_end = drawable_cursor;
            while drawable_end < pass.items.len()
                && matches!(
                    &pass.items[drawable_end],
                    PreparedTargetItem::Drawable(draw) if !draw.requires_backdrop
                )
            {
                drawable_end += 1;
            }

            if drawable_end > drawable_cursor {
                let mut frame = ctx.frame();
                let color_attachments = [Some(wgpu::RenderPassColorAttachment {
                    view: &target_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: if needs_clear {
                            wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)
                        } else {
                            wgpu::LoadOp::Load
                        },
                        store: wgpu::StoreOp::Store,
                    },
                })];
                let mut render_pass = frame.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("live2d_target_drawables"),
                    color_attachments: &color_attachments,
                    depth_stencil_attachment: None,
                    ..Default::default()
                });
                let mut last_pipeline_kind = None;
                let mut last_texture_key = None;
                for item in &pass.items[drawable_cursor..drawable_end] {
                    let PreparedTargetItem::Drawable(draw) = item else {
                        unreachable!();
                    };
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
                needs_clear = false;
                drawable_cursor = drawable_end;
                continue;
            }

            let PreparedTargetItem::Composite(draw) = &pass.items[drawable_cursor] else {
                unreachable!();
            };
            if needs_clear {
                self.clear_blend_backup(ctx);
            } else {
                self.copy_texture_to_blend_backup(ctx, &target_texture, width, height);
            }
            let (backup_texture_id, backup_view) = {
                let backup_target = self.blend_backup_target.as_ref().unwrap();
                (
                    std::ptr::from_ref(backup_target.texture()) as usize,
                    backup_target.view().clone(),
                )
            };
            let (mask_texture_id, mask_view) = {
                let mask_target = self.mask_texture.as_ref().unwrap();
                (
                    std::ptr::from_ref(mask_target.texture()) as usize,
                    mask_target.view().clone(),
                )
            };
            let bind_group = self.create_composite_bind_group(
                ctx,
                CompositeBindGroupKey {
                    source_texture: draw.source_texture_id,
                    mask_texture: mask_texture_id,
                    destination_texture: backup_texture_id,
                },
                &draw.source_view,
                &mask_view,
                &backup_view,
            );

            let mut frame = ctx.frame();
            let color_attachments = [Some(wgpu::RenderPassColorAttachment {
                view: &target_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: if needs_clear {
                        wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)
                    } else {
                        wgpu::LoadOp::Load
                    },
                    store: wgpu::StoreOp::Store,
                },
            })];
            let mut render_pass = frame.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("live2d_composite_pass"),
                color_attachments: &color_attachments,
                depth_stencil_attachment: None,
                ..Default::default()
            });
            render_pass.set_pipeline(&draw.pipeline);
            render_pass.set_bind_group(
                0,
                self.composite_uniforms.bind_group(),
                &[draw.uniform_offset],
            );
            render_pass.set_bind_group(1, &bind_group, &[]);
            FullscreenPass::draw(&mut *render_pass);

            needs_clear = false;
            drawable_cursor += 1;
        }
    }

    pub(super) fn copy_texture_to_blend_backup(
        &mut self,
        ctx: &mut GpuContext,
        source_texture: &wgpu::Texture,
        width: u32,
        height: u32,
    ) {
        let backup_target = self.blend_backup_target.as_ref().unwrap();
        ctx.encoder().copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: source_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyTextureInfo {
                texture: backup_target.texture(),
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
        );
    }

    pub(super) fn clear_blend_backup(&mut self, ctx: &mut GpuContext) {
        let backup_target = self.blend_backup_target.as_ref().unwrap();
        clear_render_target(ctx, backup_target, "live2d_blend_backup_clear");
    }

    pub(super) fn clear_mask_texture(&mut self, ctx: &mut GpuContext) {
        let mask_target = self.mask_texture.as_ref().unwrap();
        let mut frame = ctx.frame();
        let _pass = frame.begin_target_pass(
            "live2d_mask_clear",
            mask_target,
            wgpu::LoadOp::Clear(wgpu::Color::WHITE),
        );
    }

    pub(super) fn blit_root_target_to_surface(&mut self, ctx: &mut GpuContext, color: [f32; 4]) {
        let root_target = self.root_intermediate_target.as_ref().unwrap();
        let texture_id = std::ptr::from_ref(root_target.texture()) as usize;
        let bind_group = self
            .cached_blit_bind_groups
            .entry(texture_id)
            .or_insert_with(|| {
                ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("live2d_root_blit_bg"),
                    layout: &self.blit_texture_bgl,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(root_target.view()),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(ctx.sampler_linear()),
                        },
                    ],
                })
            })
            .clone();

        self.blit_uniforms.clear();
        let uniform_offset = self
            .blit_uniforms
            .push(ctx, Live2DBlitUniforms { base_color: color });
        let surface_format = ctx.surface_format();
        let pipeline = self.blit_pipeline.pipeline(ctx, surface_format);
        let mut frame = ctx.frame();
        let mut render_pass = frame.begin_surface_pass_loaded("live2d_root_blit");
        render_pass.set_pipeline(&pipeline);
        render_pass.set_bind_group(0, self.blit_uniforms.bind_group(), &[uniform_offset]);
        render_pass.set_bind_group(1, &bind_group, &[]);
        FullscreenPass::draw(&mut *render_pass);
    }

    pub(super) fn blit_root_target_to_target(
        &mut self,
        ctx: &mut GpuContext,
        target: &RenderTarget,
        color: [f32; 4],
    ) {
        let root_target = self.root_intermediate_target.as_ref().unwrap();
        let texture_id = std::ptr::from_ref(root_target.texture()) as usize;
        let bind_group = self
            .cached_blit_bind_groups
            .entry(texture_id)
            .or_insert_with(|| {
                ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("live2d_root_blit_bg"),
                    layout: &self.blit_texture_bgl,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(root_target.view()),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(ctx.sampler_linear()),
                        },
                    ],
                })
            })
            .clone();

        self.blit_uniforms.clear();
        let uniform_offset = self
            .blit_uniforms
            .push(ctx, Live2DBlitUniforms { base_color: color });
        let pipeline = self.blit_pipeline.pipeline(ctx, target.format());
        let mut frame = ctx.frame();
        let mut render_pass = frame.begin_target_pass_loaded("live2d_root_blit", target);
        render_pass.set_pipeline(&pipeline);
        render_pass.set_bind_group(0, self.blit_uniforms.bind_group(), &[uniform_offset]);
        render_pass.set_bind_group(1, &bind_group, &[]);
        FullscreenPass::draw(&mut *render_pass);
    }

    // ── Helpers ──────────────────────────────────────────────────────────
}
