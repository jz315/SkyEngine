use super::*;
use crate::asset::{AssetConfig, Assets, TextureAsset};
use crate::ecs::World;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 3],
    uv: [f32; 2],
}

#[test]
fn mesh_asset_validates_vertex_payload_size() {
    let error = MeshAsset::try_from_raw(MeshAssetDescriptor::new(
        &[0, 1, 2, 3],
        1,
        MeshVertexLayout::position_uv(),
        "bad",
    ))
    .expect_err("payload is too small for the declared layout");

    assert_eq!(
        error,
        MeshAssetError::VertexDataSizeMismatch {
            bytes: 4,
            stride: 20,
            vertex_count: 1,
        }
    );
}

#[test]
fn render_assets_insert_and_resolve_runtime_assets() {
    let mut world = World::new();
    world.insert_resource(Assets::with_empty_manifest(AssetConfig::default()));
    let mut assets = RenderAssets::new(&mut world);

    let texture = assets.insert_texture(TextureAsset::white_pixel());
    let material = assets
        .insert_standard_material(StandardMaterialAsset::new().albedo_texture(texture.clone()));
    let mesh = assets.insert_mesh(MeshAsset::from_vertices(
        &[
            Vertex {
                position: [0.0, 0.0, 0.0],
                uv: [0.0, 0.0],
            },
            Vertex {
                position: [1.0, 0.0, 0.0],
                uv: [1.0, 0.0],
            },
            Vertex {
                position: [0.0, 1.0, 0.0],
                uv: [0.0, 1.0],
            },
        ],
        MeshVertexLayout::position_uv(),
        "triangle",
    ));

    assert!(assets.texture(texture).is_some());
    assert!(assets.standard_material(material).is_some());
    assert_eq!(
        assets
            .mesh(mesh)
            .expect("mesh should resolve")
            .vertex_count(),
        3
    );
}
