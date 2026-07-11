//! Core type definitions for the render graph.

use std::any::Any;
use std::borrow::Cow;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::render::gpu::RenderTarget;
use crate::render::resources::blackboard::Blackboard;

pub(crate) type TextureFormat = wgpu::TextureFormat;

pub(crate) const DEFAULT_TEXTURE_USAGE: wgpu::TextureUsages =
    wgpu::TextureUsages::RENDER_ATTACHMENT
        .union(wgpu::TextureUsages::TEXTURE_BINDING)
        .union(wgpu::TextureUsages::COPY_SRC)
        .union(wgpu::TextureUsages::COPY_DST);

// ── Virtual resource handles ────────────────────────────────────────────────

/// Opaque handle to a virtual texture in the render graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextureHandle(pub(crate) usize, pub(crate) u64);

/// Opaque handle to a virtual buffer in the render graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BufferHandle(pub(crate) usize, pub(crate) u64);

/// Opaque handle to a graph pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PassHandle(pub(crate) usize, pub(crate) u64);

/// A mip/layer range inside a graph texture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextureSubresource {
    pub texture: TextureHandle,
    pub base_mip_level: u32,
    pub mip_level_count: u32,
    pub base_array_layer: u32,
    pub array_layer_count: u32,
}

impl TextureSubresource {
    #[inline]
    pub fn new(
        texture: TextureHandle,
        base_mip_level: u32,
        mip_level_count: u32,
        base_array_layer: u32,
        array_layer_count: u32,
    ) -> Self {
        Self {
            texture,
            base_mip_level,
            mip_level_count,
            base_array_layer,
            array_layer_count,
        }
    }
}

/// A resource that a pass can read from or write to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResourceRef {
    Surface,
    Texture(TextureHandle),
    TextureSubresource(TextureSubresource),
    Buffer(BufferHandle),
}

/// Render-target sizing policy.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TargetSize {
    /// Match the presentation surface dimensions.
    Surface,
    /// Scale factor relative to the surface (e.g. 0.5 = half res).
    Scale(f32),
    /// Fixed pixel dimensions.
    Exact(u32, u32),
}

/// Type of a render graph pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PassType {
    Render,
    Compute,
    Copy,
}

// ── Pass flags (scheduling hints) ──────────────────────────────────────────

bitflags::bitflags! {
    /// Performance and scheduling hints for a pass.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct PassFlags: u32 {
        const PREFER_ASYNC_COMPUTE    = 0x02;
        const COMPUTE_INTENSIVE       = 0x10;
        const VERTEX_BOUND_INTENSIVE  = 0x20;
        const PIXEL_BOUND_INTENSIVE   = 0x40;
        const BANDWIDTH_INTENSIVE     = 0x80;
    }
}

// ── Copy operation descriptors ─────────────────────────────────────────────

/// A single copy operation inside a `CopyPass`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CopyOp {
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
        data: Vec<u8>,
        dst: TextureHandle,
        width: u32,
        height: u32,
        bytes_per_pixel: u32,
    },
}

// ── Load operation ─────────────────────────────────────────────────────────

/// How existing contents of an attachment are loaded at pass start.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LoadOp {
    Clear([f32; 4]),
    Load,
    DontCare,
}

// ── MRT colour output slot ─────────────────────────────────────────────────

/// Describes one colour attachment slot in a render pass (MRT).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColorOutput {
    pub slot: u32,
    pub target: ResourceRef,
    pub load: LoadOp,
}

// ── Depth/stencil pass declaration ─────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DepthStencilOutput {
    pub handle: TextureHandle,
    pub clear_depth: Option<f32>,
    pub clear_stencil: Option<u32>,
    pub depth_store: bool,
    pub stencil_store: bool,
}

// ── Resource descriptors ────────────────────────────────────────────────────

pub(crate) struct TextureDesc {
    pub name: Cow<'static, str>,
    pub size: TargetSize,
    pub format: TextureFormat,
    pub usage: wgpu::TextureUsages,
    pub sample_count: u32,
    pub mip_level_count: u32,
    pub array_layer_count: u32,
    pub transient: bool,
    pub imported: Option<ImportedTexture>,
}

/// Metadata for a texture imported into the render graph from outside.
#[derive(Debug, Clone)]
pub struct ImportedTexture {
    pub texture: Arc<wgpu::Texture>,
    pub view: Arc<wgpu::TextureView>,
    pub size: [u32; 2],
    pub format: TextureFormat,
    pub usage: wgpu::TextureUsages,
    pub sample_count: u32,
    pub mip_level_count: u32,
    pub array_layer_count: u32,
}

/// A reference to a physical texture during graph execution.
#[derive(Debug)]
pub struct PhysicalTextureRef<'a> {
    pub view: &'a wgpu::TextureView,
    pub size: [u32; 2],
    pub format: TextureFormat,
    pub usage: wgpu::TextureUsages,
    pub sample_count: u32,
    pub mip_level_count: u32,
    pub array_layer_count: u32,
    pub render_target: Option<&'a RenderTarget>,
}

/// Description of a virtual buffer resource.
#[allow(dead_code)]
pub(crate) struct BufferDesc {
    pub name: Cow<'static, str>,
    pub size_bytes: u64,
    pub usage: wgpu::BufferUsages,
    pub transient: bool,
    pub imported: Option<Arc<wgpu::Buffer>>,
}

// ── Compiled pass info ──────────────────────────────────────────────────────

/// Internal pass entry (declaration only, no closures).
pub(crate) struct PassEntry {
    pub name: Cow<'static, str>,
    pub pass_type: PassType,
    pub reads: Vec<ResourceRef>,
    pub writes: Vec<ResourceRef>,
    pub color_outputs: Vec<ColorOutput>,
    pub depth_stencil: Option<DepthStencilOutput>,
    pub copy_ops: Vec<CopyOp>,
    pub flags: PassFlags,
    pub dep_level: u32,
    pub alive: bool,
}

/// Information about a compiled pass, returned to the caller for execution.
#[derive(Debug, Clone)]
pub struct CompiledPass {
    pub handle: PassHandle,
    pub index: usize,
    pub name: Cow<'static, str>,
    pub pass_type: PassType,
    pub reads: Vec<ResourceRef>,
    pub writes: Vec<ResourceRef>,
    pub color_outputs: Vec<ColorOutput>,
    pub depth_stencil: Option<DepthStencilOutput>,
    pub copy_ops: Vec<CopyOp>,
    pub flags: PassFlags,
    pub dep_level: u32,
}

/// Resource lifetime tracking (for future memory aliasing).
pub(crate) struct ResourceLifetime {
    #[allow(dead_code)]
    pub first_use: usize,
    pub last_use: usize,
}

/// Counts `PhysicalResources` view resolutions and view creations observed
/// during the latest graph execution.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PhysicalResourceViewStats {
    pub default_texture_view_resolves: u64,
    pub texture_subresource_view_creations: u64,
    pub storage_texture_view_creations: u64,
    pub render_attachment_view_creations: u64,
}

#[derive(Debug, Default)]
pub(crate) struct PhysicalResourceViewStatsCounters {
    default_texture_view_resolves: AtomicU64,
    texture_subresource_view_creations: AtomicU64,
    storage_texture_view_creations: AtomicU64,
    render_attachment_view_creations: AtomicU64,
}

impl PhysicalResourceViewStatsCounters {
    #[inline]
    pub(crate) fn reset(&self) {
        self.default_texture_view_resolves
            .store(0, Ordering::Relaxed);
        self.texture_subresource_view_creations
            .store(0, Ordering::Relaxed);
        self.storage_texture_view_creations
            .store(0, Ordering::Relaxed);
        self.render_attachment_view_creations
            .store(0, Ordering::Relaxed);
    }

    #[inline]
    pub(crate) fn snapshot(&self) -> PhysicalResourceViewStats {
        PhysicalResourceViewStats {
            default_texture_view_resolves: self
                .default_texture_view_resolves
                .load(Ordering::Relaxed),
            texture_subresource_view_creations: self
                .texture_subresource_view_creations
                .load(Ordering::Relaxed),
            storage_texture_view_creations: self
                .storage_texture_view_creations
                .load(Ordering::Relaxed),
            render_attachment_view_creations: self
                .render_attachment_view_creations
                .load(Ordering::Relaxed),
        }
    }

    #[inline]
    fn record_default_texture_view_resolve(&self) {
        self.default_texture_view_resolves
            .fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    fn record_texture_subresource_view_creation(&self) {
        self.texture_subresource_view_creations
            .fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    fn record_storage_texture_view_creation(&self) {
        self.storage_texture_view_creations
            .fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    fn record_render_attachment_view_creation(&self) {
        self.render_attachment_view_creations
            .fetch_add(1, Ordering::Relaxed);
    }
}

// ── Physical resource accessor ──────────────────────────────────────────────

/// Read-only accessor for physical resources during graph execution.
pub struct PhysicalResources<'a> {
    pub(crate) handle_token: u64,
    pub(crate) textures: &'a [Option<RenderTarget>],
    pub(crate) buffers: &'a [Option<wgpu::Buffer>],
    pub(crate) texture_descs: &'a [TextureDesc],
    pub(crate) buffer_descs: &'a [BufferDesc],
    pub(crate) alias_redirects: &'a rustc_hash::FxHashMap<usize, usize>,
    pub(crate) blackboard: &'a Blackboard,
    pub(crate) view_stats: Option<&'a PhysicalResourceViewStatsCounters>,
}

impl<'a> PhysicalResources<'a> {
    #[inline]
    fn texture_handle_matches(&self, handle: TextureHandle) -> bool {
        handle.1 == self.handle_token && handle.0 < self.texture_descs.len()
    }

    #[inline]
    fn buffer_handle_matches(&self, handle: BufferHandle) -> bool {
        handle.1 == self.handle_token && handle.0 < self.buffer_descs.len()
    }

    #[inline]
    fn texture_subresource_matches(&self, subresource: TextureSubresource) -> bool {
        if !self.texture_handle_matches(subresource.texture) {
            return false;
        }
        let desc = &self.texture_descs[subresource.texture.0];
        if subresource.mip_level_count == 0 || subresource.array_layer_count == 0 {
            return false;
        }
        let Some(mip_end) = subresource
            .base_mip_level
            .checked_add(subresource.mip_level_count)
        else {
            return false;
        };
        let Some(layer_end) = subresource
            .base_array_layer
            .checked_add(subresource.array_layer_count)
        else {
            return false;
        };
        subresource.base_mip_level < desc.mip_level_count
            && mip_end <= desc.mip_level_count
            && subresource.base_array_layer < desc.array_layer_count
            && layer_end <= desc.array_layer_count
    }

    /// Resolve a texture index, following alias redirects.
    #[inline]
    fn resolve_texture_idx(&self, idx: usize) -> usize {
        self.alias_redirects.get(&idx).copied().unwrap_or(idx)
    }

    /// Resolve a virtual texture to its backing render target when one exists.
    #[inline]
    pub fn render_target(&self, handle: TextureHandle) -> Option<&RenderTarget> {
        if !self.texture_handle_matches(handle) {
            return None;
        }
        let idx = self.resolve_texture_idx(handle.0);
        self.textures.get(idx).and_then(|o| o.as_ref())
    }

    /// Resolve a virtual texture to a [`PhysicalTextureRef`].
    #[inline]
    pub fn texture_ref(&self, handle: TextureHandle) -> PhysicalTextureRef<'_> {
        assert!(
            self.texture_handle_matches(handle),
            "texture handle does not belong to these physical resources"
        );
        if let Some(stats) = self.view_stats {
            stats.record_default_texture_view_resolve();
        }
        if let Some(rt) = self.render_target(handle) {
            return PhysicalTextureRef {
                view: rt.view(),
                size: [rt.width(), rt.height()],
                format: rt.format(),
                usage: rt.usage(),
                sample_count: rt.sample_count(),
                mip_level_count: rt.mip_level_count(),
                array_layer_count: rt.array_layer_count(),
                render_target: Some(rt),
            };
        }
        let desc = &self.texture_descs[handle.0];
        let imp = desc
            .imported
            .as_ref()
            .expect("texture_ref: virtual texture has no pool allocation and is not imported");
        PhysicalTextureRef {
            view: &imp.view,
            size: imp.size,
            format: imp.format,
            usage: imp.usage,
            sample_count: imp.sample_count,
            mip_level_count: imp.mip_level_count,
            array_layer_count: imp.array_layer_count,
            render_target: None,
        }
    }

    /// Read-only access to values published during graph setup.
    #[inline]
    pub fn blackboard(&self) -> &Blackboard {
        self.blackboard
    }

    /// Retrieve a typed blackboard value by name.
    #[inline]
    pub fn blackboard_get<T: Any>(&self, name: &str) -> Option<&T> {
        self.blackboard.get::<T>(name)
    }

    /// Resolve a virtual texture to its physical view.
    #[inline]
    pub fn view(&self, handle: TextureHandle) -> &wgpu::TextureView {
        assert!(
            self.texture_handle_matches(handle),
            "texture handle does not belong to these physical resources"
        );
        if let Some(stats) = self.view_stats {
            stats.record_default_texture_view_resolve();
        }
        if let Some(rt) = self.render_target(handle) {
            return rt.view();
        }
        self.texture_descs
            .get(handle.0)
            .and_then(|desc| desc.imported.as_ref().map(|imp| imp.view.as_ref()))
            .expect("virtual texture view not allocated")
    }

    /// Resolve a virtual texture to its full physical view.
    #[inline]
    pub fn texture_view(&self, handle: TextureHandle) -> &wgpu::TextureView {
        self.view(handle)
    }

    /// Create a view into a mip/layer range of a virtual texture.
    #[inline]
    pub fn texture_subresource_view(
        &self,
        subresource: TextureSubresource,
        dimension: wgpu::TextureViewDimension,
    ) -> wgpu::TextureView {
        if let Some(stats) = self.view_stats {
            stats.record_texture_subresource_view_creation();
        }
        assert!(
            self.texture_subresource_matches(subresource),
            "texture subresource does not belong to these physical resources or is out of range"
        );
        let descriptor = wgpu::TextureViewDescriptor {
            dimension: Some(dimension),
            base_mip_level: subresource.base_mip_level,
            mip_level_count: Some(subresource.mip_level_count),
            base_array_layer: subresource.base_array_layer,
            array_layer_count: Some(subresource.array_layer_count),
            ..Default::default()
        };

        if let Some(rt) = self.render_target(subresource.texture) {
            return rt.create_view_with(&descriptor);
        }
        let desc = &self.texture_descs[subresource.texture.0];
        let imp = desc.imported.as_ref().expect(
            "texture_subresource_view: virtual texture has no allocation and is not imported",
        );
        imp.texture.create_view(&descriptor)
    }

    /// Create a storage-compatible view into a virtual texture subresource.
    #[inline]
    pub fn storage_texture_view(
        &self,
        subresource: TextureSubresource,
        dimension: wgpu::TextureViewDimension,
    ) -> wgpu::TextureView {
        if let Some(stats) = self.view_stats {
            stats.record_storage_texture_view_creation();
        }
        let usage = self.texture_ref(subresource.texture).usage;
        assert!(
            usage.contains(wgpu::TextureUsages::STORAGE_BINDING),
            "storage_texture_view requires STORAGE_BINDING usage"
        );
        self.texture_subresource_view(subresource, dimension)
    }

    /// Create a single-mip, single-layer render attachment view.
    #[inline]
    pub fn render_attachment_view(&self, subresource: TextureSubresource) -> wgpu::TextureView {
        if let Some(stats) = self.view_stats {
            stats.record_render_attachment_view_creation();
        }
        assert_eq!(
            subresource.mip_level_count, 1,
            "render_attachment_view requires exactly one mip level"
        );
        assert_eq!(
            subresource.array_layer_count, 1,
            "render_attachment_view requires exactly one array layer"
        );
        self.texture_subresource_view(subresource, wgpu::TextureViewDimension::D2)
    }

    /// Resolve a virtual buffer to its physical GPU buffer.
    #[inline]
    pub fn buffer(&self, handle: BufferHandle) -> &wgpu::Buffer {
        assert!(
            self.buffer_handle_matches(handle),
            "buffer handle does not belong to these physical resources"
        );
        if let Some(buf) = self.buffers.get(handle.0).and_then(|o| o.as_ref()) {
            return buf;
        }
        self.buffer_descs
            .get(handle.0)
            .and_then(|desc| desc.imported.as_ref().map(|b| b.as_ref()))
            .expect("virtual buffer not allocated")
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────────

pub(crate) fn resolve_target_size(surface: [u32; 2], size: TargetSize) -> [u32; 2] {
    match size {
        TargetSize::Surface => [surface[0].max(1), surface[1].max(1)],
        TargetSize::Scale(scale) => [
            (surface[0] as f32 * scale).round().max(1.0) as u32,
            (surface[1] as f32 * scale).round().max(1.0) as u32,
        ],
        TargetSize::Exact(width, height) => [width.max(1), height.max(1)],
    }
}

#[inline]
pub(crate) fn resource_refs_overlap(a: ResourceRef, b: ResourceRef) -> bool {
    match (a, b) {
        (ResourceRef::Surface, ResourceRef::Surface) => true,
        (ResourceRef::Buffer(a), ResourceRef::Buffer(b)) => a == b,
        (ResourceRef::Texture(a), ResourceRef::Texture(b)) => a == b,
        (ResourceRef::Texture(texture), ResourceRef::TextureSubresource(sub))
        | (ResourceRef::TextureSubresource(sub), ResourceRef::Texture(texture)) => {
            texture == sub.texture
        }
        (ResourceRef::TextureSubresource(a), ResourceRef::TextureSubresource(b)) => {
            a.texture == b.texture
                && ranges_overlap(
                    a.base_mip_level,
                    a.mip_level_count,
                    b.base_mip_level,
                    b.mip_level_count,
                )
                && ranges_overlap(
                    a.base_array_layer,
                    a.array_layer_count,
                    b.base_array_layer,
                    b.array_layer_count,
                )
        }
        _ => false,
    }
}

#[inline]
fn ranges_overlap(a_start: u32, a_count: u32, b_start: u32, b_count: u32) -> bool {
    let a_end = a_start.saturating_add(a_count);
    let b_end = b_start.saturating_add(b_count);
    a_start < b_end && b_start < a_end
}

pub(crate) fn texture_format_bytes_per_pixel(format: TextureFormat) -> Option<u32> {
    match format {
        TextureFormat::Rgba8Unorm
        | TextureFormat::Rgba8UnormSrgb
        | TextureFormat::Bgra8Unorm
        | TextureFormat::Bgra8UnormSrgb
        | TextureFormat::R32Float => Some(4),
        TextureFormat::Rg32Float | TextureFormat::Rgba16Float => Some(8),
        TextureFormat::Rgba32Float => Some(16),
        TextureFormat::Depth32Float | TextureFormat::Depth24PlusStencil8 => None,
        _ => format.block_copy_size(None),
    }
}
