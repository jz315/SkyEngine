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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RayTriangle {
    pub positions: [[f32; 3]; 3],
}

impl RayTriangle {
    #[inline]
    pub const fn new(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> Self {
        Self {
            positions: [a, b, c],
        }
    }

    #[inline]
    fn centroid(self) -> [f32; 3] {
        [
            (self.positions[0][0] + self.positions[1][0] + self.positions[2][0]) / 3.0,
            (self.positions[0][1] + self.positions[1][1] + self.positions[2][1]) / 3.0,
            (self.positions[0][2] + self.positions[1][2] + self.positions[2][2]) / 3.0,
        ]
    }

    #[inline]
    fn bounds(self) -> RayAabb {
        RayAabb::from_points(&self.positions)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RayAabb {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl RayAabb {
    pub const EMPTY: Self = Self {
        min: [f32::INFINITY; 3],
        max: [f32::NEG_INFINITY; 3],
    };

    fn from_points(points: &[[f32; 3]]) -> Self {
        let mut bounds = Self::EMPTY;
        for point in points {
            bounds.grow(*point);
        }
        bounds
    }

    fn grow(&mut self, point: [f32; 3]) {
        for axis in 0..3 {
            self.min[axis] = self.min[axis].min(point[axis]);
            self.max[axis] = self.max[axis].max(point[axis]);
        }
    }

    fn union(self, other: Self) -> Self {
        let mut bounds = self;
        bounds.grow(other.min);
        bounds.grow(other.max);
        bounds
    }

    fn extent(self) -> [f32; 3] {
        [
            self.max[0] - self.min[0],
            self.max[1] - self.min[1],
            self.max[2] - self.min[2],
        ]
    }

    fn intersects_ray(self, ray: Ray, t_max: f32) -> bool {
        let mut t_min = ray.t_min;
        let mut t_max = t_max;
        for axis in 0..3 {
            let inv_dir = 1.0 / ray.direction[axis];
            let mut t0 = (self.min[axis] - ray.origin[axis]) * inv_dir;
            let mut t1 = (self.max[axis] - ray.origin[axis]) * inv_dir;
            if inv_dir < 0.0 {
                std::mem::swap(&mut t0, &mut t1);
            }
            t_min = t_min.max(t0);
            t_max = t_max.min(t1);
            if t_max < t_min {
                return false;
            }
        }
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ray {
    pub origin: [f32; 3],
    pub direction: [f32; 3],
    pub t_min: f32,
}

impl Ray {
    #[inline]
    pub const fn new(origin: [f32; 3], direction: [f32; 3]) -> Self {
        Self {
            origin,
            direction,
            t_min: 0.0001,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RayHit {
    pub t: f32,
    pub triangle_index: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RayBlasNode {
    left_first: u32,
    count: u32,
    right_child: u32,
    _pad: u32,
}

impl RayBlasNode {
    #[inline]
    pub fn is_leaf(self) -> bool {
        self.count > 0
    }

    #[inline]
    pub fn left_first(self) -> u32 {
        self.left_first
    }

    #[inline]
    pub fn count(self) -> u32 {
        self.count
    }

    #[inline]
    pub fn right_child(self) -> u32 {
        self.right_child
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RayMesh {
    triangles: Vec<RayTriangle>,
    triangle_indices: Vec<u32>,
    nodes: Vec<RayBlasNode>,
    bounds: Vec<RayAabb>,
}

impl RayMesh {
    const LEAF_SIZE: usize = 4;

    pub fn new(triangles: Vec<RayTriangle>) -> Self {
        let mut mesh = Self {
            triangle_indices: (0..triangles.len() as u32).collect(),
            triangles,
            nodes: Vec::new(),
            bounds: Vec::new(),
        };
        if !mesh.triangles.is_empty() {
            let _ = mesh.build_node(0, mesh.triangles.len());
        }
        mesh
    }

    #[inline]
    pub fn triangles(&self) -> &[RayTriangle] {
        &self.triangles
    }

    #[inline]
    pub fn nodes(&self) -> &[RayBlasNode] {
        &self.nodes
    }

    #[inline]
    pub fn node_bounds(&self) -> &[RayAabb] {
        &self.bounds
    }

    #[inline]
    pub fn triangle_indices(&self) -> &[u32] {
        &self.triangle_indices
    }

    pub fn trace(&self, ray: Ray, t_max: f32) -> Option<RayHit> {
        if self.nodes.is_empty() {
            return None;
        }
        let mut stack = [0u32; 64];
        let mut stack_len = 1usize;
        stack[0] = 0;
        let mut best_t = t_max;
        let mut best_triangle = u32::MAX;

        while stack_len > 0 {
            stack_len -= 1;
            let node_index = stack[stack_len] as usize;
            let bounds = self.bounds[node_index];
            if !bounds.intersects_ray(ray, best_t) {
                continue;
            }
            let node = self.nodes[node_index];
            if node.is_leaf() {
                for offset in 0..node.count {
                    let index = self.triangle_indices[(node.left_first + offset) as usize];
                    let triangle = self.triangles[index as usize];
                    if let Some(t) = intersect_triangle(ray, triangle, best_t) {
                        best_t = t;
                        best_triangle = index;
                    }
                }
            } else {
                if stack_len + 2 <= stack.len() {
                    stack[stack_len] = node.right_child;
                    stack[stack_len + 1] = node.left_first;
                    stack_len += 2;
                }
            }
        }

        (best_triangle != u32::MAX).then_some(RayHit {
            t: best_t,
            triangle_index: best_triangle,
        })
    }

    fn build_node(&mut self, first: usize, count: usize) -> u32 {
        let node_index = self.nodes.len() as u32;
        self.nodes.push(RayBlasNode {
            left_first: first as u32,
            count: count as u32,
            right_child: u32::MAX,
            _pad: 0,
        });
        let bounds = self.bounds_for_range(first, count);
        self.bounds.push(bounds);

        if count <= Self::LEAF_SIZE {
            return node_index;
        }

        let centroid_bounds = self.centroid_bounds_for_range(first, count);
        let extent = centroid_bounds.extent();
        let axis = if extent[0] >= extent[1] && extent[0] >= extent[2] {
            0
        } else if extent[1] >= extent[2] {
            1
        } else {
            2
        };
        if extent[axis] <= 1e-6 {
            return node_index;
        }

        self.triangle_indices[first..first + count].sort_by(|lhs, rhs| {
            let lhs_c = self.triangles[*lhs as usize].centroid()[axis];
            let rhs_c = self.triangles[*rhs as usize].centroid()[axis];
            lhs_c.total_cmp(&rhs_c)
        });

        let left_count = count / 2;
        let right_count = count - left_count;
        let left = self.build_node(first, left_count);
        let right = self.build_node(first + left_count, right_count);
        self.nodes[node_index as usize] = RayBlasNode {
            left_first: left,
            count: 0,
            right_child: right,
            _pad: 0,
        };
        node_index
    }

    fn bounds_for_range(&self, first: usize, count: usize) -> RayAabb {
        let mut bounds = RayAabb::EMPTY;
        for index in &self.triangle_indices[first..first + count] {
            bounds = bounds.union(self.triangles[*index as usize].bounds());
        }
        bounds
    }

    fn centroid_bounds_for_range(&self, first: usize, count: usize) -> RayAabb {
        let mut bounds = RayAabb::EMPTY;
        for index in &self.triangle_indices[first..first + count] {
            bounds.grow(self.triangles[*index as usize].centroid());
        }
        bounds
    }
}

fn intersect_triangle(ray: Ray, triangle: RayTriangle, t_max: f32) -> Option<f32> {
    let v0 = triangle.positions[0];
    let v1 = triangle.positions[1];
    let v2 = triangle.positions[2];
    let e1 = sub3(v1, v0);
    let e2 = sub3(v2, v0);
    let pvec = cross3(ray.direction, e2);
    let det = dot3(e1, pvec);
    if det.abs() <= 1e-7 {
        return None;
    }
    let inv_det = det.recip();
    let tvec = sub3(ray.origin, v0);
    let u = dot3(tvec, pvec) * inv_det;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let qvec = cross3(tvec, e1);
    let v = dot3(ray.direction, qvec) * inv_det;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = dot3(e2, qvec) * inv_det;
    (t >= ray.t_min && t <= t_max).then_some(t)
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
    fn raw_position_mesh_builds_ray_geometry_and_bvh() {
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

        let mesh = Mesh::from_raw(
            &ctx,
            MeshDescriptor::new(
                bytemuck::cast_slice(&vertices),
                vertices.len() as u32,
                layout,
                "ray_quad",
            )
            .with_indices(MeshIndexData::U16(&[0, 1, 2, 0, 2, 3])),
        );

        let ray_mesh = mesh
            .ray_mesh()
            .expect("position meshes should generate CPU ray geometry");
        assert_eq!(ray_mesh.triangles().len(), 2);
        assert!(!ray_mesh.nodes().is_empty());
        let hit = ray_mesh
            .trace(Ray::new([0.25, 0.25, 1.0], [0.0, 0.0, -1.0]), 10.0)
            .expect("ray should hit the quad");
        assert!((hit.t - 1.0).abs() <= 0.0001);
        assert!(ray_mesh
            .trace(Ray::new([2.0, 2.0, 1.0], [0.0, 0.0, -1.0]), 10.0)
            .is_none());
    }

    #[test]
    fn meshes_without_position_attribute_are_not_gi_traceable() {
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
            "untyped_triangle",
        );

        assert!(mesh.ray_mesh().is_none());
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
