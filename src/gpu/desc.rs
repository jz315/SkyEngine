//! Descriptor structs for GPU resource creation.
//!
//! Following the CGPU pattern: every `create_*` method takes a descriptor
//! struct.  This is self-documenting, forward-compatible, and serialisable.

use std::borrow::Cow;

use crate::gpu::handle::*;
use crate::gpu::types::*;

// ── Buffer ──────────────────────────────────────────────────────────────────

/// Descriptor for [`Gpu::create_buffer`].
pub struct BufferDesc {
    pub label: Cow<'static, str>,
    pub size: u64,
    pub usage: BufferUsage,
}

// ── Image (texture) ─────────────────────────────────────────────────────────

/// Descriptor for [`Gpu::create_image`].
pub struct ImageDesc {
    pub label: Cow<'static, str>,
    pub width: u32,
    pub height: u32,
    pub depth: u32,
    pub format: TextureFormat,
    pub usage: ImageUsage,
    pub mip_levels: u32,
}

impl ImageDesc {
    /// Convenience: 2D image with 1 mip level.
    pub fn d2(
        label: impl Into<Cow<'static, str>>,
        width: u32,
        height: u32,
        format: TextureFormat,
    ) -> Self {
        Self {
            label: label.into(),
            width,
            height,
            depth: 1,
            format,
            usage: ImageUsage::SAMPLED | ImageUsage::COPY_DST,
            mip_levels: 1,
        }
    }
}

/// Descriptor for a sub-view of an image.
///
/// Image views allow binding a specific mip level or array layer of a
/// texture, which is essential for Bloom mip-chain passes and similar
/// techniques.
pub struct ImageViewDesc {
    pub image: Image,
    pub format: TextureFormat,
    pub base_mip_level: u32,
    pub mip_level_count: Option<u32>,
    pub base_array_layer: u32,
    pub array_layer_count: Option<u32>,
}

impl ImageViewDesc {
    /// Convenience: view a single mip level of the image.
    pub fn single_mip(image: Image, format: TextureFormat, mip_level: u32) -> Self {
        Self {
            image,
            format,
            base_mip_level: mip_level,
            mip_level_count: Some(1),
            base_array_layer: 0,
            array_layer_count: None,
        }
    }
}

/// Layout for writing pixel data to an image.
pub struct ImageCopyLayout {
    pub offset: u64,
    pub bytes_per_row: u32,
    pub rows_per_image: u32,
}

// ── Sampler ─────────────────────────────────────────────────────────────────

/// Descriptor for [`Gpu::create_sampler`].
pub struct SamplerDesc {
    pub label: Cow<'static, str>,
    pub address_mode_u: AddressMode,
    pub address_mode_v: AddressMode,
    pub mag_filter: FilterMode,
    pub min_filter: FilterMode,
    pub mipmap_filter: FilterMode,
}

impl Default for SamplerDesc {
    fn default() -> Self {
        Self {
            label: Cow::Borrowed("sampler"),
            address_mode_u: AddressMode::ClampToEdge,
            address_mode_v: AddressMode::ClampToEdge,
            mag_filter: FilterMode::Linear,
            min_filter: FilterMode::Linear,
            mipmap_filter: FilterMode::Linear,
        }
    }
}

// ── Shader ──────────────────────────────────────────────────────────────────

/// Descriptor for [`Gpu::create_shader`].
pub struct ShaderDesc {
    pub label: Cow<'static, str>,
    /// WGSL source code.
    pub source: Cow<'static, str>,
}

// ── Bind group layout ───────────────────────────────────────────────────────

/// One entry in a bind group layout.
pub struct BindGroupLayoutEntry {
    pub binding: u32,
    pub ty: BindingType,
    pub visibility: ShaderStages,
}

/// Descriptor for [`Gpu::create_bind_group_layout`].
pub struct BindGroupLayoutDesc {
    pub label: Cow<'static, str>,
    pub entries: Vec<BindGroupLayoutEntry>,
}

// ── Bind group ──────────────────────────────────────────────────────────────

/// A resource bound to one slot in a bind group.
pub enum BindGroupEntry {
    Buffer {
        binding: u32,
        buffer: Buffer,
        offset: u64,
        size: u64,
    },
    Texture {
        binding: u32,
        image: Image,
    },
    /// Bind a specific sub-view of a texture (e.g. a single mip level).
    TextureView {
        binding: u32,
        view: ImageView,
    },
    Sampler {
        binding: u32,
        sampler: Sampler,
    },
}

/// Descriptor for [`Gpu::create_bind_group`].
pub struct BindGroupDesc {
    pub label: Cow<'static, str>,
    pub layout: BindGroupLayout,
    pub entries: Vec<BindGroupEntry>,
}

// ── Render pipeline ─────────────────────────────────────────────────────────

/// Primitive assembly state.
#[derive(Debug, Clone)]
pub struct PrimitiveState {
    pub topology: PrimitiveTopology,
    pub front_face: FrontFace,
    pub cull_mode: CullMode,
}

impl Default for PrimitiveState {
    fn default() -> Self {
        Self {
            topology: PrimitiveTopology::TriangleList,
            front_face: FrontFace::Ccw,
            cull_mode: CullMode::None,
        }
    }
}

/// Depth-stencil state for the render pipeline.
#[derive(Debug, Clone)]
pub struct DepthStencilState {
    pub format: TextureFormat,
    pub depth_write: bool,
    pub depth_compare: CompareFunction,
    pub stencil_front: StencilFaceState,
    pub stencil_back: StencilFaceState,
    pub stencil_read_mask: u32,
    pub stencil_write_mask: u32,
}

impl DepthStencilState {
    /// Standard depth-test-only state (write + LessEqual, no stencil).
    pub fn depth_only(format: TextureFormat) -> Self {
        Self {
            format,
            depth_write: true,
            depth_compare: CompareFunction::LessEqual,
            stencil_front: StencilFaceState::IGNORE,
            stencil_back: StencilFaceState::IGNORE,
            stencil_read_mask: 0xFF,
            stencil_write_mask: 0xFF,
        }
    }
}

/// Color target state for one attachment.
#[derive(Debug, Clone)]
pub struct ColorTargetState {
    pub format: TextureFormat,
    pub blend: Option<BlendState>,
}

/// Descriptor for [`Gpu::create_render_pipeline`].
pub struct RenderPipelineDesc {
    pub label: Cow<'static, str>,
    pub shader: Shader,
    pub vs_entry: &'static str,
    pub fs_entry: &'static str,
    pub vertex_layouts: Vec<VertexBufferLayout>,
    pub bind_group_layouts: Vec<BindGroupLayout>,
    pub color_targets: Vec<ColorTargetState>,
    pub depth_stencil: Option<DepthStencilState>,
    pub primitive: PrimitiveState,
}

// ── Render pass ─────────────────────────────────────────────────────────────

/// Where a render pass writes colour output.
pub enum ColorTarget {
    /// Render to the window surface (swapchain).
    Surface,
    /// Render to an off-screen image.
    Image(Image),
}

/// A single colour attachment in a render pass.
pub struct ColorAttachment {
    pub target: ColorTarget,
    /// `Some([r,g,b,a])` to clear, `None` to load existing contents.
    pub clear: Option<[f32; 4]>,
}

/// Descriptor for [`Gpu::begin_render_pass`].
pub struct RenderPassDesc {
    pub label: Cow<'static, str>,
    pub color_attachments: Vec<ColorAttachment>,
    /// Optional depth/stencil attachment.
    pub depth_stencil: Option<DepthStencilAttachment>,
}

/// Depth/stencil attachment for a render pass.
pub struct DepthStencilAttachment {
    /// The depth/stencil image to write to.
    pub image: Image,
    /// `Some(value)` to clear the depth buffer, `None` to load.
    pub clear_depth: Option<f32>,
    /// `Some(value)` to clear the stencil buffer, `None` to load.
    pub clear_stencil: Option<u32>,
    /// Whether to store the depth result after the pass.
    pub depth_store: bool,
    /// Whether to store the stencil result after the pass.
    pub stencil_store: bool,
}

impl DepthStencilAttachment {
    /// Convenience: clear depth to 1.0, no stencil.
    pub fn clear(image: Image) -> Self {
        Self {
            image,
            clear_depth: Some(1.0),
            clear_stencil: None,
            depth_store: true,
            stencil_store: false,
        }
    }
}

impl RenderPassDesc {
    /// Convenience: single colour attachment clearing to given colour.
    pub fn clear_surface(color: [f32; 4]) -> Self {
        Self {
            label: Cow::Borrowed("clear_pass"),
            color_attachments: vec![ColorAttachment {
                target: ColorTarget::Surface,
                clear: Some(color),
            }],
            depth_stencil: None,
        }
    }
}

// ── Compute pipeline ────────────────────────────────────────────────────────

/// Descriptor for [`Gpu::create_compute_pipeline`].
pub struct ComputePipelineDesc {
    pub label: Cow<'static, str>,
    pub shader: Shader,
    pub entry_point: &'static str,
    pub bind_group_layouts: Vec<BindGroupLayout>,
}

/// Descriptor for [`Gpu::with_compute_pass`].
pub struct ComputePassDesc {
    pub label: Cow<'static, str>,
}

// ── Copy commands ───────────────────────────────────────────────────────────

/// Descriptor for copying data between two GPU buffers.
pub struct BufferCopyDesc {
    pub src: Buffer,
    pub src_offset: u64,
    pub dst: Buffer,
    pub dst_offset: u64,
    pub size: u64,
}

/// Descriptor for copying a GPU buffer into an image.
pub struct BufferToImageCopyDesc {
    pub src: Buffer,
    pub src_offset: u64,
    pub bytes_per_row: u32,
    pub rows_per_image: u32,
    pub dst: Image,
    pub width: u32,
    pub height: u32,
}

/// Descriptor for copying one image into another.
pub struct ImageCopyDesc {
    pub src: Image,
    pub dst: Image,
    pub width: u32,
    pub height: u32,
}
