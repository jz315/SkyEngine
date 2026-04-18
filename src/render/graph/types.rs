//! Core type definitions for the render graph.

use std::borrow::Cow;
use std::sync::Arc;

use crate::render::gpu::RenderTarget;

pub(crate) type TextureFormat = wgpu::TextureFormat;

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

/// A resource that a pass can read from or write to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResourceRef {
    Surface,
    Texture(TextureHandle),
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
    pub sample_count: u32,
    pub mip_level_count: u32,
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
    pub sample_count: u32,
    pub mip_level_count: u32,
}

/// A reference to a physical texture during graph execution.
#[derive(Debug)]
pub struct PhysicalTextureRef<'a> {
    pub view: &'a wgpu::TextureView,
    pub size: [u32; 2],
    pub format: TextureFormat,
    pub sample_count: u32,
    pub mip_level_count: u32,
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

// ── Physical resource accessor ──────────────────────────────────────────────

/// Read-only accessor for physical resources during graph execution.
pub struct PhysicalResources<'a> {
    pub(crate) handle_token: u64,
    pub(crate) textures: &'a [Option<RenderTarget>],
    pub(crate) buffers: &'a [Option<wgpu::Buffer>],
    pub(crate) texture_descs: &'a [TextureDesc],
    pub(crate) buffer_descs: &'a [BufferDesc],
    pub(crate) alias_redirects: &'a rustc_hash::FxHashMap<usize, usize>,
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
        if let Some(rt) = self.render_target(handle) {
            return PhysicalTextureRef {
                view: rt.view(),
                size: [rt.width(), rt.height()],
                format: rt.format(),
                sample_count: rt.sample_count(),
                mip_level_count: rt.mip_level_count(),
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
            sample_count: imp.sample_count,
            mip_level_count: imp.mip_level_count,
            render_target: None,
        }
    }

    /// Resolve a virtual texture to its physical view.
    #[inline]
    pub fn view(&self, handle: TextureHandle) -> &wgpu::TextureView {
        assert!(
            self.texture_handle_matches(handle),
            "texture handle does not belong to these physical resources"
        );
        if let Some(rt) = self.render_target(handle) {
            return rt.view();
        }
        self.texture_descs
            .get(handle.0)
            .and_then(|desc| desc.imported.as_ref().map(|imp| imp.view.as_ref()))
            .expect("virtual texture view not allocated")
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
