use std::fs;

use super::*;

fn create_test_device() -> (wgpu::Device, wgpu::Queue) {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        compatible_surface: None,
        force_fallback_adapter: false,
    }))
    .expect("No suitable GPU adapter found for mesh tests");

    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("mesh_test_device"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::default(),
        memory_hints: wgpu::MemoryHints::Performance,
        ..Default::default()
    }))
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
                VertexAttribute::new(VertexSemantic::Position, wgpu::VertexFormat::Float32x3, 0),
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
        while !buffer.len().is_multiple_of(align) {
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
