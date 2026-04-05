//! GPU mesh buffers for custom geometry rendering.

use std::borrow::Cow;

use wgpu::util::DeviceExt;

use crate::gpu::GpuContext;

/// Errors returned by fallible mesh APIs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MeshError {
    EmptyVertices,
    EmptyIndices,
}

impl std::fmt::Display for MeshError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyVertices => write!(f, "Mesh requires at least one vertex"),
            Self::EmptyIndices => write!(f, "Indexed mesh requires at least one index"),
        }
    }
}

impl std::error::Error for MeshError {}

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
pub struct Mesh {
    vertex_buffer: wgpu::Buffer,
    index_buffer: Option<wgpu::Buffer>,
    vertex_count: u32,
    index_count: u32,
    index_format: Option<wgpu::IndexFormat>,
    label: Cow<'static, str>,
}

impl std::fmt::Debug for Mesh {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Mesh")
            .field("vertex_count", &self.vertex_count)
            .field("index_count", &self.index_count)
            .field("index_format", &self.index_format)
            .field("label", &self.label)
            .finish_non_exhaustive()
    }
}

impl Mesh {
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

        let label = label.into();
        let vertex_buffer = ctx
            .device()
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(&format!("{}_vertices", label)),
                contents: bytemuck::cast_slice(vertices),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            });

        Ok(Self {
            vertex_buffer,
            index_buffer: None,
            vertex_count: vertices.len() as u32,
            index_count: 0,
            index_format: None,
            label,
        })
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

        let label = label.into();
        let vertex_buffer = ctx
            .device()
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(&format!("{}_vertices", label)),
                contents: bytemuck::cast_slice(vertices),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            });
        let index_buffer = ctx
            .device()
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(&format!("{}_indices", label)),
                contents: indices.bytes(),
                usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            });

        Ok(Self {
            vertex_buffer,
            index_buffer: Some(index_buffer),
            vertex_count: vertices.len() as u32,
            index_count: indices.count(),
            index_format: Some(indices.format()),
            label,
        })
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
    pub fn has_indices(&self) -> bool {
        self.index_buffer.is_some()
    }

    #[inline]
    pub fn label(&self) -> &str {
        &self.label
    }
}

#[cfg(test)]
mod tests {
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
}
