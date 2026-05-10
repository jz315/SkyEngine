/// Backend-neutral vertex attribute format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MeshVertexFormat {
    Float32,
    Float32x2,
    Float32x3,
    Float32x4,
    Uint32,
    Uint32x2,
    Uint32x3,
    Uint32x4,
    Sint32,
    Sint32x2,
    Sint32x3,
    Sint32x4,
    Unorm8x4,
    Snorm8x4,
    Uint16x2,
    Uint16x4,
    Unorm16x2,
    Unorm16x4,
}

impl MeshVertexFormat {
    #[inline]
    pub const fn byte_size(self) -> u32 {
        match self {
            Self::Float32 | Self::Uint32 | Self::Sint32 => 4,
            Self::Float32x2 | Self::Uint32x2 | Self::Sint32x2 => 8,
            Self::Float32x3 | Self::Uint32x3 | Self::Sint32x3 => 12,
            Self::Float32x4 | Self::Uint32x4 | Self::Sint32x4 => 16,
            Self::Unorm8x4 | Self::Snorm8x4 => 4,
            Self::Uint16x2 | Self::Unorm16x2 => 4,
            Self::Uint16x4 | Self::Unorm16x4 => 8,
        }
    }

    pub(crate) const fn to_wgpu(self) -> wgpu::VertexFormat {
        match self {
            Self::Float32 => wgpu::VertexFormat::Float32,
            Self::Float32x2 => wgpu::VertexFormat::Float32x2,
            Self::Float32x3 => wgpu::VertexFormat::Float32x3,
            Self::Float32x4 => wgpu::VertexFormat::Float32x4,
            Self::Uint32 => wgpu::VertexFormat::Uint32,
            Self::Uint32x2 => wgpu::VertexFormat::Uint32x2,
            Self::Uint32x3 => wgpu::VertexFormat::Uint32x3,
            Self::Uint32x4 => wgpu::VertexFormat::Uint32x4,
            Self::Sint32 => wgpu::VertexFormat::Sint32,
            Self::Sint32x2 => wgpu::VertexFormat::Sint32x2,
            Self::Sint32x3 => wgpu::VertexFormat::Sint32x3,
            Self::Sint32x4 => wgpu::VertexFormat::Sint32x4,
            Self::Unorm8x4 => wgpu::VertexFormat::Unorm8x4,
            Self::Snorm8x4 => wgpu::VertexFormat::Snorm8x4,
            Self::Uint16x2 => wgpu::VertexFormat::Uint16x2,
            Self::Uint16x4 => wgpu::VertexFormat::Uint16x4,
            Self::Unorm16x2 => wgpu::VertexFormat::Unorm16x2,
            Self::Unorm16x4 => wgpu::VertexFormat::Unorm16x4,
        }
    }
}

/// Semantic meaning attached to a mesh vertex attribute.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MeshVertexSemantic {
    Position,
    Normal,
    Tangent,
    UV0,
    UV1,
    Color,
    Custom(u32),
}

impl MeshVertexSemantic {
    pub(crate) const fn to_wgpu(self) -> crate::render::expert::VertexSemantic {
        match self {
            Self::Position => crate::render::expert::VertexSemantic::Position,
            Self::Normal => crate::render::expert::VertexSemantic::Normal,
            Self::Tangent => crate::render::expert::VertexSemantic::Tangent,
            Self::UV0 => crate::render::expert::VertexSemantic::UV0,
            Self::UV1 => crate::render::expert::VertexSemantic::UV1,
            Self::Color => crate::render::expert::VertexSemantic::Color,
            Self::Custom(value) => crate::render::expert::VertexSemantic::Custom(value),
        }
    }
}

/// One semantic vertex attribute in a [`MeshVertexLayout`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MeshVertexAttribute {
    pub semantic: MeshVertexSemantic,
    pub format: MeshVertexFormat,
    pub offset: u32,
}

impl MeshVertexAttribute {
    #[inline]
    pub const fn new(semantic: MeshVertexSemantic, format: MeshVertexFormat, offset: u32) -> Self {
        Self {
            semantic,
            format,
            offset,
        }
    }
}

/// Backend-neutral vertex buffer layout metadata.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MeshVertexLayout {
    stride: u32,
    attributes: Vec<MeshVertexAttribute>,
}

impl MeshVertexLayout {
    #[inline]
    pub fn new(stride: u32, attributes: impl Into<Vec<MeshVertexAttribute>>) -> Self {
        Self {
            stride,
            attributes: attributes.into(),
        }
    }

    #[inline]
    pub fn empty(stride: u32) -> Self {
        Self::new(stride, Vec::new())
    }

    #[inline]
    pub fn position_uv() -> Self {
        Self::new(
            20,
            [
                MeshVertexAttribute::new(
                    MeshVertexSemantic::Position,
                    MeshVertexFormat::Float32x3,
                    0,
                ),
                MeshVertexAttribute::new(MeshVertexSemantic::UV0, MeshVertexFormat::Float32x2, 12),
            ],
        )
    }

    #[inline]
    pub fn position_normal_uv() -> Self {
        Self::new(
            32,
            [
                MeshVertexAttribute::new(
                    MeshVertexSemantic::Position,
                    MeshVertexFormat::Float32x3,
                    0,
                ),
                MeshVertexAttribute::new(
                    MeshVertexSemantic::Normal,
                    MeshVertexFormat::Float32x3,
                    12,
                ),
                MeshVertexAttribute::new(MeshVertexSemantic::UV0, MeshVertexFormat::Float32x2, 24),
            ],
        )
    }

    #[inline]
    pub fn position_normal_tangent_uv() -> Self {
        Self::new(
            48,
            [
                MeshVertexAttribute::new(
                    MeshVertexSemantic::Position,
                    MeshVertexFormat::Float32x3,
                    0,
                ),
                MeshVertexAttribute::new(
                    MeshVertexSemantic::Normal,
                    MeshVertexFormat::Float32x3,
                    12,
                ),
                MeshVertexAttribute::new(
                    MeshVertexSemantic::Tangent,
                    MeshVertexFormat::Float32x4,
                    24,
                ),
                MeshVertexAttribute::new(MeshVertexSemantic::UV0, MeshVertexFormat::Float32x2, 40),
            ],
        )
    }

    #[inline]
    pub fn stride(&self) -> u32 {
        self.stride
    }

    #[inline]
    pub fn attributes(&self) -> &[MeshVertexAttribute] {
        &self.attributes
    }

    pub(crate) fn to_wgpu(&self) -> crate::render::expert::VertexLayout {
        crate::render::expert::VertexLayout::new(
            self.stride,
            self.attributes
                .iter()
                .map(|attribute| {
                    crate::render::expert::VertexAttribute::new(
                        attribute.semantic.to_wgpu(),
                        attribute.format.to_wgpu(),
                        attribute.offset,
                    )
                })
                .collect::<Vec<_>>(),
        )
    }
}
