//! Read-only debug snapshots for render graph inspection.

use std::borrow::Cow;

use super::*;

/// Read-only snapshot of the render graph's declared and compiled state.
#[derive(Debug, Clone)]
pub struct RenderGraphDebugDump {
    /// Whether the graph has a valid cached compilation.
    pub compiled: bool,
    /// Passes in declaration order.
    pub passes: Vec<RenderGraphPassDebug>,
    /// Resources declared in the graph, including the presentation surface.
    pub resources: Vec<RenderGraphResourceDebug>,
    /// Resource lifetimes produced by the most recent compilation.
    pub lifetimes: Vec<RenderGraphLifetimeDebug>,
    /// Memory aliasing statistics from the most recent allocation, if any.
    pub aliasing: Option<AliasingStats>,
    /// Texture alias groups from the most recent allocation.
    pub alias_groups: Vec<RenderGraphAliasGroupDebug>,
    /// Secondary-to-primary texture redirects from the most recent allocation.
    pub alias_redirects: Vec<RenderGraphAliasRedirectDebug>,
    /// Alive pass indices in compiled execution order.
    pub compiled_execution_order: Vec<usize>,
    /// Number of culled passes from the most recent compilation.
    pub culled_count: usize,
    /// Maximum dependency level from the most recent compilation.
    pub max_dep_level: u32,
}

/// Debug snapshot of one pass.
#[derive(Debug, Clone)]
pub struct RenderGraphPassDebug {
    pub handle: PassHandle,
    pub declaration_order: usize,
    /// Position in compiled execution order. `None` means the pass is currently
    /// culled or the graph has not been compiled.
    pub execution_order: Option<usize>,
    pub name: Cow<'static, str>,
    pub pass_type: PassType,
    pub flags: PassFlags,
    pub dep_level: u32,
    pub alive: bool,
    pub reads: Vec<ResourceRef>,
    pub writes: Vec<ResourceRef>,
    pub color_outputs: Vec<ColorOutput>,
    pub depth_stencil: Option<DepthStencilOutput>,
    pub copy_ops: Vec<CopyOpDebug>,
}

/// Copy operation summary that avoids cloning upload payload bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CopyOpDebug {
    TextureToTexture {
        src: TextureHandle,
        dst: TextureHandle,
    },
    BufferToBuffer {
        src: BufferHandle,
        dst: BufferHandle,
    },
    BufferToTexture {
        src: BufferHandle,
        dst: TextureHandle,
        bytes_per_row: Option<u32>,
        rows_per_image: Option<u32>,
    },
    UploadToTexture {
        dst: TextureHandle,
        width: u32,
        height: u32,
        bytes_per_pixel: u32,
        data_len: usize,
    },
}

/// Resource category for a debug resource row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderGraphResourceKind {
    Surface,
    Texture,
    Buffer,
}

/// Debug snapshot of one graph resource.
#[derive(Debug, Clone)]
pub struct RenderGraphResourceDebug {
    pub resource: ResourceRef,
    pub kind: RenderGraphResourceKind,
    pub name: Cow<'static, str>,
    pub transient: bool,
    pub persistent: bool,
    pub imported: bool,
    pub live: bool,
    pub external_source: bool,
    pub external_sink: bool,
    pub texture: Option<RenderGraphTextureResourceDebug>,
    pub buffer: Option<RenderGraphBufferResourceDebug>,
}

/// Texture descriptor fields relevant to graph inspection.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderGraphTextureResourceDebug {
    pub size: TargetSize,
    pub format: wgpu::TextureFormat,
    pub usage: wgpu::TextureUsages,
    pub sample_count: u32,
    pub mip_level_count: u32,
    pub array_layer_count: u32,
}

/// Buffer descriptor fields relevant to graph inspection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderGraphBufferResourceDebug {
    pub size_bytes: u64,
    pub usage: wgpu::BufferUsages,
}

/// Lifetime span for a resource after compilation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderGraphLifetimeDebug {
    pub resource: ResourceRef,
    pub first_use: usize,
    pub last_use: usize,
}

/// Debug snapshot of one alias group from the latest physical allocation.
#[derive(Debug, Clone)]
pub struct RenderGraphAliasGroupDebug {
    pub group_index: usize,
    pub primary_texture: TextureHandle,
    pub members: Vec<RenderGraphAliasMemberDebug>,
    pub format: wgpu::TextureFormat,
    pub usage: wgpu::TextureUsages,
    pub sample_count: u32,
    pub mip_level_count: u32,
    pub array_layer_count: u32,
    pub width: u32,
    pub height: u32,
}

/// One virtual texture that participates in an alias group.
#[derive(Debug, Clone)]
pub struct RenderGraphAliasMemberDebug {
    pub texture: TextureHandle,
    pub texture_index: usize,
    pub name: Cow<'static, str>,
    pub primary: bool,
}

/// Secondary-to-primary texture redirect created by alias allocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderGraphAliasRedirectDebug {
    pub from: TextureHandle,
    pub to: TextureHandle,
}

impl RenderGraph {
    /// Build a read-only debug snapshot of the current render graph state.
    ///
    /// This method does not compile, allocate, or execute the graph.  Call
    /// [`compile`](Self::compile) first when compiled pass order, culled state,
    /// and lifetimes are required.  Call
    /// [`allocate_physical_resources`](Self::allocate_physical_resources) first
    /// when aliasing groups and alias redirects are required.
    #[must_use]
    pub fn debug_dump(&self) -> RenderGraphDebugDump {
        let mut execution_positions = FxHashMap::default();
        for (position, &pass_index) in self.order.iter().enumerate() {
            execution_positions.insert(pass_index, position);
        }

        let passes = self
            .passes
            .iter()
            .enumerate()
            .map(|(index, pass)| {
                let access = pass.access_info(index);
                RenderGraphPassDebug {
                    handle: PassHandle(index, self.handle_token),
                    declaration_order: access.pass_index,
                    execution_order: execution_positions.get(&index).copied(),
                    name: access.name.clone(),
                    pass_type: access.pass_type,
                    flags: access.flags,
                    dep_level: pass.dep_level,
                    alive: pass.alive,
                    reads: access.reads.to_vec(),
                    writes: access.writes.to_vec(),
                    color_outputs: access.color_outputs.to_vec(),
                    depth_stencil: access.depth_stencil,
                    copy_ops: access.copy_ops.iter().map(copy_op_debug).collect(),
                }
            })
            .collect();

        let resources = self.debug_resources();
        let lifetimes = self.debug_lifetimes();
        let alias_groups = self.debug_alias_groups();
        let alias_redirects = self.debug_alias_redirects();

        RenderGraphDebugDump {
            compiled: self.compiled,
            passes,
            resources,
            lifetimes,
            aliasing: self.alias_stats.clone(),
            alias_groups,
            alias_redirects,
            compiled_execution_order: self.order.clone(),
            culled_count: self.culled_count,
            max_dep_level: self.max_dep_level,
        }
    }

    fn debug_resources(&self) -> Vec<RenderGraphResourceDebug> {
        let mut resources = Vec::with_capacity(
            1usize
                .saturating_add(self.textures.len())
                .saturating_add(self.buffers.len()),
        );

        resources.push(RenderGraphResourceDebug {
            resource: ResourceRef::Surface,
            kind: RenderGraphResourceKind::Surface,
            name: Cow::Borrowed("Surface"),
            transient: false,
            persistent: false,
            imported: true,
            live: self.resource_is_live(ResourceRef::Surface),
            external_source: self.resource_has_external_source(ResourceRef::Surface),
            external_sink: self.resource_has_external_sink(ResourceRef::Surface),
            texture: None,
            buffer: None,
        });

        for (index, desc) in self.textures.iter().enumerate() {
            let handle = TextureHandle(index, self.handle_token);
            let resource = ResourceRef::Texture(handle);
            let imported = desc.imported.is_some();
            let transient = desc.transient;
            resources.push(RenderGraphResourceDebug {
                resource,
                kind: RenderGraphResourceKind::Texture,
                name: desc.name.clone(),
                transient,
                persistent: !transient && !imported,
                imported,
                live: self.resource_is_live(resource),
                external_source: self.resource_has_external_source(resource),
                external_sink: self.resource_has_external_sink(resource),
                texture: Some(RenderGraphTextureResourceDebug {
                    size: desc.size,
                    format: desc.format,
                    usage: desc.usage,
                    sample_count: desc.sample_count,
                    mip_level_count: desc.mip_level_count,
                    array_layer_count: desc.array_layer_count,
                }),
                buffer: None,
            });
        }

        for (index, desc) in self.buffers.iter().enumerate() {
            let handle = BufferHandle(index, self.handle_token);
            let resource = ResourceRef::Buffer(handle);
            let imported = desc.imported.is_some();
            let transient = desc.transient;
            resources.push(RenderGraphResourceDebug {
                resource,
                kind: RenderGraphResourceKind::Buffer,
                name: desc.name.clone(),
                transient,
                persistent: !transient && !imported,
                imported,
                live: self.resource_is_live(resource),
                external_source: self.resource_has_external_source(resource),
                external_sink: self.resource_has_external_sink(resource),
                texture: None,
                buffer: Some(RenderGraphBufferResourceDebug {
                    size_bytes: desc.size_bytes,
                    usage: desc.usage,
                }),
            });
        }

        resources
    }

    fn debug_lifetimes(&self) -> Vec<RenderGraphLifetimeDebug> {
        let mut lifetimes = self
            .lifetimes
            .iter()
            .map(|(&resource, lifetime)| RenderGraphLifetimeDebug {
                resource,
                first_use: lifetime.first_use,
                last_use: lifetime.last_use,
            })
            .collect::<Vec<_>>();
        lifetimes.sort_by_key(|lifetime| resource_sort_key(lifetime.resource));
        lifetimes
    }

    fn debug_alias_groups(&self) -> Vec<RenderGraphAliasGroupDebug> {
        self.alias_groups
            .iter()
            .enumerate()
            .map(|(group_index, group)| {
                let primary_index = group.members.first().copied().unwrap_or_default();
                let primary_texture = TextureHandle(primary_index, self.handle_token);
                let members = group
                    .members
                    .iter()
                    .copied()
                    .map(|texture_index| RenderGraphAliasMemberDebug {
                        texture: TextureHandle(texture_index, self.handle_token),
                        texture_index,
                        name: self
                            .textures
                            .get(texture_index)
                            .map(|desc| desc.name.clone())
                            .unwrap_or(Cow::Borrowed("<invalid>")),
                        primary: texture_index == primary_index,
                    })
                    .collect();

                RenderGraphAliasGroupDebug {
                    group_index,
                    primary_texture,
                    members,
                    format: group.format,
                    usage: group.usage,
                    sample_count: group.sample_count,
                    mip_level_count: group.mip_level_count,
                    array_layer_count: group.array_layer_count,
                    width: group.width,
                    height: group.height,
                }
            })
            .collect()
    }

    fn debug_alias_redirects(&self) -> Vec<RenderGraphAliasRedirectDebug> {
        let mut redirects = self
            .alias_redirects
            .iter()
            .map(|(&from, &to)| RenderGraphAliasRedirectDebug {
                from: TextureHandle(from, self.handle_token),
                to: TextureHandle(to, self.handle_token),
            })
            .collect::<Vec<_>>();
        redirects.sort_by_key(|redirect| redirect.from.0);
        redirects
    }
}

fn copy_op_debug(op: &CopyOp) -> CopyOpDebug {
    match op {
        CopyOp::TextureToTexture { src, dst } => CopyOpDebug::TextureToTexture {
            src: *src,
            dst: *dst,
        },
        CopyOp::BufferToBuffer { src, dst } => CopyOpDebug::BufferToBuffer {
            src: *src,
            dst: *dst,
        },
        CopyOp::BufferToTexture {
            src,
            dst,
            bytes_per_row,
            rows_per_image,
        } => CopyOpDebug::BufferToTexture {
            src: *src,
            dst: *dst,
            bytes_per_row: *bytes_per_row,
            rows_per_image: *rows_per_image,
        },
        CopyOp::UploadToTexture {
            data,
            dst,
            width,
            height,
            bytes_per_pixel,
        } => CopyOpDebug::UploadToTexture {
            dst: *dst,
            width: *width,
            height: *height,
            bytes_per_pixel: *bytes_per_pixel,
            data_len: data.len(),
        },
    }
}

fn resource_sort_key(resource: ResourceRef) -> (u8, usize, u32, u32) {
    match resource {
        ResourceRef::Surface => (0, 0, 0, 0),
        ResourceRef::Texture(handle) => (1, handle.0, 0, 0),
        ResourceRef::TextureSubresource(subresource) => (
            2,
            subresource.texture.0,
            subresource.base_mip_level,
            subresource.base_array_layer,
        ),
        ResourceRef::Buffer(handle) => (3, handle.0, 0, 0),
    }
}
