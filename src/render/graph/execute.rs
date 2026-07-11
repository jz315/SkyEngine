//! Execution engine: compile, allocate, run passes, release.

use super::*;
use std::borrow::Cow;

use rustc_hash::FxHashMap;

struct ValidatedBufferToTextureCopy {
    width: u32,
    height: u32,
    row_bytes: Option<u32>,
    rows_per_image: Option<u32>,
}

enum ValidatedCopyOp<'a> {
    TextureToTexture {
        src: TextureHandle,
        dst: TextureHandle,
        extent: wgpu::Extent3d,
    },
    BufferToBuffer {
        src: BufferHandle,
        dst: BufferHandle,
        size: u64,
    },
    BufferToTexture {
        src: BufferHandle,
        dst: TextureHandle,
        copy: ValidatedBufferToTextureCopy,
    },
    UploadToTexture {
        data: &'a [u8],
        dst: TextureHandle,
        width: u32,
        height: u32,
        bytes_per_pixel: u32,
    },
}

impl RenderGraph {
    fn trace_aliasing_enabled() -> bool {
        std::env::var_os("SKY_RENDER_GRAPH_TRACE_ALIAS").is_some()
    }

    fn texture_label(&self, tex_idx: usize) -> &str {
        self.textures
            .get(tex_idx)
            .map(|desc| desc.name.as_ref())
            .unwrap_or("<invalid>")
    }

    fn resource_label(&self, resource: ResourceRef) -> String {
        match resource {
            ResourceRef::Surface => "surface".to_string(),
            ResourceRef::Texture(handle) => {
                format!("{}#{}", self.texture_label(handle.0), handle.0)
            }
            ResourceRef::TextureSubresource(subresource) => format!(
                "{}#{}[mip {}..{}, layer {}..{}]",
                self.texture_label(subresource.texture.0),
                subresource.texture.0,
                subresource.base_mip_level,
                subresource.base_mip_level + subresource.mip_level_count,
                subresource.base_array_layer,
                subresource.base_array_layer + subresource.array_layer_count
            ),
            ResourceRef::Buffer(handle) => format!("buffer#{}", handle.0),
        }
    }

    fn trace_aliasing_state(&self, compiled: &[CompiledPass]) {
        if !Self::trace_aliasing_enabled() {
            return;
        }

        eprintln!("[RenderGraph][alias] groups:");
        for group in &self.alias_groups {
            if group.members.len() <= 1 {
                continue;
            }
            let members = group
                .members
                .iter()
                .map(|&idx| format!("{}#{}", self.texture_label(idx), idx))
                .collect::<Vec<_>>()
                .join(", ");
            eprintln!("  [{}]", members);
        }

        eprintln!("[RenderGraph][alias] pass order:");
        for pass in compiled {
            let reads = pass
                .reads
                .iter()
                .copied()
                .map(|resource| self.resource_label(resource))
                .collect::<Vec<_>>()
                .join(", ");
            let writes = pass
                .writes
                .iter()
                .copied()
                .map(|resource| self.resource_label(resource))
                .collect::<Vec<_>>()
                .join(", ");
            eprintln!(
                "  {}#{} {:?}: R=[{}] W=[{}]",
                pass.name, pass.index, pass.pass_type, reads, writes
            );
        }
    }

    #[inline]
    fn aliased_primary_for_texture(&self, tex_idx: usize) -> Option<usize> {
        if let Some(&primary) = self.alias_redirects.get(&tex_idx) {
            return Some(primary);
        }
        self.alias_redirects
            .values()
            .any(|&primary| primary == tex_idx)
            .then_some(tex_idx)
    }

    #[inline]
    fn resource_texture_index(resource: ResourceRef) -> Option<usize> {
        match resource {
            ResourceRef::Texture(handle) => Some(handle.0),
            ResourceRef::TextureSubresource(subresource) => Some(subresource.texture.0),
            ResourceRef::Surface | ResourceRef::Buffer(_) => None,
        }
    }

    fn texture_actual_usage_for(&self, handle: TextureHandle) -> wgpu::TextureUsages {
        self.textures[handle.0]
            .imported
            .as_ref()
            .map_or_else(|| self.texture_usage_for(handle), |imported| imported.usage)
    }

    fn buffer_actual_usage_for(&self, handle: BufferHandle) -> wgpu::BufferUsages {
        self.buffers[handle.0].imported.as_ref().map_or_else(
            || self.buffer_usage_for(handle),
            |imported| imported.usage(),
        )
    }

    #[inline]
    fn align_to_256(value: u32) -> u32 {
        value.div_ceil(256) * 256
    }

    fn flush_before_aliased_owner_change(
        &self,
        ctx: &mut GpuContext,
        pass: &CompiledPass,
        active_alias_owners: &mut FxHashMap<usize, usize>,
    ) {
        if self.alias_redirects.is_empty() {
            return;
        }

        for resource in pass.reads.iter().chain(pass.writes.iter()).copied() {
            let Some(tex_idx) = Self::resource_texture_index(resource) else {
                continue;
            };
            let Some(primary_idx) = self.aliased_primary_for_texture(tex_idx) else {
                continue;
            };

            if active_alias_owners
                .get(&primary_idx)
                .is_some_and(|&owner_idx| owner_idx != tex_idx)
                && ctx.has_active_frame()
            {
                if Self::trace_aliasing_enabled() {
                    let old_owner = active_alias_owners[&primary_idx];
                    eprintln!(
                        "[RenderGraph][alias] flush before pass {}: {}#{} -> {}#{} (primary {}#{})",
                        pass.name,
                        self.texture_label(old_owner),
                        old_owner,
                        self.texture_label(tex_idx),
                        tex_idx,
                        self.texture_label(primary_idx),
                        primary_idx
                    );
                }
                ctx.flush("render_graph_encoder_after_alias_owner_change");
                active_alias_owners.clear();
            }

            active_alias_owners.insert(primary_idx, tex_idx);
        }
    }

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
        if src_desc.sample_count != 1 || dst_desc.sample_count != 1 {
            return Err(RenderGraphError::InvalidTextureCopy {
                src,
                dst,
                details: Cow::Owned(format!(
                    "texture-to-texture copies require single-sampled textures, got source sample_count={} and destination sample_count={}",
                    src_desc.sample_count, dst_desc.sample_count
                )),
            });
        }
        let src_usage = self.texture_actual_usage_for(src);
        if !src_usage.contains(wgpu::TextureUsages::COPY_SRC) {
            return Err(RenderGraphError::InvalidTextureCopy {
                src,
                dst,
                details: Cow::Owned(format!(
                    "source texture usage {src_usage:?} does not include COPY_SRC"
                )),
            });
        }
        let dst_usage = self.texture_actual_usage_for(dst);
        if !dst_usage.contains(wgpu::TextureUsages::COPY_DST) {
            return Err(RenderGraphError::InvalidTextureCopy {
                src,
                dst,
                details: Cow::Owned(format!(
                    "destination texture usage {dst_usage:?} does not include COPY_DST"
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
        let src_usage = self.buffer_actual_usage_for(src);
        if !src_usage.contains(wgpu::BufferUsages::COPY_SRC) {
            return Err(RenderGraphError::InvalidBufferCopy {
                src,
                dst,
                details: Cow::Owned(format!(
                    "source buffer usage {src_usage:?} does not include COPY_SRC"
                )),
            });
        }
        let dst_usage = self.buffer_actual_usage_for(dst);
        if !dst_usage.contains(wgpu::BufferUsages::COPY_DST) {
            return Err(RenderGraphError::InvalidBufferCopy {
                src,
                dst,
                details: Cow::Owned(format!(
                    "destination buffer usage {dst_usage:?} does not include COPY_DST"
                )),
            });
        }
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
        let dst_sample_count = self.textures[dst.0].sample_count;
        if dst_sample_count != 1 {
            return Err(RenderGraphError::InvalidBufferTextureCopyLayout {
                buffer: src,
                texture: dst,
                details: Cow::Owned(format!(
                    "buffer-to-texture copies require single-sampled textures, got sample_count={dst_sample_count}"
                )),
            });
        }
        let src_usage = self.buffer_actual_usage_for(src);
        if !src_usage.contains(wgpu::BufferUsages::COPY_SRC) {
            return Err(RenderGraphError::InvalidBufferTextureCopyLayout {
                buffer: src,
                texture: dst,
                details: Cow::Owned(format!(
                    "source buffer usage {src_usage:?} does not include COPY_SRC"
                )),
            });
        }
        let dst_usage = self.texture_actual_usage_for(dst);
        if !dst_usage.contains(wgpu::TextureUsages::COPY_DST) {
            return Err(RenderGraphError::InvalidBufferTextureCopyLayout {
                buffer: src,
                texture: dst,
                details: Cow::Owned(format!(
                    "destination texture usage {dst_usage:?} does not include COPY_DST"
                )),
            });
        }
        let dst_format = self.textures[dst.0].format;
        let bytes_per_pixel = texture_format_bytes_per_pixel(dst_format).ok_or(
            RenderGraphError::UnsupportedBufferTextureCopyFormat {
                texture: dst,
                format: dst_format,
            },
        )?;
        let min_row_bytes = width * bytes_per_pixel;
        let row_bytes = match bytes_per_row {
            Some(row_bytes) => Some(row_bytes),
            None if height == 1 => None,
            None => Some(Self::align_to_256(min_row_bytes)),
        };
        if let Some(row_bytes) = row_bytes {
            if row_bytes % 256 != 0 {
                return Err(RenderGraphError::InvalidBufferTextureCopyLayout {
                    buffer: src,
                    texture: dst,
                    details: Cow::Owned(format!(
                        "bytes_per_row={row_bytes} is not 256-byte aligned"
                    )),
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
        }

        if let Some(rows_per_image) = rows_per_image {
            if rows_per_image < height {
                return Err(RenderGraphError::InvalidBufferTextureCopyLayout {
                    buffer: src,
                    texture: dst,
                    details: Cow::Owned(format!(
                        "rows_per_image={rows_per_image} is smaller than the copy height {height}"
                    )),
                });
            }
        }
        let rows_per_image = row_bytes.and(rows_per_image);

        let required_bytes = match row_bytes {
            Some(row_bytes) => {
                row_bytes as u64 * height.saturating_sub(1) as u64 + min_row_bytes as u64
            }
            None => min_row_bytes as u64,
        };
        let actual_bytes = self.buffers[src.0].size_bytes;
        if actual_bytes < required_bytes {
            return Err(RenderGraphError::InvalidBufferTextureCopyLayout {
                buffer: src,
                texture: dst,
                details: Cow::Owned(format!(
                    "source buffer is too small: needs {required_bytes} bytes, has {actual_bytes}"
                )),
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
        let dst_sample_count = self.textures[dst.0].sample_count;
        if dst_sample_count != 1 {
            return Err(RenderGraphError::InvalidTextureUpload {
                texture: dst,
                details: Cow::Owned(format!(
                    "texture uploads require single-sampled textures, got sample_count={dst_sample_count}"
                )),
            });
        }
        let dst_usage = self.texture_actual_usage_for(dst);
        if !dst_usage.contains(wgpu::TextureUsages::COPY_DST) {
            return Err(RenderGraphError::InvalidTextureUpload {
                texture: dst,
                details: Cow::Owned(format!(
                    "destination texture usage {dst_usage:?} does not include COPY_DST"
                )),
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

    fn validate_copy_op<'a>(
        &self,
        op: &'a CopyOp,
        surface_size: [u32; 2],
    ) -> Result<ValidatedCopyOp<'a>, RenderGraphError> {
        match op {
            CopyOp::TextureToTexture { src, dst } => {
                let extent = self.validate_texture_to_texture_copy(*src, *dst, surface_size)?;
                Ok(ValidatedCopyOp::TextureToTexture {
                    src: *src,
                    dst: *dst,
                    extent,
                })
            }
            CopyOp::BufferToBuffer { src, dst } => {
                let size = self.validate_buffer_to_buffer_copy(*src, *dst)?;
                Ok(ValidatedCopyOp::BufferToBuffer {
                    src: *src,
                    dst: *dst,
                    size,
                })
            }
            CopyOp::BufferToTexture {
                src,
                dst,
                bytes_per_row,
                rows_per_image,
            } => {
                let copy = self.validate_buffer_to_texture_copy(
                    *src,
                    *dst,
                    *bytes_per_row,
                    *rows_per_image,
                    surface_size,
                )?;
                Ok(ValidatedCopyOp::BufferToTexture {
                    src: *src,
                    dst: *dst,
                    copy,
                })
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
                Ok(ValidatedCopyOp::UploadToTexture {
                    data,
                    dst: *dst,
                    width: *width,
                    height: *height,
                    bytes_per_pixel: *bytes_per_pixel,
                })
            }
        }
    }

    fn validate_copy_passes<'a>(
        &self,
        compiled: &'a [CompiledPass],
        surface_size: [u32; 2],
    ) -> Result<Vec<Vec<ValidatedCopyOp<'a>>>, RenderGraphError> {
        compiled
            .iter()
            .map(|pass| {
                if pass.pass_type != PassType::Copy {
                    return Ok(Vec::new());
                }
                pass.copy_ops
                    .iter()
                    .map(|op| self.validate_copy_op(op, surface_size))
                    .collect()
            })
            .collect()
    }

    // ── Execution ───────────────────────────────────────────────────────

    /// Execute copy operations for a single copy pass.
    ///
    /// NOTE: This method takes `&self` (not `&mut self`) by design.  During
    /// `try_execute`, `PhysicalResources` holds shared borrows of several
    /// `self` fields.  If this method ever needs `&mut self`, the borrow
    /// pattern in `try_execute` must be restructured (e.g. by cloning the
    /// compiled pass list or splitting the struct).
    fn execute_copy_pass(
        &self,
        ctx: &mut GpuContext,
        pass: &CompiledPass,
        ops: &[ValidatedCopyOp<'_>],
    ) -> Result<(), RenderGraphError> {
        if ctx.has_active_frame() {
            let flush_label = format!("render_graph_encoder_after_copy_{}", pass.name);
            ctx.flush(&flush_label);
        }

        let encoder_label = format!("render_graph_copy_pass_{}#{}", pass.name, pass.index);
        let mut encoder: Option<wgpu::CommandEncoder> = None;
        let submit_pending = |ctx: &mut GpuContext, encoder: &mut Option<wgpu::CommandEncoder>| {
            if let Some(encoder) = encoder.take() {
                ctx.queue().submit(std::iter::once(encoder.finish()));
            }
        };

        for op in ops {
            match op {
                ValidatedCopyOp::TextureToTexture { src, dst, extent } => {
                    let src_tex = self.try_resolve_texture(*src)?;
                    let dst_tex = self.try_resolve_texture(*dst)?;
                    let encoder = encoder.get_or_insert_with(|| {
                        ctx.device()
                            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                                label: Some(&encoder_label),
                            })
                    });
                    encoder.copy_texture_to_texture(
                        src_tex.as_image_copy(),
                        dst_tex.as_image_copy(),
                        *extent,
                    );
                }
                ValidatedCopyOp::BufferToBuffer { src, dst, size } => {
                    let src_buf = self.try_resolve_buffer(*src)?;
                    let dst_buf = self.try_resolve_buffer(*dst)?;
                    let encoder = encoder.get_or_insert_with(|| {
                        ctx.device()
                            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                                label: Some(&encoder_label),
                            })
                    });
                    encoder.copy_buffer_to_buffer(src_buf, 0, dst_buf, 0, *size);
                }
                ValidatedCopyOp::BufferToTexture { src, dst, copy } => {
                    let src_buf = self.try_resolve_buffer(*src)?;
                    let dst_tex = self.try_resolve_texture(*dst)?;
                    let encoder = encoder.get_or_insert_with(|| {
                        ctx.device()
                            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                                label: Some(&encoder_label),
                            })
                    });
                    encoder.copy_buffer_to_texture(
                        wgpu::TexelCopyBufferInfo {
                            buffer: src_buf,
                            layout: wgpu::TexelCopyBufferLayout {
                                offset: 0,
                                bytes_per_row: copy.row_bytes,
                                rows_per_image: copy.rows_per_image,
                            },
                        },
                        dst_tex.as_image_copy(),
                        wgpu::Extent3d {
                            width: copy.width,
                            height: copy.height,
                            depth_or_array_layers: 1,
                        },
                    );
                }
                ValidatedCopyOp::UploadToTexture {
                    data,
                    dst,
                    width,
                    height,
                    bytes_per_pixel,
                } => {
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
                            bytes_per_row: Some(*width * *bytes_per_pixel),
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

    fn pass_execution_error(
        &self,
        pass: &CompiledPass,
        execution_order: usize,
        error: RenderGraphError,
    ) -> RenderGraphError {
        let compiled_order = self
            .order
            .iter()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        RenderGraphError::ExecutionFailed(format!(
            "pass \"{}\" (#{}, {:?}, exec #{execution_order}, dep {}) failed: {}; \
             compiled order [{compiled_order}]",
            pass.name, pass.index, pass.pass_type, pass.dep_level, error
        ))
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

        let compiled = std::mem::take(&mut self.cached_compiled);
        let result = {
            let validated_copy_passes =
                match self.validate_copy_passes(&compiled, ctx.surface_size()) {
                    Ok(validated) => validated,
                    Err(err) => {
                        self.cached_compiled = compiled;
                        return Err(err);
                    }
                };

            self.view_stats.reset();
            self.allocate_physical_resources(ctx);
            self.trace_aliasing_state(&compiled);

            let resources = PhysicalResources {
                handle_token: self.handle_token,
                textures: &self.physical_textures,
                buffers: &self.physical_buffers,
                texture_descs: &self.textures,
                buffer_descs: &self.buffers,
                alias_redirects: &self.alias_redirects,
                blackboard: &self.blackboard,
                view_stats: Some(&self.view_stats),
            };

            let mut err = None;
            let mut active_alias_owners = FxHashMap::default();
            for (execution_order, (pass, copy_ops)) in compiled
                .iter()
                .zip(validated_copy_passes.iter())
                .enumerate()
            {
                if Self::trace_aliasing_enabled() {
                    eprintln!(
                        "[RenderGraph][alias] executing {}#{}",
                        pass.name, pass.index
                    );
                }
                self.flush_before_aliased_owner_change(ctx, pass, &mut active_alias_owners);
                if pass.pass_type == PassType::Copy {
                    if let Err(e) = self.execute_copy_pass(ctx, pass, copy_ops) {
                        err = Some(self.pass_execution_error(pass, execution_order, e));
                        break;
                    }
                    active_alias_owners.clear();
                } else {
                    if let Err(e) = run_pass(pass, ctx, &resources) {
                        err = Some(self.pass_execution_error(pass, execution_order, e));
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

    /// Convenience wrapper around [`Self::try_execute`] that panics on error.
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

    /// Like [`Self::try_execute`], but with a [`RenderGraphProfiler`] for timing.
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

        let compiled = std::mem::take(&mut self.cached_compiled);
        let result = {
            let validated_copy_passes =
                match self.validate_copy_passes(&compiled, ctx.surface_size()) {
                    Ok(validated) => validated,
                    Err(err) => {
                        self.cached_compiled = compiled;
                        return Err(err);
                    }
                };

            self.view_stats.reset();
            self.allocate_physical_resources(ctx);
            self.trace_aliasing_state(&compiled);

            let resources = PhysicalResources {
                handle_token: self.handle_token,
                textures: &self.physical_textures,
                buffers: &self.physical_buffers,
                texture_descs: &self.textures,
                buffer_descs: &self.buffers,
                alias_redirects: &self.alias_redirects,
                blackboard: &self.blackboard,
                view_stats: Some(&self.view_stats),
            };

            let mut err = None;
            let mut active_alias_owners = FxHashMap::default();
            for (execution_order, (pass, copy_ops)) in compiled
                .iter()
                .zip(validated_copy_passes.iter())
                .enumerate()
            {
                self.flush_before_aliased_owner_change(ctx, pass, &mut active_alias_owners);
                profiler.on_pass_begin(&pass.name, pass.pass_type);
                #[cfg(feature = "profile-gpu")]
                let gpu_profile_scope = ctx.begin_gpu_profile_scope(
                    "render_graph",
                    format!("{:?}:{}", pass.pass_type, pass.name),
                );
                let start = std::time::Instant::now();
                if Self::trace_aliasing_enabled() {
                    eprintln!("[RenderGraph][alias] begin {}", pass.name);
                }
                if pass.pass_type == PassType::Copy {
                    if let Err(e) = self.execute_copy_pass(ctx, pass, copy_ops) {
                        #[cfg(feature = "profile-gpu")]
                        if let Some(scope) = gpu_profile_scope {
                            ctx.end_gpu_profile_scope(scope);
                        }
                        profiler.on_pass_end(&pass.name, start.elapsed());
                        err = Some(self.pass_execution_error(pass, execution_order, e));
                        break;
                    }
                    active_alias_owners.clear();
                } else {
                    if let Err(e) = run_pass(pass, ctx, &resources) {
                        #[cfg(feature = "profile-gpu")]
                        if let Some(scope) = gpu_profile_scope {
                            ctx.end_gpu_profile_scope(scope);
                        }
                        profiler.on_pass_end(&pass.name, start.elapsed());
                        err = Some(self.pass_execution_error(pass, execution_order, e));
                        break;
                    }
                }
                #[cfg(feature = "profile-gpu")]
                if let Some(scope) = gpu_profile_scope {
                    ctx.end_gpu_profile_scope(scope);
                }
                profiler.on_pass_end(&pass.name, start.elapsed());
                if Self::trace_aliasing_enabled() {
                    eprintln!("[RenderGraph][alias] end {}", pass.name);
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

    /// Like [`Self::execute`], but with a [`RenderGraphProfiler`] for timing.
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
