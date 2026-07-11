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
