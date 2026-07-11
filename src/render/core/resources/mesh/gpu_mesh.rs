use std::borrow::Cow;
use std::path::Path;

use wgpu::util::DeviceExt;

use crate::gpu::GpuContext;

use super::math::{add3, cross3, dot3, fallback_tangent, length_sq3, mul3, normalize3, sub3};
use super::{
    BoundingSphere, MeshError, MeshHandle, RayMesh, RayTriangle, SubMesh, VertexAttribute,
    VertexLayout, VertexSemantic,
};

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
    ray_mesh: Option<RayMesh>,
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
            .field("traceable", &self.ray_mesh.is_some())
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
        let ray_mesh = extract_ray_mesh(vertex_bytes, vertex_count, &vertex_layout, indices);

        Ok(Self {
            vertex_buffer,
            index_buffer,
            vertex_count,
            index_count,
            index_format: indices.map(MeshIndexData::format),
            vertex_layout,
            sub_meshes,
            bounding_sphere,
            ray_mesh,
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

            let end = sub_mesh.index_offset.saturating_add(sub_mesh.index_count);
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
    pub fn ray_mesh(&self) -> Option<&RayMesh> {
        self.ray_mesh.as_ref()
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

fn extract_ray_mesh(
    vertex_bytes: &[u8],
    vertex_count: u32,
    vertex_layout: &VertexLayout,
    indices: Option<MeshIndexData<'_>>,
) -> Option<RayMesh> {
    let position = vertex_layout.attributes().iter().find(|attribute| {
        attribute.semantic == VertexSemantic::Position
            && attribute.format == wgpu::VertexFormat::Float32x3
    })?;
    let stride = vertex_layout.stride() as usize;
    let offset = position.offset as usize;
    if offset + 12 > stride {
        return None;
    }

    let mut positions = Vec::with_capacity(vertex_count as usize);
    for vertex_index in 0..vertex_count as usize {
        let start = vertex_index * stride + offset;
        let bytes = vertex_bytes.get(start..start + 12)?;
        positions.push([
            f32::from_le_bytes(bytes[0..4].try_into().ok()?),
            f32::from_le_bytes(bytes[4..8].try_into().ok()?),
            f32::from_le_bytes(bytes[8..12].try_into().ok()?),
        ]);
    }

    let mut triangles = Vec::new();
    match indices {
        Some(MeshIndexData::U16(indices)) => {
            for triangle in indices.chunks_exact(3) {
                let i0 = triangle[0] as usize;
                let i1 = triangle[1] as usize;
                let i2 = triangle[2] as usize;
                triangles.push(RayTriangle::new(
                    *positions.get(i0)?,
                    *positions.get(i1)?,
                    *positions.get(i2)?,
                ));
            }
        }
        Some(MeshIndexData::U32(indices)) => {
            for triangle in indices.chunks_exact(3) {
                let i0 = triangle[0] as usize;
                let i1 = triangle[1] as usize;
                let i2 = triangle[2] as usize;
                triangles.push(RayTriangle::new(
                    *positions.get(i0)?,
                    *positions.get(i1)?,
                    *positions.get(i2)?,
                ));
            }
        }
        None => {
            for triangle in positions.chunks_exact(3) {
                triangles.push(RayTriangle::new(triangle[0], triangle[1], triangle[2]));
            }
        }
    }

    (!triangles.is_empty()).then(|| RayMesh::new(triangles))
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
