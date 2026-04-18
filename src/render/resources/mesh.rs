//! GPU mesh buffers for custom geometry rendering.

use std::borrow::Cow;
use std::path::Path;

use rustc_hash::FxHashMap;
use wgpu::util::DeviceExt;

use crate::gpu::GpuContext;

/// Errors returned by fallible mesh APIs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MeshError {
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
    IndexedSubMeshOutOfBounds {
        end: u32,
        index_count: u32,
    },
    NonIndexedSubMeshHasIndices {
        index_offset: u32,
        index_count: u32,
    },
    GltfImport {
        path: String,
        message: String,
    },
    GltfMissingMesh {
        path: String,
    },
    GltfMissingPositions {
        mesh: String,
        primitive: usize,
    },
    GltfUnsupportedPrimitiveMode {
        mesh: String,
        primitive: usize,
        mode: String,
    },
    GltfTooManyVertices {
        count: usize,
    },
    GltfTooManyIndices {
        count: usize,
    },
}

impl std::fmt::Display for MeshError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyVertices => write!(f, "Mesh requires at least one vertex"),
            Self::EmptyIndices => write!(f, "Indexed mesh requires at least one index"),
            Self::InvalidVertexLayout { stride } => {
                write!(f, "Mesh vertex layout must use a non-zero stride (got {stride})")
            }
            Self::VertexDataSizeMismatch {
                bytes,
                stride,
                vertex_count,
            } => write!(
                f,
                "Mesh vertex payload size mismatch: {bytes} bytes cannot describe {vertex_count} vertices with stride {stride}"
            ),
            Self::IndexedSubMeshOutOfBounds { end, index_count } => write!(
                f,
                "Sub-mesh index range ends at {end}, beyond mesh index count {index_count}"
            ),
            Self::NonIndexedSubMeshHasIndices {
                index_offset,
                index_count,
            } => write!(
                f,
                "Non-indexed meshes cannot use sub-mesh index ranges (offset {index_offset}, count {index_count})"
            ),
            Self::GltfImport { path, message } => {
                write!(f, "Failed to import glTF mesh from `{path}`: {message}")
            }
            Self::GltfMissingMesh { path } => {
                write!(f, "glTF file `{path}` did not contain any triangle mesh primitives")
            }
            Self::GltfMissingPositions { mesh, primitive } => write!(
                f,
                "glTF mesh `{mesh}` primitive {primitive} is missing POSITION data"
            ),
            Self::GltfUnsupportedPrimitiveMode {
                mesh,
                primitive,
                mode,
            } => write!(
                f,
                "glTF mesh `{mesh}` primitive {primitive} uses unsupported mode `{mode}`"
            ),
            Self::GltfTooManyVertices { count } => {
                write!(f, "glTF mesh expands to {count} vertices, exceeding u32 indexing")
            }
            Self::GltfTooManyIndices { count } => {
                write!(f, "glTF mesh expands to {count} indices, exceeding u32 indexing")
            }
        }
    }
}

impl std::error::Error for MeshError {}

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

/// Conservative culling volume for a mesh or sub-mesh.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoundingSphere {
    pub center: [f32; 3],
    pub radius: f32,
}

impl BoundingSphere {
    pub const UNBOUNDED: Self = Self {
        center: [0.0, 0.0, 0.0],
        radius: f32::INFINITY,
    };

    #[inline]
    pub const fn new(center: [f32; 3], radius: f32) -> Self {
        Self { center, radius }
    }
}

impl Default for BoundingSphere {
    fn default() -> Self {
        Self::UNBOUNDED
    }
}

/// Continuous geometry slice inside a mesh.
///
/// `index_count == 0` is used for non-indexed meshes and means the sub-mesh
/// spans the mesh's full vertex range.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SubMesh {
    pub index_offset: u32,
    pub index_count: u32,
    pub vertex_offset: i32,
    pub material_index: u32,
    pub bounding_sphere: BoundingSphere,
}

impl SubMesh {
    #[inline]
    pub const fn new(
        index_offset: u32,
        index_count: u32,
        vertex_offset: i32,
        material_index: u32,
        bounding_sphere: BoundingSphere,
    ) -> Self {
        Self {
            index_offset,
            index_count,
            vertex_offset,
            material_index,
            bounding_sphere,
        }
    }
}

/// Stable handle used by future ECS-facing mesh components.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MeshHandle {
    kind: MeshHandleKind,
    slot: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum MeshHandleKind {
    Builtin,
    Dynamic,
}

impl MeshHandle {
    pub const BUILTIN_QUAD: Self = Self::builtin(0);

    #[inline]
    pub const fn builtin(slot: u32) -> Self {
        Self {
            kind: MeshHandleKind::Builtin,
            slot,
        }
    }

    #[inline]
    pub const fn dynamic(slot: u32) -> Self {
        Self {
            kind: MeshHandleKind::Dynamic,
            slot,
        }
    }

    #[inline]
    pub const fn is_builtin(self) -> bool {
        matches!(self.kind, MeshHandleKind::Builtin)
    }

    #[inline]
    pub const fn slot(self) -> u32 {
        self.slot
    }
}

/// Type-erased registry backing [`MeshHandle`] lookups.
#[derive(Clone)]
pub struct MeshRegistry {
    builtins: FxHashMap<u32, Mesh>,
    meshes: Vec<Option<Mesh>>,
    generations: Vec<u32>,
    free_list: Vec<u32>,
    len: usize,
}

impl MeshRegistry {
    #[inline]
    pub fn new() -> Self {
        Self {
            builtins: FxHashMap::default(),
            meshes: Vec::new(),
            generations: Vec::new(),
            free_list: Vec::new(),
            len: 0,
        }
    }

    pub fn ensure_builtin_quad(&mut self, ctx: &GpuContext) -> MeshHandle {
        self.builtins
            .entry(MeshHandle::BUILTIN_QUAD.slot())
            .or_insert_with(|| Mesh::builtin_quad(ctx));
        MeshHandle::BUILTIN_QUAD
    }

    pub fn insert(&mut self, mesh: Mesh) -> MeshHandle {
        let slot = if let Some(slot) = self.free_list.pop() {
            self.meshes[slot as usize] = Some(mesh);
            slot
        } else {
            let slot = self.meshes.len() as u32;
            self.meshes.push(Some(mesh));
            self.generations.push(0);
            slot
        };
        self.len += 1;
        MeshHandle::dynamic(slot)
    }

    pub fn get(&self, handle: MeshHandle) -> Option<&Mesh> {
        match handle.kind {
            MeshHandleKind::Builtin => self.builtins.get(&handle.slot),
            MeshHandleKind::Dynamic => self.meshes.get(handle.slot as usize)?.as_ref(),
        }
    }

    pub fn get_mut(&mut self, handle: MeshHandle) -> Option<&mut Mesh> {
        match handle.kind {
            MeshHandleKind::Builtin => self.builtins.get_mut(&handle.slot),
            MeshHandleKind::Dynamic => self.meshes.get_mut(handle.slot as usize)?.as_mut(),
        }
    }

    pub fn remove(&mut self, handle: MeshHandle) -> Option<Mesh> {
        if handle.is_builtin() {
            return None;
        }
        let mesh = self.meshes.get_mut(handle.slot as usize)?.take()?;
        self.generations[handle.slot as usize] =
            self.generations[handle.slot as usize].wrapping_add(1);
        self.free_list.push(handle.slot);
        self.len -= 1;
        Some(mesh)
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.len + self.builtins.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0 && self.builtins.is_empty()
    }
}

impl Default for MeshRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Raw mesh upload descriptor used by [`Mesh::from_raw`].
#[derive(Debug, Clone)]
pub struct MeshDescriptor<'a> {
    pub label: Cow<'static, str>,
    pub vertex_bytes: &'a [u8],
    pub vertex_count: u32,
    pub vertex_layout: VertexLayout,
    pub indices: Option<MeshIndexData<'a>>,
    pub sub_meshes: Vec<SubMesh>,
    pub bounding_sphere: BoundingSphere,
}

impl<'a> MeshDescriptor<'a> {
    #[inline]
    pub fn new(
        vertex_bytes: &'a [u8],
        vertex_count: u32,
        vertex_layout: VertexLayout,
        label: impl Into<Cow<'static, str>>,
    ) -> Self {
        Self {
            label: label.into(),
            vertex_bytes,
            vertex_count,
            vertex_layout,
            indices: None,
            sub_meshes: Vec::new(),
            bounding_sphere: BoundingSphere::UNBOUNDED,
        }
    }

    #[inline]
    pub fn with_indices(mut self, indices: MeshIndexData<'a>) -> Self {
        self.indices = Some(indices);
        self
    }

    #[inline]
    pub fn with_sub_meshes(mut self, sub_meshes: impl Into<Vec<SubMesh>>) -> Self {
        self.sub_meshes = sub_meshes.into();
        self
    }

    #[inline]
    pub fn with_bounding_sphere(mut self, bounding_sphere: BoundingSphere) -> Self {
        self.bounding_sphere = bounding_sphere;
        self
    }
}

/// Borrowed mesh index data accepted by [`Mesh::from_vertices_indices`].
#[derive(Debug, Clone, Copy)]
pub enum MeshIndexData<'a> {
    U16(&'a [u16]),
    U32(&'a [u32]),
}

impl<'a> MeshIndexData<'a> {
    fn is_empty(self) -> bool {
        match self {
            Self::U16(data) => data.is_empty(),
            Self::U32(data) => data.is_empty(),
        }
    }

    fn count(self) -> u32 {
        match self {
            Self::U16(data) => data.len() as u32,
            Self::U32(data) => data.len() as u32,
        }
    }

    fn format(self) -> wgpu::IndexFormat {
        match self {
            Self::U16(_) => wgpu::IndexFormat::Uint16,
            Self::U32(_) => wgpu::IndexFormat::Uint32,
        }
    }

    fn bytes(self) -> &'a [u8] {
        match self {
            Self::U16(data) => bytemuck::cast_slice(data),
            Self::U32(data) => bytemuck::cast_slice(data),
        }
    }
}

/// GPU-backed vertex/index buffers for custom draw calls.
#[derive(Clone)]
pub struct Mesh {
    vertex_buffer: wgpu::Buffer,
    index_buffer: Option<wgpu::Buffer>,
    vertex_count: u32,
    index_count: u32,
    index_format: Option<wgpu::IndexFormat>,
    vertex_layout: VertexLayout,
    sub_meshes: Vec<SubMesh>,
    bounding_sphere: BoundingSphere,
    label: Cow<'static, str>,
}

impl std::fmt::Debug for Mesh {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Mesh")
            .field("vertex_count", &self.vertex_count)
            .field("index_count", &self.index_count)
            .field("index_format", &self.index_format)
            .field("vertex_layout", &self.vertex_layout)
            .field("sub_meshes", &self.sub_meshes)
            .field("bounding_sphere", &self.bounding_sphere)
            .field("label", &self.label)
            .finish_non_exhaustive()
    }
}

impl Mesh {
    /// Built-in textured quad handle used by the planned sprite material path.
    pub const QUAD: MeshHandle = MeshHandle::BUILTIN_QUAD;

    #[inline]
    pub fn vertex_layout_position_uv() -> VertexLayout {
        VertexLayout::new(
            20,
            [
                VertexAttribute::new(VertexSemantic::Position, wgpu::VertexFormat::Float32x3, 0),
                VertexAttribute::new(VertexSemantic::UV0, wgpu::VertexFormat::Float32x2, 12),
            ],
        )
    }

    #[inline]
    pub fn vertex_layout_position_normal_uv() -> VertexLayout {
        VertexLayout::new(
            32,
            [
                VertexAttribute::new(VertexSemantic::Position, wgpu::VertexFormat::Float32x3, 0),
                VertexAttribute::new(VertexSemantic::Normal, wgpu::VertexFormat::Float32x3, 12),
                VertexAttribute::new(VertexSemantic::UV0, wgpu::VertexFormat::Float32x2, 24),
            ],
        )
    }

    #[inline]
    pub fn vertex_layout_position_normal_tangent_uv() -> VertexLayout {
        VertexLayout::new(
            48,
            [
                VertexAttribute::new(VertexSemantic::Position, wgpu::VertexFormat::Float32x3, 0),
                VertexAttribute::new(VertexSemantic::Normal, wgpu::VertexFormat::Float32x3, 12),
                VertexAttribute::new(VertexSemantic::Tangent, wgpu::VertexFormat::Float32x4, 24),
                VertexAttribute::new(VertexSemantic::UV0, wgpu::VertexFormat::Float32x2, 40),
            ],
        )
    }

    /// Create a non-indexed mesh from a typed vertex slice.
    pub fn from_vertices<T: bytemuck::Pod>(
        ctx: &GpuContext,
        vertices: &[T],
        label: impl Into<Cow<'static, str>>,
    ) -> Self {
        Self::try_from_vertices(ctx, vertices, label).expect("Mesh::from_vertices failed")
    }

    /// Fallible variant of [`Mesh::from_vertices`].
    pub fn try_from_vertices<T: bytemuck::Pod>(
        ctx: &GpuContext,
        vertices: &[T],
        label: impl Into<Cow<'static, str>>,
    ) -> Result<Self, MeshError> {
        if vertices.is_empty() {
            return Err(MeshError::EmptyVertices);
        }

        Self::try_from_raw(
            ctx,
            MeshDescriptor::new(
                bytemuck::cast_slice(vertices),
                vertices.len() as u32,
                VertexLayout::empty(std::mem::size_of::<T>() as u32),
                label,
            ),
        )
    }

    /// Create an indexed mesh from typed vertex and index slices.
    pub fn from_vertices_indices<T: bytemuck::Pod>(
        ctx: &GpuContext,
        vertices: &[T],
        indices: MeshIndexData<'_>,
        label: impl Into<Cow<'static, str>>,
    ) -> Self {
        Self::try_from_vertices_indices(ctx, vertices, indices, label)
            .expect("Mesh::from_vertices_indices failed")
    }

    /// Fallible variant of [`Mesh::from_vertices_indices`].
    pub fn try_from_vertices_indices<T: bytemuck::Pod>(
        ctx: &GpuContext,
        vertices: &[T],
        indices: MeshIndexData<'_>,
        label: impl Into<Cow<'static, str>>,
    ) -> Result<Self, MeshError> {
        if vertices.is_empty() {
            return Err(MeshError::EmptyVertices);
        }
        if indices.is_empty() {
            return Err(MeshError::EmptyIndices);
        }

        Self::try_from_raw(
            ctx,
            MeshDescriptor::new(
                bytemuck::cast_slice(vertices),
                vertices.len() as u32,
                VertexLayout::empty(std::mem::size_of::<T>() as u32),
                label,
            )
            .with_indices(indices),
        )
    }

    /// Create a mesh from explicit raw bytes and layout metadata.
    pub fn from_raw(ctx: &GpuContext, desc: MeshDescriptor<'_>) -> Self {
        Self::try_from_raw(ctx, desc).expect("Mesh::from_raw failed")
    }

    /// Fallible variant of [`Mesh::from_raw`].
    pub fn try_from_raw(ctx: &GpuContext, desc: MeshDescriptor<'_>) -> Result<Self, MeshError> {
        let MeshDescriptor {
            label,
            vertex_bytes,
            vertex_count,
            vertex_layout,
            indices,
            sub_meshes,
            bounding_sphere,
        } = desc;

        if vertex_count == 0 || vertex_bytes.is_empty() {
            return Err(MeshError::EmptyVertices);
        }
        if vertex_layout.stride() == 0 {
            return Err(MeshError::InvalidVertexLayout {
                stride: vertex_layout.stride(),
            });
        }
        let expected_bytes = vertex_layout.stride() as usize * vertex_count as usize;
        if vertex_bytes.len() != expected_bytes {
            return Err(MeshError::VertexDataSizeMismatch {
                bytes: vertex_bytes.len(),
                stride: vertex_layout.stride(),
                vertex_count,
            });
        }

        let index_count = indices.map(MeshIndexData::count).unwrap_or(0);
        if matches!(indices, Some(data) if data.is_empty()) {
            return Err(MeshError::EmptyIndices);
        }

        let sub_meshes = if sub_meshes.is_empty() {
            vec![Self::default_sub_mesh(index_count, bounding_sphere)]
        } else {
            sub_meshes
        };
        Self::validate_sub_meshes(&sub_meshes, index_count)?;

        let vertex_buffer = ctx
            .device()
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(&format!("{}_vertices", label)),
                contents: vertex_bytes,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            });
        let index_buffer = indices.map(|indices| {
            ctx.device()
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(&format!("{}_indices", label)),
                    contents: indices.bytes(),
                    usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                })
        });

        Ok(Self {
            vertex_buffer,
            index_buffer,
            vertex_count,
            index_count,
            index_format: indices.map(MeshIndexData::format),
            vertex_layout,
            sub_meshes,
            bounding_sphere,
            label,
        })
    }

    /// Create the built-in XY-plane textured quad mesh.
    pub fn builtin_quad(ctx: &GpuContext) -> Self {
        #[repr(C)]
        #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
        struct TexturedQuadVertex {
            position: [f32; 3],
            uv: [f32; 2],
        }

        const QUAD_VERTICES: [TexturedQuadVertex; 4] = [
            TexturedQuadVertex {
                position: [0.0, 0.0, 0.0],
                uv: [0.0, 1.0],
            },
            TexturedQuadVertex {
                position: [1.0, 0.0, 0.0],
                uv: [1.0, 1.0],
            },
            TexturedQuadVertex {
                position: [1.0, 1.0, 0.0],
                uv: [1.0, 0.0],
            },
            TexturedQuadVertex {
                position: [0.0, 1.0, 0.0],
                uv: [0.0, 0.0],
            },
        ];
        const QUAD_INDICES: [u16; 6] = [0, 1, 2, 0, 2, 3];

        Self::from_raw(
            ctx,
            MeshDescriptor::new(
                bytemuck::cast_slice(&QUAD_VERTICES),
                QUAD_VERTICES.len() as u32,
                Self::vertex_layout_position_uv(),
                "builtin_quad",
            )
            .with_indices(MeshIndexData::U16(&QUAD_INDICES))
            .with_bounding_sphere(BoundingSphere::new(
                [0.5, 0.5, 0.0],
                (0.5f32 * 0.5 + 0.5 * 0.5).sqrt(),
            )),
        )
    }

    /// Load a mesh from a glTF/GLB asset, expanding each triangle primitive into a sub-mesh.
    pub fn from_gltf(ctx: &GpuContext, path: impl AsRef<Path>) -> Result<Self, MeshError> {
        #[repr(C)]
        #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
        struct GltfVertex {
            position: [f32; 3],
            normal: [f32; 3],
            tangent: [f32; 4],
            uv: [f32; 2],
        }

        let path = path.as_ref();
        let path_label = path.display().to_string();
        let (document, buffers, _) = gltf::import(path).map_err(|error| MeshError::GltfImport {
            path: path_label.clone(),
            message: error.to_string(),
        })?;

        let mut vertices = Vec::<GltfVertex>::new();
        let mut indices = Vec::<u32>::new();
        let mut sub_meshes = Vec::<SubMesh>::new();
        let mut saw_primitive = false;

        for mesh in document.meshes() {
            let mesh_name = mesh.name().unwrap_or(
                path.file_stem()
                    .and_then(|name| name.to_str())
                    .unwrap_or("gltf_mesh"),
            );
            for (primitive_index, primitive) in mesh.primitives().enumerate() {
                saw_primitive = true;
                if primitive.mode() != gltf::mesh::Mode::Triangles {
                    return Err(MeshError::GltfUnsupportedPrimitiveMode {
                        mesh: mesh_name.to_string(),
                        primitive: primitive_index,
                        mode: format!("{:?}", primitive.mode()),
                    });
                }

                let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()].0));
                let positions: Vec<[f32; 3]> = reader
                    .read_positions()
                    .ok_or_else(|| MeshError::GltfMissingPositions {
                        mesh: mesh_name.to_string(),
                        primitive: primitive_index,
                    })?
                    .collect();
                let normals: Vec<[f32; 3]> = reader
                    .read_normals()
                    .map(Iterator::collect)
                    .unwrap_or_else(|| vec![[0.0, 0.0, 1.0]; positions.len()]);
                let uvs: Vec<[f32; 2]> = reader
                    .read_tex_coords(0)
                    .map(|coords| coords.into_f32().collect())
                    .unwrap_or_else(|| vec![[0.0, 0.0]; positions.len()]);
                let primitive_indices: Vec<u32> = reader
                    .read_indices()
                    .map(|indices| indices.into_u32().collect())
                    .unwrap_or_else(|| (0..positions.len() as u32).collect());
                let tangents: Vec<[f32; 4]> = reader
                    .read_tangents()
                    .map(Iterator::collect)
                    .unwrap_or_else(|| {
                        generate_tangents(&positions, &normals, &uvs, &primitive_indices)
                    });

                let base_vertex =
                    u32::try_from(vertices.len()).map_err(|_| MeshError::GltfTooManyVertices {
                        count: vertices.len(),
                    })?;
                vertices.extend(
                    positions
                        .iter()
                        .enumerate()
                        .map(|(index, position)| GltfVertex {
                            position: *position,
                            normal: normals.get(index).copied().unwrap_or([0.0, 0.0, 1.0]),
                            tangent: tangents.get(index).copied().unwrap_or([1.0, 0.0, 0.0, 1.0]),
                            uv: uvs.get(index).copied().unwrap_or([0.0, 0.0]),
                        }),
                );
                let index_offset =
                    u32::try_from(indices.len()).map_err(|_| MeshError::GltfTooManyIndices {
                        count: indices.len(),
                    })?;
                indices.extend(
                    primitive_indices
                        .into_iter()
                        .map(|index| base_vertex + index),
                );
                let index_count =
                    u32::try_from(indices.len()).map_err(|_| MeshError::GltfTooManyIndices {
                        count: indices.len(),
                    })? - index_offset;

                sub_meshes.push(SubMesh::new(
                    index_offset,
                    index_count,
                    0,
                    primitive.material().index().unwrap_or(0) as u32,
                    bounding_sphere_from_points(&positions),
                ));
            }
        }

        if !saw_primitive || vertices.is_empty() {
            return Err(MeshError::GltfMissingMesh { path: path_label });
        }

        let label = path
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or("gltf_mesh")
            .to_string();
        let bounds_points: Vec<[f32; 3]> = vertices.iter().map(|vertex| vertex.position).collect();
        Self::try_from_raw(
            ctx,
            MeshDescriptor::new(
                bytemuck::cast_slice(&vertices),
                u32::try_from(vertices.len()).map_err(|_| MeshError::GltfTooManyVertices {
                    count: vertices.len(),
                })?,
                Self::vertex_layout_position_normal_tangent_uv(),
                label,
            )
            .with_indices(MeshIndexData::U32(&indices))
            .with_sub_meshes(sub_meshes)
            .with_bounding_sphere(bounding_sphere_from_points(&bounds_points)),
        )
    }

    fn default_sub_mesh(index_count: u32, bounding_sphere: BoundingSphere) -> SubMesh {
        SubMesh::new(0, index_count, 0, 0, bounding_sphere)
    }

    fn validate_sub_meshes(sub_meshes: &[SubMesh], mesh_index_count: u32) -> Result<(), MeshError> {
        for sub_mesh in sub_meshes {
            if mesh_index_count == 0 {
                if sub_mesh.index_offset != 0 || sub_mesh.index_count != 0 {
                    return Err(MeshError::NonIndexedSubMeshHasIndices {
                        index_offset: sub_mesh.index_offset,
                        index_count: sub_mesh.index_count,
                    });
                }
                continue;
            }

            let end = sub_mesh
                .index_offset
                .checked_add(sub_mesh.index_count)
                .unwrap_or(u32::MAX);
            if end > mesh_index_count {
                return Err(MeshError::IndexedSubMeshOutOfBounds {
                    end,
                    index_count: mesh_index_count,
                });
            }
        }
        Ok(())
    }

    #[inline]
    pub fn vertex_buffer(&self) -> &wgpu::Buffer {
        &self.vertex_buffer
    }

    #[inline]
    pub fn index_buffer(&self) -> Option<&wgpu::Buffer> {
        self.index_buffer.as_ref()
    }

    #[inline]
    pub fn vertex_count(&self) -> u32 {
        self.vertex_count
    }

    #[inline]
    pub fn index_count(&self) -> u32 {
        self.index_count
    }

    #[inline]
    pub fn index_format(&self) -> Option<wgpu::IndexFormat> {
        self.index_format
    }

    #[inline]
    pub fn vertex_layout(&self) -> &VertexLayout {
        &self.vertex_layout
    }

    #[inline]
    pub fn sub_meshes(&self) -> &[SubMesh] {
        &self.sub_meshes
    }

    #[inline]
    pub fn bounding_sphere(&self) -> BoundingSphere {
        self.bounding_sphere
    }

    #[inline]
    pub fn has_indices(&self) -> bool {
        self.index_buffer.is_some()
    }

    #[inline]
    pub fn label(&self) -> &str {
        &self.label
    }
}

fn bounding_sphere_from_points(points: &[[f32; 3]]) -> BoundingSphere {
    if points.is_empty() {
        return BoundingSphere::UNBOUNDED;
    }

    let mut min = points[0];
    let mut max = points[0];
    for point in &points[1..] {
        for axis in 0..3 {
            min[axis] = min[axis].min(point[axis]);
            max[axis] = max[axis].max(point[axis]);
        }
    }
    let center = [
        (min[0] + max[0]) * 0.5,
        (min[1] + max[1]) * 0.5,
        (min[2] + max[2]) * 0.5,
    ];
    let radius = points
        .iter()
        .map(|point| {
            let dx = point[0] - center[0];
            let dy = point[1] - center[1];
            let dz = point[2] - center[2];
            (dx * dx + dy * dy + dz * dz).sqrt()
        })
        .fold(0.0, f32::max);
    BoundingSphere::new(center, radius)
}

fn generate_tangents(
    positions: &[[f32; 3]],
    normals: &[[f32; 3]],
    uvs: &[[f32; 2]],
    indices: &[u32],
) -> Vec<[f32; 4]> {
    let vertex_count = positions.len();
    if vertex_count == 0 {
        return Vec::new();
    }

    let mut tangent_accum = vec![[0.0; 3]; vertex_count];
    let mut bitangent_accum = vec![[0.0; 3]; vertex_count];

    for triangle in indices.chunks_exact(3) {
        let [i0, i1, i2] = [
            triangle[0] as usize,
            triangle[1] as usize,
            triangle[2] as usize,
        ];
        if i0 >= vertex_count || i1 >= vertex_count || i2 >= vertex_count {
            continue;
        }

        let p0 = positions[i0];
        let p1 = positions[i1];
        let p2 = positions[i2];
        let uv0 = uvs.get(i0).copied().unwrap_or([0.0, 0.0]);
        let uv1 = uvs.get(i1).copied().unwrap_or([0.0, 0.0]);
        let uv2 = uvs.get(i2).copied().unwrap_or([0.0, 0.0]);

        let edge1 = sub3(p1, p0);
        let edge2 = sub3(p2, p0);
        let delta_uv1 = [uv1[0] - uv0[0], uv1[1] - uv0[1]];
        let delta_uv2 = [uv2[0] - uv0[0], uv2[1] - uv0[1]];
        let determinant = delta_uv1[0] * delta_uv2[1] - delta_uv1[1] * delta_uv2[0];
        if determinant.abs() <= f32::EPSILON {
            continue;
        }

        let inv_det = determinant.recip();
        let tangent = mul3(
            sub3(mul3(edge1, delta_uv2[1]), mul3(edge2, delta_uv1[1])),
            inv_det,
        );
        let bitangent = mul3(
            sub3(mul3(edge2, delta_uv1[0]), mul3(edge1, delta_uv2[0])),
            inv_det,
        );

        for index in [i0, i1, i2] {
            tangent_accum[index] = add3(tangent_accum[index], tangent);
            bitangent_accum[index] = add3(bitangent_accum[index], bitangent);
        }
    }

    positions
        .iter()
        .enumerate()
        .map(|(index, _)| {
            let normal = normalize3(normals.get(index).copied().unwrap_or([0.0, 0.0, 1.0]));
            let tangent = tangent_accum[index];
            let orthogonal = sub3(tangent, mul3(normal, dot3(normal, tangent)));
            let tangent = if length_sq3(orthogonal) <= 1e-8 {
                fallback_tangent(normal)
            } else {
                normalize3(orthogonal)
            };
            let bitangent = bitangent_accum[index];
            let handedness = if dot3(cross3(normal, tangent), bitangent) < 0.0 {
                -1.0
            } else {
                1.0
            };
            [tangent[0], tangent[1], tangent[2], handedness]
        })
        .collect()
}

#[inline]
fn add3(lhs: [f32; 3], rhs: [f32; 3]) -> [f32; 3] {
    [lhs[0] + rhs[0], lhs[1] + rhs[1], lhs[2] + rhs[2]]
}

#[inline]
fn sub3(lhs: [f32; 3], rhs: [f32; 3]) -> [f32; 3] {
    [lhs[0] - rhs[0], lhs[1] - rhs[1], lhs[2] - rhs[2]]
}

#[inline]
fn mul3(value: [f32; 3], scalar: f32) -> [f32; 3] {
    [value[0] * scalar, value[1] * scalar, value[2] * scalar]
}

#[inline]
fn dot3(lhs: [f32; 3], rhs: [f32; 3]) -> f32 {
    lhs[0] * rhs[0] + lhs[1] * rhs[1] + lhs[2] * rhs[2]
}

#[inline]
fn cross3(lhs: [f32; 3], rhs: [f32; 3]) -> [f32; 3] {
    [
        lhs[1] * rhs[2] - lhs[2] * rhs[1],
        lhs[2] * rhs[0] - lhs[0] * rhs[2],
        lhs[0] * rhs[1] - lhs[1] * rhs[0],
    ]
}

#[inline]
fn length_sq3(value: [f32; 3]) -> f32 {
    dot3(value, value)
}

#[inline]
fn normalize3(value: [f32; 3]) -> [f32; 3] {
    let len_sq = length_sq3(value);
    if len_sq <= f32::EPSILON {
        [0.0, 0.0, 1.0]
    } else {
        mul3(value, len_sq.sqrt().recip())
    }
}

fn fallback_tangent(normal: [f32; 3]) -> [f32; 3] {
    let axis = if normal[2].abs() < 0.999 {
        [0.0, 0.0, 1.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    normalize3(cross3(axis, normal))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn create_test_device() -> (wgpu::Device, wgpu::Queue) {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .expect("No suitable GPU adapter found for mesh tests");

        pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("mesh_test_device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
            },
            None,
        ))
        .expect("Failed to create test GPU device")
    }

    #[repr(C)]
    #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    struct Vertex {
        pos: [f32; 2],
    }

    #[test]
    fn indexed_mesh_tracks_counts_and_format() {
        let (device, queue) = create_test_device();
        let ctx = crate::gpu::GpuContext::new_headless(
            device,
            queue,
            wgpu::TextureFormat::Bgra8Unorm,
            [4, 4],
        );

        let mesh = Mesh::from_vertices_indices(
            &ctx,
            &[
                Vertex { pos: [0.0, 0.0] },
                Vertex { pos: [1.0, 0.0] },
                Vertex { pos: [0.0, 1.0] },
            ],
            MeshIndexData::U16(&[0, 1, 2]),
            "tri",
        );

        assert_eq!(mesh.vertex_count(), 3);
        assert_eq!(mesh.index_count(), 3);
        assert_eq!(mesh.index_format(), Some(wgpu::IndexFormat::Uint16));
        assert!(mesh.has_indices());
    }

    #[test]
    fn typed_constructor_defaults_to_untyped_layout_and_single_submesh() {
        let (device, queue) = create_test_device();
        let ctx = crate::gpu::GpuContext::new_headless(
            device,
            queue,
            wgpu::TextureFormat::Bgra8Unorm,
            [4, 4],
        );

        let mesh = Mesh::from_vertices(
            &ctx,
            &[
                Vertex { pos: [0.0, 0.0] },
                Vertex { pos: [1.0, 0.0] },
                Vertex { pos: [0.0, 1.0] },
            ],
            "triangle",
        );

        assert_eq!(
            mesh.vertex_layout(),
            &VertexLayout::empty(std::mem::size_of::<Vertex>() as u32)
        );
        assert_eq!(mesh.sub_meshes().len(), 1);
        assert_eq!(
            mesh.sub_meshes()[0],
            SubMesh::new(0, 0, 0, 0, BoundingSphere::UNBOUNDED)
        );
        assert_eq!(mesh.bounding_sphere(), BoundingSphere::UNBOUNDED);
    }

    #[test]
    fn raw_descriptor_preserves_layout_submeshes_and_bounds() {
        let (device, queue) = create_test_device();
        let ctx = crate::gpu::GpuContext::new_headless(
            device,
            queue,
            wgpu::TextureFormat::Bgra8Unorm,
            [4, 4],
        );

        #[repr(C)]
        #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
        struct QuadVertex {
            position: [f32; 3],
            uv: [f32; 2],
        }

        let vertices = [
            QuadVertex {
                position: [0.0, 0.0, 0.0],
                uv: [0.0, 1.0],
            },
            QuadVertex {
                position: [1.0, 0.0, 0.0],
                uv: [1.0, 1.0],
            },
            QuadVertex {
                position: [1.0, 1.0, 0.0],
                uv: [1.0, 0.0],
            },
            QuadVertex {
                position: [0.0, 1.0, 0.0],
                uv: [0.0, 0.0],
            },
        ];
        let layout = VertexLayout::new(
            std::mem::size_of::<QuadVertex>() as u32,
            [
                VertexAttribute::new(VertexSemantic::Position, wgpu::VertexFormat::Float32x3, 0),
                VertexAttribute::new(VertexSemantic::UV0, wgpu::VertexFormat::Float32x2, 12),
            ],
        );
        let bounds = BoundingSphere::new([0.5, 0.5, 0.0], 0.8);
        let sub_meshes = [
            SubMesh::new(0, 3, 0, 0, bounds),
            SubMesh::new(3, 3, 0, 1, bounds),
        ];

        let mesh = Mesh::from_raw(
            &ctx,
            MeshDescriptor::new(
                bytemuck::cast_slice(&vertices),
                vertices.len() as u32,
                layout.clone(),
                "quad",
            )
            .with_indices(MeshIndexData::U16(&[0, 1, 2, 0, 2, 3]))
            .with_sub_meshes(sub_meshes)
            .with_bounding_sphere(bounds),
        );

        assert_eq!(mesh.vertex_layout(), &layout);
        assert_eq!(mesh.sub_meshes(), &sub_meshes);
        assert_eq!(mesh.bounding_sphere(), bounds);
    }

    #[test]
    fn builtin_quad_matches_planned_defaults() {
        let (device, queue) = create_test_device();
        let ctx = crate::gpu::GpuContext::new_headless(
            device,
            queue,
            wgpu::TextureFormat::Bgra8Unorm,
            [4, 4],
        );

        let mesh = Mesh::builtin_quad(&ctx);

        assert_eq!(Mesh::QUAD, MeshHandle::BUILTIN_QUAD);
        assert_eq!(mesh.vertex_count(), 4);
        assert_eq!(mesh.index_count(), 6);
        assert_eq!(mesh.index_format(), Some(wgpu::IndexFormat::Uint16));
        assert_eq!(mesh.sub_meshes().len(), 1);
        assert_eq!(mesh.sub_meshes()[0].material_index, 0);
        assert_eq!(
            mesh.vertex_layout(),
            &VertexLayout::new(
                20,
                [
                    VertexAttribute::new(
                        VertexSemantic::Position,
                        wgpu::VertexFormat::Float32x3,
                        0
                    ),
                    VertexAttribute::new(VertexSemantic::UV0, wgpu::VertexFormat::Float32x2, 12),
                ],
            )
        );
        assert!((mesh.bounding_sphere().center[0] - 0.5).abs() <= 0.0001);
        assert!((mesh.bounding_sphere().center[1] - 0.5).abs() <= 0.0001);
    }

    #[test]
    fn non_indexed_submesh_ranges_are_rejected() {
        let (device, queue) = create_test_device();
        let ctx = crate::gpu::GpuContext::new_headless(
            device,
            queue,
            wgpu::TextureFormat::Bgra8Unorm,
            [4, 4],
        );

        let err = Mesh::try_from_raw(
            &ctx,
            MeshDescriptor::new(
                bytemuck::cast_slice(&[Vertex { pos: [0.0, 0.0] }]),
                1,
                VertexLayout::empty(std::mem::size_of::<Vertex>() as u32),
                "invalid_non_indexed",
            )
            .with_sub_meshes([SubMesh::new(1, 2, 0, 0, BoundingSphere::UNBOUNDED)]),
        )
        .unwrap_err();

        assert!(matches!(
            err,
            MeshError::NonIndexedSubMeshHasIndices {
                index_offset: 1,
                index_count: 2
            }
        ));
    }

    #[test]
    fn mesh_registry_tracks_builtin_and_dynamic_meshes() {
        let (device, queue) = create_test_device();
        let ctx = crate::gpu::GpuContext::new_headless(
            device,
            queue,
            wgpu::TextureFormat::Bgra8Unorm,
            [4, 4],
        );
        let mut registry = MeshRegistry::new();

        let quad = registry.ensure_builtin_quad(&ctx);
        let dynamic = registry.insert(Mesh::from_vertices(
            &ctx,
            &[Vertex { pos: [0.0, 0.0] }],
            "point",
        ));

        assert_eq!(quad, MeshHandle::BUILTIN_QUAD);
        assert!(registry.get(quad).is_some());
        assert!(registry.get(dynamic).is_some());
        assert_eq!(registry.len(), 2);
        assert!(registry.remove(quad).is_none());
        assert!(registry.remove(dynamic).is_some());
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn gltf_loader_builds_submeshes_and_material_slots() {
        fn push_aligned(buffer: &mut Vec<u8>, align: usize) {
            while buffer.len() % align != 0 {
                buffer.push(0);
            }
        }

        let dir = tempfile::tempdir().expect("tempdir should be created");
        let gltf_path = dir.path().join("multi_primitive.gltf");
        let bin_path = dir.path().join("multi_primitive.bin");

        let positions_a = [[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let normals_a = [[0.0f32, 0.0, 1.0]; 3];
        let uvs_a = [[0.0f32, 0.0], [1.0, 0.0], [0.0, 1.0]];
        let indices_a = [0u16, 1, 2];

        let positions_b = [[1.0f32, 0.0, 0.0], [2.0, 0.0, 0.0], [1.0, 1.0, 0.0]];
        let normals_b = [[0.0f32, 0.0, 1.0]; 3];
        let uvs_b = [[0.0f32, 0.0], [1.0, 0.0], [0.0, 1.0]];
        let indices_b = [0u16, 1, 2];

        let mut bin = Vec::new();
        let pos_a_offset = bin.len();
        bin.extend_from_slice(bytemuck::cast_slice(&positions_a));
        push_aligned(&mut bin, 4);
        let norm_a_offset = bin.len();
        bin.extend_from_slice(bytemuck::cast_slice(&normals_a));
        push_aligned(&mut bin, 4);
        let uv_a_offset = bin.len();
        bin.extend_from_slice(bytemuck::cast_slice(&uvs_a));
        push_aligned(&mut bin, 4);
        let idx_a_offset = bin.len();
        bin.extend_from_slice(bytemuck::cast_slice(&indices_a));
        push_aligned(&mut bin, 4);

        let pos_b_offset = bin.len();
        bin.extend_from_slice(bytemuck::cast_slice(&positions_b));
        push_aligned(&mut bin, 4);
        let norm_b_offset = bin.len();
        bin.extend_from_slice(bytemuck::cast_slice(&normals_b));
        push_aligned(&mut bin, 4);
        let uv_b_offset = bin.len();
        bin.extend_from_slice(bytemuck::cast_slice(&uvs_b));
        push_aligned(&mut bin, 4);
        let idx_b_offset = bin.len();
        bin.extend_from_slice(bytemuck::cast_slice(&indices_b));
        push_aligned(&mut bin, 4);

        fs::write(&bin_path, &bin).expect("binary gltf buffer should be written");

        let json = format!(
            r#"{{
  "asset": {{ "version": "2.0" }},
  "buffers": [
    {{ "byteLength": {buffer_len}, "uri": "multi_primitive.bin" }}
  ],
  "bufferViews": [
    {{ "buffer": 0, "byteOffset": {pos_a_offset}, "byteLength": 36 }},
    {{ "buffer": 0, "byteOffset": {norm_a_offset}, "byteLength": 36 }},
    {{ "buffer": 0, "byteOffset": {uv_a_offset}, "byteLength": 24 }},
    {{ "buffer": 0, "byteOffset": {idx_a_offset}, "byteLength": 6 }},
    {{ "buffer": 0, "byteOffset": {pos_b_offset}, "byteLength": 36 }},
    {{ "buffer": 0, "byteOffset": {norm_b_offset}, "byteLength": 36 }},
    {{ "buffer": 0, "byteOffset": {uv_b_offset}, "byteLength": 24 }},
    {{ "buffer": 0, "byteOffset": {idx_b_offset}, "byteLength": 6 }}
  ],
  "accessors": [
    {{ "bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3", "min": [0.0, 0.0, 0.0], "max": [1.0, 1.0, 0.0] }},
    {{ "bufferView": 1, "componentType": 5126, "count": 3, "type": "VEC3" }},
    {{ "bufferView": 2, "componentType": 5126, "count": 3, "type": "VEC2" }},
    {{ "bufferView": 3, "componentType": 5123, "count": 3, "type": "SCALAR" }},
    {{ "bufferView": 4, "componentType": 5126, "count": 3, "type": "VEC3", "min": [1.0, 0.0, 0.0], "max": [2.0, 1.0, 0.0] }},
    {{ "bufferView": 5, "componentType": 5126, "count": 3, "type": "VEC3" }},
    {{ "bufferView": 6, "componentType": 5126, "count": 3, "type": "VEC2" }},
    {{ "bufferView": 7, "componentType": 5123, "count": 3, "type": "SCALAR" }}
  ],
  "materials": [{{}}, {{}}],
  "meshes": [
    {{
      "primitives": [
        {{
          "attributes": {{ "POSITION": 0, "NORMAL": 1, "TEXCOORD_0": 2 }},
          "indices": 3,
          "material": 0
        }},
        {{
          "attributes": {{ "POSITION": 4, "NORMAL": 5, "TEXCOORD_0": 6 }},
          "indices": 7,
          "material": 1
        }}
      ]
    }}
  ],
  "nodes": [{{ "mesh": 0 }}],
  "scenes": [{{ "nodes": [0] }}],
  "scene": 0
}}"#,
            buffer_len = bin.len(),
            pos_a_offset = pos_a_offset,
            norm_a_offset = norm_a_offset,
            uv_a_offset = uv_a_offset,
            idx_a_offset = idx_a_offset,
            pos_b_offset = pos_b_offset,
            norm_b_offset = norm_b_offset,
            uv_b_offset = uv_b_offset,
            idx_b_offset = idx_b_offset,
        );
        fs::write(&gltf_path, json).expect("gltf json should be written");

        let (device, queue) = create_test_device();
        let ctx = crate::gpu::GpuContext::new_headless(
            device,
            queue,
            wgpu::TextureFormat::Bgra8Unorm,
            [4, 4],
        );
        let mesh = Mesh::from_gltf(&ctx, &gltf_path).expect("gltf mesh should load");

        assert_eq!(mesh.label(), "multi_primitive");
        assert_eq!(
            mesh.vertex_layout(),
            &Mesh::vertex_layout_position_normal_tangent_uv()
        );
        assert_eq!(mesh.vertex_count(), 6);
        assert_eq!(mesh.index_count(), 6);
        assert_eq!(mesh.index_format(), Some(wgpu::IndexFormat::Uint32));
        assert_eq!(mesh.sub_meshes().len(), 2);
        assert_eq!(mesh.sub_meshes()[0].material_index, 0);
        assert_eq!(mesh.sub_meshes()[0].index_count, 3);
        assert_eq!(mesh.sub_meshes()[1].material_index, 1);
        assert_eq!(mesh.sub_meshes()[1].index_offset, 3);
        assert!(mesh.bounding_sphere().radius.is_finite());
    }
}
