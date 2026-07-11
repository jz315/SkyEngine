use std::borrow::Cow;
use std::sync::Arc;

use crate::asset::Asset;

use super::MeshVertexLayout;

/// Error returned by CPU-side mesh asset construction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MeshAssetError {
    EmptyVertices,
    EmptyIndices,
    InvalidVertexLayout {
        stride: u32,
    },
    VertexDataSizeMismatch {
        bytes: usize,
        stride: u32,
        vertex_count: u32,
    },
}

impl std::fmt::Display for MeshAssetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyVertices => write!(f, "MeshAsset requires at least one vertex"),
            Self::EmptyIndices => write!(f, "Indexed MeshAsset requires at least one index"),
            Self::InvalidVertexLayout { stride } => {
                write!(
                    f,
                    "MeshAsset vertex layout must use a non-zero stride (got {stride})"
                )
            }
            Self::VertexDataSizeMismatch {
                bytes,
                stride,
                vertex_count,
            } => write!(
                f,
                "MeshAsset vertex payload size mismatch: {bytes} bytes cannot describe \
                 {vertex_count} vertices with stride {stride}"
            ),
        }
    }
}

impl std::error::Error for MeshAssetError {}

/// Conservative culling volume stored with mesh assets.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeshBoundingSphere {
    pub center: [f32; 3],
    pub radius: f32,
}

impl MeshBoundingSphere {
    pub const UNBOUNDED: Self = Self {
        center: [0.0, 0.0, 0.0],
        radius: f32::INFINITY,
    };

    #[inline]
    pub const fn new(center: [f32; 3], radius: f32) -> Self {
        Self { center, radius }
    }

    pub(crate) const fn to_wgpu(self) -> crate::render::expert::resources::BoundingSphere {
        crate::render::expert::resources::BoundingSphere {
            center: self.center,
            radius: self.radius,
        }
    }
}

impl Default for MeshBoundingSphere {
    fn default() -> Self {
        Self::UNBOUNDED
    }
}

/// Continuous geometry slice inside a [`MeshAsset`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeshSubMesh {
    pub index_offset: u32,
    pub index_count: u32,
    pub vertex_offset: i32,
    pub material_index: u32,
    pub bounding_sphere: MeshBoundingSphere,
}

impl MeshSubMesh {
    #[inline]
    pub const fn new(
        index_offset: u32,
        index_count: u32,
        vertex_offset: i32,
        material_index: u32,
        bounding_sphere: MeshBoundingSphere,
    ) -> Self {
        Self {
            index_offset,
            index_count,
            vertex_offset,
            material_index,
            bounding_sphere,
        }
    }

    pub(crate) const fn to_wgpu(self) -> crate::render::expert::resources::SubMesh {
        crate::render::expert::resources::SubMesh {
            index_offset: self.index_offset,
            index_count: self.index_count,
            vertex_offset: self.vertex_offset,
            material_index: self.material_index,
            bounding_sphere: self.bounding_sphere.to_wgpu(),
        }
    }
}

/// CPU-side index buffer data for a [`MeshAsset`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MeshIndexData {
    U16(Arc<[u16]>),
    U32(Arc<[u32]>),
}

impl MeshIndexData {
    #[inline]
    pub fn u16(indices: impl Into<Arc<[u16]>>) -> Self {
        Self::U16(indices.into())
    }

    #[inline]
    pub fn u32(indices: impl Into<Arc<[u32]>>) -> Self {
        Self::U32(indices.into())
    }

    #[inline]
    pub fn count(&self) -> u32 {
        match self {
            Self::U16(indices) => indices.len() as u32,
            Self::U32(indices) => indices.len() as u32,
        }
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.count() == 0
    }

    pub(crate) fn as_wgpu(&self) -> crate::render::expert::resources::MeshIndexData<'_> {
        match self {
            Self::U16(indices) => crate::render::expert::resources::MeshIndexData::U16(indices),
            Self::U32(indices) => crate::render::expert::resources::MeshIndexData::U32(indices),
        }
    }
}

/// Raw CPU mesh descriptor used by [`MeshAsset::try_from_raw`].
#[derive(Debug, Clone)]
pub struct MeshAssetDescriptor<'a> {
    pub label: Cow<'static, str>,
    pub vertex_bytes: &'a [u8],
    pub vertex_count: u32,
    pub vertex_layout: MeshVertexLayout,
    pub indices: Option<MeshIndexData>,
    pub sub_meshes: Vec<MeshSubMesh>,
    pub bounding_sphere: MeshBoundingSphere,
}

impl<'a> MeshAssetDescriptor<'a> {
    #[inline]
    pub fn new(
        vertex_bytes: &'a [u8],
        vertex_count: u32,
        vertex_layout: MeshVertexLayout,
        label: impl Into<Cow<'static, str>>,
    ) -> Self {
        Self {
            label: label.into(),
            vertex_bytes,
            vertex_count,
            vertex_layout,
            indices: None,
            sub_meshes: Vec::new(),
            bounding_sphere: MeshBoundingSphere::UNBOUNDED,
        }
    }

    #[inline]
    pub fn with_indices(mut self, indices: MeshIndexData) -> Self {
        self.indices = Some(indices);
        self
    }

    #[inline]
    pub fn with_sub_meshes(mut self, sub_meshes: impl Into<Vec<MeshSubMesh>>) -> Self {
        self.sub_meshes = sub_meshes.into();
        self
    }

    #[inline]
    pub fn with_bounding_sphere(mut self, bounding_sphere: MeshBoundingSphere) -> Self {
        self.bounding_sphere = bounding_sphere;
        self
    }
}

/// Backend-neutral CPU mesh asset.
#[derive(Debug, Clone, PartialEq)]
pub struct MeshAsset {
    label: Cow<'static, str>,
    vertex_bytes: Arc<[u8]>,
    vertex_count: u32,
    vertex_layout: MeshVertexLayout,
    indices: Option<MeshIndexData>,
    sub_meshes: Vec<MeshSubMesh>,
    bounding_sphere: MeshBoundingSphere,
}

impl Asset for MeshAsset {
    const TYPE: &'static str = "mesh";
}

impl MeshAsset {
    pub fn from_raw(desc: MeshAssetDescriptor<'_>) -> Self {
        Self::try_from_raw(desc).expect("MeshAsset::from_raw failed")
    }

    pub fn try_from_raw(desc: MeshAssetDescriptor<'_>) -> Result<Self, MeshAssetError> {
        if desc.vertex_count == 0 || desc.vertex_bytes.is_empty() {
            return Err(MeshAssetError::EmptyVertices);
        }
        if desc.vertex_layout.stride() == 0 {
            return Err(MeshAssetError::InvalidVertexLayout {
                stride: desc.vertex_layout.stride(),
            });
        }
        let expected_bytes = desc.vertex_layout.stride() as usize * desc.vertex_count as usize;
        if desc.vertex_bytes.len() != expected_bytes {
            return Err(MeshAssetError::VertexDataSizeMismatch {
                bytes: desc.vertex_bytes.len(),
                stride: desc.vertex_layout.stride(),
                vertex_count: desc.vertex_count,
            });
        }
        if matches!(desc.indices.as_ref(), Some(indices) if indices.is_empty()) {
            return Err(MeshAssetError::EmptyIndices);
        }

        Ok(Self {
            label: desc.label,
            vertex_bytes: Arc::<[u8]>::from(desc.vertex_bytes.to_vec()),
            vertex_count: desc.vertex_count,
            vertex_layout: desc.vertex_layout,
            indices: desc.indices,
            sub_meshes: desc.sub_meshes,
            bounding_sphere: desc.bounding_sphere,
        })
    }

    pub fn from_vertices<T: bytemuck::Pod>(
        vertices: &[T],
        vertex_layout: MeshVertexLayout,
        label: impl Into<Cow<'static, str>>,
    ) -> Self {
        Self::try_from_vertices(vertices, vertex_layout, label)
            .expect("MeshAsset::from_vertices failed")
    }

    pub fn try_from_vertices<T: bytemuck::Pod>(
        vertices: &[T],
        vertex_layout: MeshVertexLayout,
        label: impl Into<Cow<'static, str>>,
    ) -> Result<Self, MeshAssetError> {
        Self::try_from_raw(MeshAssetDescriptor::new(
            bytemuck::cast_slice(vertices),
            vertices.len() as u32,
            vertex_layout,
            label,
        ))
    }

    #[inline]
    pub fn label(&self) -> &str {
        &self.label
    }

    #[inline]
    pub fn vertex_bytes(&self) -> &[u8] {
        &self.vertex_bytes
    }

    #[inline]
    pub fn vertex_count(&self) -> u32 {
        self.vertex_count
    }

    #[inline]
    pub fn vertex_layout(&self) -> &MeshVertexLayout {
        &self.vertex_layout
    }

    #[inline]
    pub fn indices(&self) -> Option<&MeshIndexData> {
        self.indices.as_ref()
    }

    #[inline]
    pub fn sub_meshes(&self) -> &[MeshSubMesh] {
        &self.sub_meshes
    }

    #[inline]
    pub fn bounding_sphere(&self) -> MeshBoundingSphere {
        self.bounding_sphere
    }

    pub(crate) fn to_wgpu_mesh(
        &self,
        gpu: &crate::gpu::GpuContext,
    ) -> Result<crate::render::expert::resources::Mesh, crate::render::expert::resources::MeshError>
    {
        let mut desc = crate::render::expert::resources::MeshDescriptor::new(
            &self.vertex_bytes,
            self.vertex_count,
            self.vertex_layout.to_wgpu(),
            self.label.to_string(),
        )
        .with_bounding_sphere(self.bounding_sphere.to_wgpu())
        .with_sub_meshes(
            self.sub_meshes
                .iter()
                .map(|sub_mesh| sub_mesh.to_wgpu())
                .collect::<Vec<_>>(),
        );

        if let Some(indices) = self.indices.as_ref() {
            desc = desc.with_indices(indices.as_wgpu());
        }

        crate::render::expert::resources::Mesh::try_from_raw(gpu, desc)
    }
}
