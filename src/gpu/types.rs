//! Backend-agnostic GPU type definitions.
//!
//! These enums and structs mirror common GPU concepts without depending on any
//! specific graphics API. Both the `gpu::Gpu` trait and all backends speak
//! this shared vocabulary.

/// Texture / surface pixel format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum TextureFormat {
    // 8-bit per channel
    Rgba8Unorm = 0,
    Rgba8UnormSrgb,
    Bgra8Unorm,
    Bgra8UnormSrgb,
    // 16-bit float
    Rgba16Float,
    // 32-bit float
    R32Float,
    Rg32Float,
    Rgba32Float,
    // Depth / stencil
    Depth32Float,
    Depth24PlusStencil8,
}

/// Buffer usage flags (bitfield).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BufferUsage(u32);

impl BufferUsage {
    pub const VERTEX: Self = Self(1 << 0);
    pub const INDEX: Self = Self(1 << 1);
    pub const UNIFORM: Self = Self(1 << 2);
    pub const STORAGE: Self = Self(1 << 3);
    pub const COPY_SRC: Self = Self(1 << 4);
    pub const COPY_DST: Self = Self(1 << 5);
    /// Buffer can be used as an indirect draw argument source.
    pub const INDIRECT: Self = Self(1 << 6);
    /// Buffer can be used for query resolution.
    pub const QUERY_RESOLVE: Self = Self(1 << 7);

    #[inline]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    #[inline]
    pub const fn bits(self) -> u32 {
        self.0
    }
}

impl std::ops::BitOr for BufferUsage {
    type Output = Self;
    #[inline]
    fn bitor(self, rhs: Self) -> Self {
        self.union(rhs)
    }
}

/// Image (texture) usage flags (bitfield).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ImageUsage(u32);

impl ImageUsage {
    pub const SAMPLED: Self = Self(1 << 0);
    pub const STORAGE: Self = Self(1 << 1);
    pub const RENDER_TARGET: Self = Self(1 << 2);
    pub const COPY_SRC: Self = Self(1 << 3);
    pub const COPY_DST: Self = Self(1 << 4);

    #[inline]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    #[inline]
    pub const fn bits(self) -> u32 {
        self.0
    }
}

impl std::ops::BitOr for ImageUsage {
    type Output = Self;
    #[inline]
    fn bitor(self, rhs: Self) -> Self {
        self.union(rhs)
    }
}

/// Index buffer element format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexFormat {
    Uint16,
    Uint32,
}

/// Vertex attribute format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VertexFormat {
    Float32,
    Float32x2,
    Float32x3,
    Float32x4,
    Uint32,
    Uint8x4,
    Unorm8x4,
}

impl VertexFormat {
    /// Size in bytes of one vertex attribute element.
    #[inline]
    pub const fn size(self) -> u64 {
        match self {
            Self::Float32 | Self::Uint32 => 4,
            Self::Float32x2 => 8,
            Self::Float32x3 => 12,
            Self::Float32x4 => 16,
            Self::Uint8x4 | Self::Unorm8x4 => 4,
        }
    }
}

/// Vertex input rate — per-vertex or per-instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VertexStepMode {
    Vertex,
    Instance,
}

/// One attribute within a vertex buffer layout.
#[derive(Debug, Clone, Copy)]
pub struct VertexAttribute {
    pub format: VertexFormat,
    pub offset: u64,
    pub shader_location: u32,
}

/// Layout of a single vertex buffer binding.
#[derive(Debug, Clone)]
pub struct VertexBufferLayout {
    pub stride: u64,
    pub step_mode: VertexStepMode,
    pub attributes: Vec<VertexAttribute>,
}

/// Primitive topology for the input assembler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrimitiveTopology {
    PointList,
    LineList,
    LineStrip,
    TriangleList,
    TriangleStrip,
}

/// Front face winding order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrontFace {
    Ccw,
    Cw,
}

/// Triangle cull mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CullMode {
    None,
    Front,
    Back,
}

/// Blend factor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlendFactor {
    Zero,
    One,
    SrcAlpha,
    OneMinusSrcAlpha,
    DstAlpha,
    OneMinusDstAlpha,
    SrcColor,
    OneMinusSrcColor,
    DstColor,
    OneMinusDstColor,
}

/// Blend operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlendOp {
    Add,
    Subtract,
    ReverseSubtract,
    Min,
    Max,
}

/// Blend state for one color attachment.
#[derive(Debug, Clone, Copy)]
pub struct BlendState {
    pub src_color: BlendFactor,
    pub dst_color: BlendFactor,
    pub color_op: BlendOp,
    pub src_alpha: BlendFactor,
    pub dst_alpha: BlendFactor,
    pub alpha_op: BlendOp,
}

impl BlendState {
    /// Standard alpha blending: `src.rgb * src.a + dst.rgb * (1 - src.a)`.
    pub const ALPHA_BLEND: Self = Self {
        src_color: BlendFactor::SrcAlpha,
        dst_color: BlendFactor::OneMinusSrcAlpha,
        color_op: BlendOp::Add,
        src_alpha: BlendFactor::One,
        dst_alpha: BlendFactor::OneMinusSrcAlpha,
        alpha_op: BlendOp::Add,
    };

    /// Pre-multiplied alpha: `src.rgb + dst.rgb * (1 - src.a)`.
    pub const PREMULTIPLIED: Self = Self {
        src_color: BlendFactor::One,
        dst_color: BlendFactor::OneMinusSrcAlpha,
        color_op: BlendOp::Add,
        src_alpha: BlendFactor::One,
        dst_alpha: BlendFactor::OneMinusSrcAlpha,
        alpha_op: BlendOp::Add,
    };

    /// Additive blending: `src + dst`.
    pub const ADDITIVE: Self = Self {
        src_color: BlendFactor::One,
        dst_color: BlendFactor::One,
        color_op: BlendOp::Add,
        src_alpha: BlendFactor::One,
        dst_alpha: BlendFactor::One,
        alpha_op: BlendOp::Add,
    };
}

/// Depth compare function.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareFunction {
    Never,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Equal,
    NotEqual,
    Always,
}

/// Stencil operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StencilOperation {
    Keep,
    Zero,
    Replace,
    Invert,
    IncrementClamp,
    DecrementClamp,
    IncrementWrap,
    DecrementWrap,
}

/// Stencil state for one face.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StencilFaceState {
    pub compare: CompareFunction,
    pub fail_op: StencilOperation,
    pub depth_fail_op: StencilOperation,
    pub pass_op: StencilOperation,
}

impl StencilFaceState {
    /// Stencil state that does nothing (passthrough).
    pub const IGNORE: Self = Self {
        compare: CompareFunction::Always,
        fail_op: StencilOperation::Keep,
        depth_fail_op: StencilOperation::Keep,
        pass_op: StencilOperation::Keep,
    };
}

/// Sampler address (wrap) mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressMode {
    ClampToEdge,
    Repeat,
    MirrorRepeat,
}

/// Sampler filter mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterMode {
    Nearest,
    Linear,
}

/// Storage texture access mode (for compute passes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StorageTextureAccess {
    ReadOnly,
    WriteOnly,
    ReadWrite,
}

/// Bind group entry type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingType {
    UniformBuffer,
    /// Read-write storage buffer.
    StorageBuffer,
    /// Read-only storage buffer (for compute shaders that only read).
    StorageBufferReadOnly,
    Texture,
    /// Non-filterable float texture (e.g. `Rgba16Float` sampled without
    /// filtering — required for HDR render targets bound as shader inputs).
    TextureNonFiltering,
    Sampler,
    /// Non-filtering sampler (paired with `TextureNonFiltering`).
    SamplerNonFiltering,
    /// Storage texture (for compute read/write).
    StorageTexture {
        access: StorageTextureAccess,
        format: TextureFormat,
    },
}

/// Shader stages where a binding is visible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShaderStages(u32);

impl ShaderStages {
    pub const VERTEX: Self = Self(1 << 0);
    pub const FRAGMENT: Self = Self(1 << 1);
    pub const COMPUTE: Self = Self(1 << 2);
    pub const VERTEX_FRAGMENT: Self = Self(Self::VERTEX.0 | Self::FRAGMENT.0);

    #[inline]
    pub const fn bits(self) -> u32 {
        self.0
    }
}

impl std::ops::BitOr for ShaderStages {
    type Output = Self;
    #[inline]
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

/// Error returned by GPU operations.
#[derive(Debug)]
pub enum GpuError {
    /// Surface lost (window minimized, etc.) — caller should skip frame.
    SurfaceLost,
    /// Out of memory.
    OutOfMemory,
    /// Backend-specific error message.
    Other(String),
}

impl std::fmt::Display for GpuError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SurfaceLost => write!(f, "GPU surface lost"),
            Self::OutOfMemory => write!(f, "GPU out of memory"),
            Self::Other(msg) => write!(f, "GPU error: {msg}"),
        }
    }
}

impl std::error::Error for GpuError {}
