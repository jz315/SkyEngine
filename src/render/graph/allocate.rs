//! Physical GPU resource allocation, resolution, and lifecycle management.
//!
//! Contains the methods that bridge virtual (graph-declared) resources to
//! actual wgpu textures and buffers.

use super::*;
use crate::render::gpu::RenderTargetDescriptor;

impl RenderGraph {
    // ── Physical resource management ────────────────────────────────────

    pub(super) fn buffer_usage_for(&self, handle: BufferHandle) -> wgpu::BufferUsages {
        debug_assert!(
            self.compiled,
            "buffer_usage_for() called before compile() — inferred COPY_SRC/COPY_DST \
             flags from copy passes will be missing"
        );
        let mut usage = self.buffers[handle.0].usage;
        let resource = ResourceRef::Buffer(handle);

        if self.compiled {
            for &pass_idx in &self.order {
                let pass = &self.passes[pass_idx];
                if pass.reads.contains(&resource) || pass.writes.contains(&resource) {
                    usage = usage | wgpu::BufferUsages::STORAGE;
                }

                for op in &pass.copy_ops {
                    match op {
                        CopyOp::BufferToBuffer { src, dst } => {
                            if *src == handle {
                                usage = usage | wgpu::BufferUsages::COPY_SRC;
                            }
                            if *dst == handle {
                                usage = usage | wgpu::BufferUsages::COPY_DST;
                            }
                        }
                        CopyOp::BufferToTexture { src, .. } => {
                            if *src == handle {
                                usage = usage | wgpu::BufferUsages::COPY_SRC;
                            }
                        }
                        CopyOp::TextureToTexture { .. } | CopyOp::UploadToTexture { .. } => {}
                    }
                }
            }
        }

        usage
    }

    pub(super) fn try_resolve_texture(
        &self,
        handle: TextureHandle,
    ) -> Result<&wgpu::Texture, RenderGraphError> {
        if !self.texture_handle_is_valid(handle) {
            return Err(RenderGraphError::InvalidResourceHandle {
                pass: None,
                resource: ResourceRef::Texture(handle),
            });
        }
        // Follow alias redirect: secondary members point to the primary's slot.
        let resolved_idx = self
            .alias_redirects
            .get(&handle.0)
            .copied()
            .unwrap_or(handle.0);
        if let Some(rt) = self
            .physical_textures
            .get(resolved_idx)
            .and_then(|o| o.as_ref())
        {
            return Ok(rt.texture());
        }
        self.textures
            .get(handle.0)
            .and_then(|desc| desc.imported.as_ref().map(|imp| imp.texture.as_ref()))
            .ok_or(RenderGraphError::MissingPhysicalResource {
                resource: ResourceRef::Texture(handle),
            })
    }

    pub(super) fn resolve_texture_extent(
        &self,
        handle: TextureHandle,
        surface_size: [u32; 2],
    ) -> [u32; 2] {
        let desc = &self.textures[handle.0];
        if let Some(imported) = desc.imported.as_ref() {
            return imported.size;
        }
        resolve_target_size(surface_size, desc.size)
    }

    pub(super) fn try_resolve_buffer(
        &self,
        handle: BufferHandle,
    ) -> Result<&wgpu::Buffer, RenderGraphError> {
        if !self.buffer_handle_is_valid(handle) {
            return Err(RenderGraphError::InvalidResourceHandle {
                pass: None,
                resource: ResourceRef::Buffer(handle),
            });
        }
        if let Some(buf) = self.physical_buffers.get(handle.0).and_then(|o| o.as_ref()) {
            return Ok(buf);
        }
        self.buffers
            .get(handle.0)
            .and_then(|desc| desc.imported.as_ref().map(|b| b.as_ref()))
            .ok_or(RenderGraphError::MissingPhysicalResource {
                resource: ResourceRef::Buffer(handle),
            })
    }

    /// Allocate/resize physical GPU resources for all virtual textures and buffers.
    pub fn allocate_physical_resources(&mut self, ctx: &GpuContext) {
        let surface_size = ctx.surface_size();
        self.physical_textures
            .resize_with(self.textures.len(), || None);
        self.physical_buffers
            .resize_with(self.buffers.len(), || None);

        // ── Memory alias analysis (deferred from compile) ───────────────
        // Computed here instead of in compile() because we need the real
        // surface dimensions for best-fit waste calculations.
        let (alias_groups, alias_stats) = alias::compute_texture_aliases(
            &self.textures,
            &self.lifetimes,
            self.handle_token,
            surface_size,
        );
        self.alias_groups = alias_groups;
        self.alias_stats = Some(alias_stats);

        // ── Build set of aliased texture indices for fast lookup ────────
        let mut aliased_tex_indices: FxHashSet<usize> = FxHashSet::default();
        self.alias_redirects.clear();
        for group in &self.alias_groups {
            if group.members.len() > 1 {
                for &idx in &group.members {
                    aliased_tex_indices.insert(idx);
                }
            }
        }

        // ── Allocate alias groups first ─────────────────────────────────
        // Each group with >1 member shares a SINGLE physical RenderTarget.
        // Only the primary member (members[0]) gets an actual allocation;
        // secondary members redirect to the primary via alias_redirects.
        for group in &self.alias_groups {
            if group.members.len() <= 1 {
                continue;
            }

            // Resolve actual dimensions across all members.
            let mut max_w = 0u32;
            let mut max_h = 0u32;
            for &tex_idx in &group.members {
                let [w, h] = resolve_target_size(surface_size, self.textures[tex_idx].size);
                max_w = max_w.max(w);
                max_h = max_h.max(h);
            }

            // Acquire ONE shared RenderTarget from the pool.
            let key = PoolKey {
                format: group.format,
                width: max_w,
                height: max_h,
                sample_count: group.sample_count,
                mip_level_count: group.mip_level_count,
            };
            let shared_label: Cow<'static, str> = {
                let first_name = &self.textures[group.members[0]].name;
                Cow::Owned(format!(
                    "alias_group[{}+{}]",
                    first_name,
                    group.members.len() - 1
                ))
            };
            let shared_target = self.transient_pool.acquire(ctx, key, shared_label);

            // Assign the target to the primary member only.
            let primary_idx = group.members[0];
            self.physical_textures[primary_idx] = Some(shared_target);

            // Secondary members redirect to the primary.
            for &tex_idx in &group.members[1..] {
                self.alias_redirects.insert(tex_idx, primary_idx);
                // Clear any stale physical entry for this slot.
                self.physical_textures[tex_idx] = None;
            }
        }

        // ── Allocate non-aliased textures ───────────────────────────────
        for tex_idx in 0..self.textures.len() {
            let (
                desc_format,
                desc_size,
                desc_sample_count,
                desc_mip_level_count,
                desc_transient,
                desc_imported,
                desc_name,
            ) = {
                let desc = &self.textures[tex_idx];
                (
                    desc.format,
                    desc.size,
                    desc.sample_count,
                    desc.mip_level_count,
                    desc.transient,
                    desc.imported.is_some(),
                    desc.name.clone(),
                )
            };
            // Skip aliased textures (already handled above).
            if aliased_tex_indices.contains(&tex_idx) {
                continue;
            }

            let handle = TextureHandle(tex_idx, self.handle_token);
            if !self.resource_is_live(ResourceRef::Texture(handle)) {
                if let Some(target) = self.physical_textures[tex_idx].take() {
                    if desc_transient {
                        self.transient_pool.release(
                            PoolKey {
                                format: target.format(),
                                width: target.width(),
                                height: target.height(),
                                sample_count: target.sample_count(),
                                mip_level_count: target.mip_level_count(),
                            },
                            target,
                        );
                    }
                }
                continue;
            }

            if desc_imported {
                continue;
            }

            let [w, h] = resolve_target_size(surface_size, desc_size);
            let key = PoolKey {
                format: desc_format,
                width: w,
                height: h,
                sample_count: desc_sample_count,
                mip_level_count: desc_mip_level_count,
            };

            if desc_transient {
                if self.physical_textures[tex_idx].is_none() {
                    let target = self.transient_pool.acquire(ctx, key, desc_name.clone());
                    self.physical_textures[tex_idx] = Some(target);
                }
            } else {
                let descriptor = RenderTargetDescriptor::new(w, h, desc_format)
                    .sample_count(desc_sample_count)
                    .mip_level_count(desc_mip_level_count)
                    .label(desc_name.clone());

                if self.physical_textures[tex_idx].is_none() {
                    if let Some(cached) = self.take_persistent_texture(desc_name.as_ref()) {
                        self.physical_textures[tex_idx] = Some(cached);
                    }
                }

                match self.physical_textures[tex_idx].as_mut() {
                    Some(existing) => existing.resize_with(ctx, descriptor),
                    None => {
                        self.physical_textures[tex_idx] =
                            Some(RenderTarget::from_descriptor(ctx, descriptor));
                    }
                }
            }
        }

        // ── Allocate buffers ────────────────────────────────────────────
        for buf_idx in 0..self.buffers.len() {
            let (desc_size_bytes, desc_transient, desc_imported, desc_name) = {
                let desc = &self.buffers[buf_idx];
                (
                    desc.size_bytes,
                    desc.transient,
                    desc.imported.is_some(),
                    desc.name.clone(),
                )
            };
            let handle = BufferHandle(buf_idx, self.handle_token);
            if !self.resource_is_live(ResourceRef::Buffer(handle)) {
                if let Some(buffer) = self.physical_buffers[buf_idx].take() {
                    if desc_transient {
                        self.transient_buffer_pool.release(
                            BufferPoolKey {
                                size_bytes: buffer.size(),
                                usage: buffer.usage(),
                            },
                            buffer,
                        );
                    }
                }
                continue;
            }

            if desc_imported {
                continue;
            }

            let key = BufferPoolKey {
                size_bytes: desc_size_bytes,
                usage: self.buffer_usage_for(handle),
            };

            if desc_transient {
                if self.physical_buffers[buf_idx].is_none() {
                    let buffer = self
                        .transient_buffer_pool
                        .acquire(ctx, key, desc_name.clone());
                    self.physical_buffers[buf_idx] = Some(buffer);
                }
            } else {
                if self.physical_buffers[buf_idx].is_none() {
                    if let Some(cached) = self.take_persistent_buffer(desc_name.as_ref()) {
                        self.physical_buffers[buf_idx] = Some(cached);
                    }
                }

                let needs_recreate = match self.physical_buffers[buf_idx].as_ref() {
                    None => true,
                    Some(existing) => {
                        existing.size() < desc_size_bytes || !existing.usage().contains(key.usage)
                    }
                };
                if needs_recreate {
                    self.physical_buffers[buf_idx] =
                        Some(ctx.device().create_buffer(&wgpu::BufferDescriptor {
                            label: Some(desc_name.as_ref()),
                            size: desc_size_bytes,
                            usage: key.usage,
                            mapped_at_creation: false,
                        }));
                }
            }
        }
    }

    /// Return transient resources to the pool after frame execution.
    pub fn release_transient_resources(&mut self, _ctx: &GpuContext) {
        for (tex_idx, desc) in self.textures.iter().enumerate() {
            if desc.transient {
                // Skip secondary alias members — their slot is None and the
                // primary will be released on its own iteration.
                if self.alias_redirects.contains_key(&tex_idx) {
                    continue;
                }
                if let Some(target) = self.physical_textures[tex_idx].take() {
                    let key = PoolKey {
                        format: target.format(),
                        width: target.width(),
                        height: target.height(),
                        sample_count: target.sample_count(),
                        mip_level_count: target.mip_level_count(),
                    };
                    self.transient_pool.release(key, target);
                }
            }
        }

        for (buf_idx, desc) in self.buffers.iter().enumerate() {
            if desc.transient {
                if let Some(buffer) = self.physical_buffers[buf_idx].take() {
                    let key = BufferPoolKey {
                        size_bytes: buffer.size(),
                        usage: buffer.usage(),
                    };
                    self.transient_buffer_pool.release(key, buffer);
                }
            }
        }
    }

    /// Get the physical [`RenderTarget`] for a virtual texture handle.
    pub fn try_physical_texture(
        &self,
        handle: TextureHandle,
    ) -> Result<&RenderTarget, RenderGraphError> {
        if !self.texture_handle_is_valid(handle) {
            return Err(RenderGraphError::InvalidResourceHandle {
                pass: None,
                resource: ResourceRef::Texture(handle),
            });
        }
        // Follow alias redirect.
        let resolved_idx = self
            .alias_redirects
            .get(&handle.0)
            .copied()
            .unwrap_or(handle.0);
        self.physical_textures
            .get(resolved_idx)
            .and_then(|target| target.as_ref())
            .ok_or(RenderGraphError::MissingPhysicalResource {
                resource: ResourceRef::Texture(handle),
            })
    }

    /// Get the physical [`RenderTarget`] for a virtual texture handle.
    pub fn physical_texture(&self, handle: TextureHandle) -> &RenderTarget {
        self.try_physical_texture(handle)
            .expect("virtual texture not allocated — call allocate_physical_resources first")
    }

    /// Get the physical GPU buffer for a virtual buffer handle.
    pub fn try_physical_buffer(
        &self,
        handle: BufferHandle,
    ) -> Result<&wgpu::Buffer, RenderGraphError> {
        self.try_resolve_buffer(handle)
    }

    /// Get the physical GPU buffer for a virtual buffer handle.
    pub fn physical_buffer(&self, handle: BufferHandle) -> &wgpu::Buffer {
        self.try_physical_buffer(handle)
            .expect("virtual buffer not allocated — call allocate_physical_resources first")
    }
}
