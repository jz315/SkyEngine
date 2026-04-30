//! Execution engine: compile, allocate, run passes, release.

use super::*;
use std::borrow::Cow;

struct ValidatedBufferToTextureCopy {
    width: u32,
    height: u32,
    row_bytes: u32,
    rows_per_image: u32,
}

impl RenderGraph {
    fn validate_texture_to_texture_copy(
        &self,
        src: TextureHandle,
        dst: TextureHandle,
        surface_size: [u32; 2],
    ) -> Result<wgpu::Extent3d, RenderGraphError> {
        let src_desc = &self.textures[src.0];
        let dst_desc = &self.textures[dst.0];
        if src_desc.format != dst_desc.format {
            return Err(RenderGraphError::InvalidTextureCopy {
                src,
                dst,
                details: Cow::Owned(format!(
                    "source format {:?} does not match destination format {:?}",
                    src_desc.format, dst_desc.format
                )),
            });
        }

        let src_extent = self.resolve_texture_extent(src, surface_size);
        let dst_extent = self.resolve_texture_extent(dst, surface_size);
        if src_extent != dst_extent {
            return Err(RenderGraphError::InvalidTextureCopy {
                src,
                dst,
                details: Cow::Owned(format!(
                    "source extent {}x{} does not match destination extent {}x{}",
                    src_extent[0], src_extent[1], dst_extent[0], dst_extent[1]
                )),
            });
        }

        Ok(wgpu::Extent3d {
            width: src_extent[0],
            height: src_extent[1],
            depth_or_array_layers: 1,
        })
    }

    fn validate_buffer_to_buffer_copy(
        &self,
        src: BufferHandle,
        dst: BufferHandle,
    ) -> Result<u64, RenderGraphError> {
        let src_size = self.buffers[src.0].size_bytes;
        let dst_size = self.buffers[dst.0].size_bytes;
        if src_size != dst_size {
            return Err(RenderGraphError::InvalidBufferCopy {
                src,
                dst,
                details: Cow::Owned(format!(
                    "source size {src_size} does not match destination size {dst_size}"
                )),
            });
        }
        Ok(src_size)
    }

    fn validate_buffer_to_texture_copy(
        &self,
        src: BufferHandle,
        dst: TextureHandle,
        bytes_per_row: Option<u32>,
        rows_per_image: Option<u32>,
        surface_size: [u32; 2],
    ) -> Result<ValidatedBufferToTextureCopy, RenderGraphError> {
        let [width, height] = self.resolve_texture_extent(dst, surface_size);
        let dst_format = self.textures[dst.0].format;
        let bytes_per_pixel = texture_format_bytes_per_pixel(dst_format).ok_or(
            RenderGraphError::UnsupportedBufferTextureCopyFormat {
                texture: dst,
                format: dst_format,
            },
        )?;
        let min_row_bytes = width * bytes_per_pixel;
        let row_bytes = bytes_per_row.unwrap_or(min_row_bytes);
        if row_bytes % 256 != 0 {
            return Err(RenderGraphError::InvalidBufferTextureCopyLayout {
                buffer: src,
                texture: dst,
                details: Cow::Owned(format!("bytes_per_row={row_bytes} is not 256-byte aligned")),
            });
        }
        if row_bytes < min_row_bytes {
            return Err(RenderGraphError::InvalidBufferTextureCopyLayout {
                buffer: src,
                texture: dst,
                details: Cow::Owned(format!(
                    "bytes_per_row={row_bytes} is smaller than the required row size {min_row_bytes}"
                )),
            });
        }

        let rows_per_image = rows_per_image.unwrap_or(height);
        if rows_per_image < height {
            return Err(RenderGraphError::InvalidBufferTextureCopyLayout {
                buffer: src,
                texture: dst,
                details: Cow::Owned(format!(
                    "rows_per_image={rows_per_image} is smaller than the copy height {height}"
                )),
            });
        }

        let required_bytes =
            row_bytes as u64 * rows_per_image.saturating_sub(1) as u64 + min_row_bytes as u64;
        let actual_bytes = self.buffers[src.0].size_bytes;
        if actual_bytes < required_bytes {
            return Err(RenderGraphError::SourceBufferTooSmall {
                buffer: src,
                required_bytes,
                actual_bytes,
            });
        }

        Ok(ValidatedBufferToTextureCopy {
            width,
            height,
            row_bytes,
            rows_per_image,
        })
    }

    fn validate_texture_upload(
        &self,
        dst: TextureHandle,
        width: u32,
        height: u32,
        bytes_per_pixel: u32,
        data_len: usize,
        surface_size: [u32; 2],
    ) -> Result<(), RenderGraphError> {
        if width == 0 || height == 0 {
            return Err(RenderGraphError::InvalidTextureUpload {
                texture: dst,
                details: Cow::Borrowed("upload extent must be non-zero"),
            });
        }

        let texture_extent = self.resolve_texture_extent(dst, surface_size);
        if width > texture_extent[0] || height > texture_extent[1] {
            return Err(RenderGraphError::InvalidTextureUpload {
                texture: dst,
                details: Cow::Owned(format!(
                    "upload extent {}x{} exceeds texture extent {}x{}",
                    width, height, texture_extent[0], texture_extent[1]
                )),
            });
        }

        let dst_format = self.textures[dst.0].format;
        let expected_bpp = texture_format_bytes_per_pixel(dst_format).ok_or(
            RenderGraphError::UnsupportedTextureUploadFormat {
                texture: dst,
                format: dst_format,
            },
        )?;
        if bytes_per_pixel != expected_bpp {
            return Err(RenderGraphError::InvalidTextureUpload {
                texture: dst,
                details: Cow::Owned(format!(
                    "bytes_per_pixel={bytes_per_pixel} does not match format {:?} ({expected_bpp} bytes)",
                    dst_format
                )),
            });
        }

        let expected_len = width as u64 * height as u64 * bytes_per_pixel as u64;
        if data_len as u64 != expected_len {
            return Err(RenderGraphError::InvalidTextureUpload {
                texture: dst,
                details: Cow::Owned(format!(
                    "data length {data_len} does not match expected byte count {expected_len}"
                )),
            });
        }

        Ok(())
    }

    // ── Execution ───────────────────────────────────────────────────────

    /// Execute copy operations for a single copy pass.
    ///
    /// NOTE: This method takes `&self` (not `&mut self`) by design.  During
    /// `try_execute`, `PhysicalResources` holds shared borrows of several
    /// `self` fields.  If this method ever needs `&mut self`, the borrow
    /// pattern in `try_execute` must be restructured (e.g. by cloning the
    /// compiled pass list or splitting the struct).
    pub(super) fn execute_copy_pass(
        &self,
        ctx: &mut GpuContext,
        pass: &CompiledPass,
    ) -> Result<(), RenderGraphError> {
        if ctx.has_active_frame() {
            ctx.flush("render_graph_encoder_after_copy");
        }

        let mut encoder: Option<wgpu::CommandEncoder> = None;
        let submit_pending = |ctx: &mut GpuContext, encoder: &mut Option<wgpu::CommandEncoder>| {
            if let Some(encoder) = encoder.take() {
                ctx.queue().submit(std::iter::once(encoder.finish()));
            }
        };
        let surface_size = ctx.surface_size();

        for op in &pass.copy_ops {
            match op {
                CopyOp::TextureToTexture { src, dst } => {
                    let copy_extent =
                        self.validate_texture_to_texture_copy(*src, *dst, surface_size)?;
                    let src_tex = self.try_resolve_texture(*src)?;
                    let dst_tex = self.try_resolve_texture(*dst)?;
                    let encoder = encoder.get_or_insert_with(|| {
                        ctx.device()
                            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                                label: Some("render_graph_copy_pass"),
                            })
                    });
                    encoder.copy_texture_to_texture(
                        src_tex.as_image_copy(),
                        dst_tex.as_image_copy(),
                        copy_extent,
                    );
                }
                CopyOp::BufferToBuffer { src, dst } => {
                    let size = self.validate_buffer_to_buffer_copy(*src, *dst)?;
                    let src_buf = self.try_resolve_buffer(*src)?;
                    let dst_buf = self.try_resolve_buffer(*dst)?;
                    let encoder = encoder.get_or_insert_with(|| {
                        ctx.device()
                            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                                label: Some("render_graph_copy_pass"),
                            })
                    });
                    encoder.copy_buffer_to_buffer(src_buf, 0, dst_buf, 0, size);
                }
                CopyOp::BufferToTexture {
                    src,
                    dst,
                    bytes_per_row,
                    rows_per_image,
                } => {
                    let validated = self.validate_buffer_to_texture_copy(
                        *src,
                        *dst,
                        *bytes_per_row,
                        *rows_per_image,
                        surface_size,
                    )?;

                    let src_buf = self.try_resolve_buffer(*src)?;
                    let dst_tex = self.try_resolve_texture(*dst)?;
                    let encoder = encoder.get_or_insert_with(|| {
                        ctx.device()
                            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                                label: Some("render_graph_copy_pass"),
                            })
                    });
                    encoder.copy_buffer_to_texture(
                        wgpu::TexelCopyBufferInfo {
                            buffer: src_buf,
                            layout: wgpu::TexelCopyBufferLayout {
                                offset: 0,
                                bytes_per_row: Some(validated.row_bytes),
                                rows_per_image: Some(validated.rows_per_image),
                            },
                        },
                        dst_tex.as_image_copy(),
                        wgpu::Extent3d {
                            width: validated.width,
                            height: validated.height,
                            depth_or_array_layers: 1,
                        },
                    );
                }
                CopyOp::UploadToTexture {
                    data,
                    dst,
                    width,
                    height,
                    bytes_per_pixel,
                } => {
                    self.validate_texture_upload(
                        *dst,
                        *width,
                        *height,
                        *bytes_per_pixel,
                        data.len(),
                        surface_size,
                    )?;
                    submit_pending(ctx, &mut encoder);
                    let dst_tex = self.try_resolve_texture(*dst)?;
                    // NOTE: Unlike encoder.copy_buffer_to_texture (which requires
                    // bytes_per_row aligned to 256), queue.write_texture() handles
                    // staging internally and accepts any valid bytes_per_row.
                    ctx.queue().write_texture(
                        dst_tex.as_image_copy(),
                        data,
                        wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(width * bytes_per_pixel),
                            rows_per_image: Some(*height),
                        },
                        wgpu::Extent3d {
                            width: *width,
                            height: *height,
                            depth_or_array_layers: 1,
                        },
                    );
                }
            }
        }

        submit_pending(ctx, &mut encoder);

        Ok(())
    }

    /// Compile, allocate resources, and execute all alive passes in order.
    pub fn try_execute<F>(
        &mut self,
        ctx: &mut GpuContext,
        mut run_pass: F,
    ) -> Result<(), RenderGraphError>
    where
        F: FnMut(
            &CompiledPass,
            &mut GpuContext,
            &PhysicalResources<'_>,
        ) -> Result<(), RenderGraphError>,
    {
        if !self.compiled {
            self.compile()?;
        }

        self.allocate_physical_resources(ctx);

        let compiled = std::mem::take(&mut self.cached_compiled);
        let result = {
            let resources = PhysicalResources {
                handle_token: self.handle_token,
                textures: &self.physical_textures,
                buffers: &self.physical_buffers,
                texture_descs: &self.textures,
                buffer_descs: &self.buffers,
                alias_redirects: &self.alias_redirects,
                blackboard: &self.blackboard,
            };

            let mut err = None;
            for pass in &compiled {
                if pass.pass_type == PassType::Copy {
                    if let Err(e) = self.execute_copy_pass(ctx, pass) {
                        err = Some(e);
                        break;
                    }
                } else {
                    if let Err(e) = run_pass(pass, ctx, &resources) {
                        err = Some(e);
                        break;
                    }
                }
            }
            err
        };
        self.cached_compiled = compiled;

        self.release_transient_resources(ctx);

        match result {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    /// Convenience wrapper around [`try_execute`] that panics on error.
    pub fn execute<F>(&mut self, ctx: &mut GpuContext, run_pass: F)
    where
        F: FnMut(
            &CompiledPass,
            &mut GpuContext,
            &PhysicalResources<'_>,
        ) -> Result<(), RenderGraphError>,
    {
        self.try_execute(ctx, run_pass)
            .expect("RenderGraph::execute failed");
    }

    /// Like [`try_execute`], but with a [`RenderGraphProfiler`] for timing.
    pub fn try_execute_profiled<P, F>(
        &mut self,
        ctx: &mut GpuContext,
        profiler: &mut P,
        mut run_pass: F,
    ) -> Result<(), RenderGraphError>
    where
        P: RenderGraphProfiler,
        F: FnMut(
            &CompiledPass,
            &mut GpuContext,
            &PhysicalResources<'_>,
        ) -> Result<(), RenderGraphError>,
    {
        if !self.compiled {
            self.compile()?;
        }

        profiler.on_compile(self.passes.len(), self.culled_count, self.max_dep_level);

        self.allocate_physical_resources(ctx);

        let compiled = std::mem::take(&mut self.cached_compiled);
        let result = {
            let resources = PhysicalResources {
                handle_token: self.handle_token,
                textures: &self.physical_textures,
                buffers: &self.physical_buffers,
                texture_descs: &self.textures,
                buffer_descs: &self.buffers,
                alias_redirects: &self.alias_redirects,
                blackboard: &self.blackboard,
            };

            let mut err = None;
            for pass in &compiled {
                profiler.on_pass_begin(&pass.name, pass.pass_type);
                let start = std::time::Instant::now();
                if pass.pass_type == PassType::Copy {
                    if let Err(e) = self.execute_copy_pass(ctx, pass) {
                        profiler.on_pass_end(&pass.name, start.elapsed());
                        err = Some(e);
                        break;
                    }
                } else {
                    if let Err(e) = run_pass(pass, ctx, &resources) {
                        profiler.on_pass_end(&pass.name, start.elapsed());
                        err = Some(e);
                        break;
                    }
                }
                profiler.on_pass_end(&pass.name, start.elapsed());
            }
            err
        };
        self.cached_compiled = compiled;

        self.release_transient_resources(ctx);

        match result {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    /// Like [`execute`], but with a [`RenderGraphProfiler`] for timing.
    pub fn execute_profiled<P, F>(&mut self, ctx: &mut GpuContext, profiler: &mut P, run_pass: F)
    where
        P: RenderGraphProfiler,
        F: FnMut(
            &CompiledPass,
            &mut GpuContext,
            &PhysicalResources<'_>,
        ) -> Result<(), RenderGraphError>,
    {
        self.try_execute_profiled(ctx, profiler, run_pass)
            .expect("RenderGraph::execute_profiled failed");
    }
}
