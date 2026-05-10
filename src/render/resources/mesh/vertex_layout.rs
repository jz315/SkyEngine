/// Semantic meaning attached to a vertex attribute.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VertexSemantic {
    Position,
    Normal,
    Tangent,
    UV0,
    UV1,
    Color,
    Custom(u32),
}

/// One vertex attribute inside a [`VertexLayout`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VertexAttribute {
    pub semantic: VertexSemantic,
    pub format: wgpu::VertexFormat,
    pub offset: u32,
}

impl VertexAttribute {
    #[inline]
    pub const fn new(semantic: VertexSemantic, format: wgpu::VertexFormat, offset: u32) -> Self {
        Self {
            semantic,
            format,
            offset,
        }
    }
}

/// Vertex buffer layout metadata stored alongside a [`Mesh`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VertexLayout {
    stride: u32,
    attributes: Vec<VertexAttribute>,
}

impl VertexLayout {
    #[inline]
    pub fn new(stride: u32, attributes: impl Into<Vec<VertexAttribute>>) -> Self {
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
    pub fn stride(&self) -> u32 {
        self.stride
    }

    #[inline]
    pub fn attributes(&self) -> &[VertexAttribute] {
        &self.attributes
    }
}
