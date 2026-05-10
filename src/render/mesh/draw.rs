use std::ops::Range;
use std::sync::Arc;

use crate::render::resources::material::{MaterialPipelineCache, PreparedMaterial};
use crate::render::resources::mesh::Mesh;

use super::errors::MeshPassError;

/// One mesh draw submitted to [`super::MeshPass`].
pub struct MeshDraw<'a> {
    pub(super) mesh: &'a Mesh,
    pub(super) pipeline: &'a mut MaterialPipelineCache,
    pub(super) material: Option<&'a PreparedMaterial>,
    pub(super) vertex_range: Option<Range<u32>>,
    pub(super) index_range: Option<Range<u32>>,
    pub(super) base_vertex: i32,
    pub(super) instances: Range<u32>,
}

impl<'a> MeshDraw<'a> {
    pub fn new(mesh: &'a Mesh, pipeline: &'a mut MaterialPipelineCache) -> Self {
        Self {
            mesh,
            pipeline,
            material: None,
            vertex_range: None,
            index_range: None,
            base_vertex: 0,
            instances: 0..1,
        }
    }

    #[inline]
    pub fn material(mut self, material: &'a PreparedMaterial) -> Self {
        self.material = Some(material);
        self
    }

    #[inline]
    pub fn vertices(mut self, range: Range<u32>) -> Self {
        self.vertex_range = Some(range);
        self
    }

    #[inline]
    pub fn indices(mut self, range: Range<u32>) -> Self {
        self.index_range = Some(range);
        self
    }

    #[inline]
    pub fn base_vertex(mut self, base_vertex: i32) -> Self {
        self.base_vertex = base_vertex;
        self
    }

    #[inline]
    pub fn instances(mut self, range: Range<u32>) -> Self {
        self.instances = range;
        self
    }
}

pub(super) struct PreparedMeshDraw {
    pub(super) pipeline: Arc<wgpu::RenderPipeline>,
    pub(super) vertex_range: Range<u32>,
    pub(super) index_range: Option<Range<u32>>,
    pub(super) base_vertex: i32,
    pub(super) instances: Range<u32>,
}

pub(super) fn validate_instances(range: Range<u32>) -> Result<Range<u32>, MeshPassError> {
    if range.start > range.end {
        return Err(MeshPassError::InvalidInstanceRange {
            start: range.start,
            end: range.end,
        });
    }
    Ok(range)
}

pub(super) fn validate_vertex_range(
    mesh: &Mesh,
    range: Range<u32>,
) -> Result<Range<u32>, MeshPassError> {
    if range.start > range.end || range.end > mesh.vertex_count() {
        return Err(MeshPassError::VertexRangeOutOfBounds {
            mesh: mesh.label().to_string(),
            start: range.start,
            end: range.end,
            vertex_count: mesh.vertex_count(),
        });
    }
    Ok(range)
}

pub(super) fn validate_index_range(
    mesh: &Mesh,
    range: Range<u32>,
) -> Result<Range<u32>, MeshPassError> {
    if range.start > range.end || range.end > mesh.index_count() {
        return Err(MeshPassError::IndexRangeOutOfBounds {
            mesh: mesh.label().to_string(),
            start: range.start,
            end: range.end,
            index_count: mesh.index_count(),
        });
    }
    Ok(range)
}
