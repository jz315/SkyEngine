use super::*;
use crate::asset::{cook, Asset, AssetConfig, AssetId, AssetMeta, Assets, TextureAsset};
use crate::ecs::World;
use crate::render::resources::material::AlphaMode;

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

#[test]
fn standard_material_cooker_and_factory_load_runtime_asset(
) -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let source = dir.path().join("hero.skymaterial");
    std::fs::write(
        &source,
        r#"{
  "albedo": [0.25, 0.5, 0.75, 1.0],
  "metallic": 0.3,
  "roughness": 0.4,
  "emissive": [0.1, 0.2, 0.3, 1.0],
  "alpha_mode": "mask",
  "alpha_cutoff": 0.33,
  "receive_shadows": false
}"#,
    )?;

    let config = AssetConfig::new(dir.path(), "native");
    let registry = render_cook_registry();
    let manifest = cook::cook_all_with_registry(&config, &registry)?;
    let entry = manifest
        .assets
        .iter()
        .find(|entry| entry.asset_type == StandardMaterialAsset::TYPE)
        .expect("material asset should be cooked");

    let assets = Assets::new(config)?;
    register_render_asset_factories(&assets);
    let material = assets.load_blocking::<StandardMaterialAsset>(entry.asset_id)?;

    assert_eq!(material.albedo.r, 0.25);
    assert_eq!(material.albedo.g, 0.5);
    assert_eq!(material.albedo.b, 0.75);
    assert_eq!(material.metallic, 0.3);
    assert_eq!(material.roughness, 0.4);
    assert_eq!(material.emissive.r, 0.1);
    assert_eq!(material.alpha_mode, AlphaMode::Mask);
    assert_eq!(material.alpha_cutoff, 0.33);
    assert!(!material.receive_shadows);
    Ok(())
}

#[test]
fn standard_material_cooker_extracts_texture_dependencies() -> Result<(), Box<dyn std::error::Error>>
{
    let dir = tempfile::tempdir()?;
    let texture = dir.path().join("albedo.png");
    image::save_buffer(&texture, &[255, 32, 16, 255], 1, 1, image::ColorType::Rgba8)?;
    let source = dir.path().join("textured.skymaterial");
    std::fs::write(
        &source,
        r#"{
  "albedo": [1.0, 1.0, 1.0, 1.0],
  "albedo_texture": "albedo.png",
  "metallic": 0.1,
  "roughness": 0.7
}"#,
    )?;

    let config = AssetConfig::new(dir.path(), "native");
    let registry = render_cook_registry();
    let manifest = cook::cook_all_with_registry(&config, &registry)?;
    let material_entry = manifest
        .assets
        .iter()
        .find(|entry| entry.asset_type == StandardMaterialAsset::TYPE)
        .expect("material asset should be cooked");
    let texture_entry = manifest
        .assets
        .iter()
        .find(|entry| entry.asset_type == TextureAsset::TYPE)
        .expect("texture dependency should be cooked");

    assert_eq!(material_entry.dependencies, vec![texture_entry.asset_id]);

    let cooked = std::fs::read_to_string(config.cooked_root().join(&material_entry.cooked_path))?;
    assert!(cooked.contains(&texture_entry.asset_id.to_string()));
    assert!(!cooked.contains("albedo.png"));

    let assets = Assets::new(config)?;
    register_render_asset_factories(&assets);
    let material = assets.load_blocking::<StandardMaterialAsset>(material_entry.asset_id)?;

    assert_eq!(material.albedo.r, 1.0);
    let albedo_texture = material
        .albedo_texture
        .as_ref()
        .expect("material install should bind an albedo texture handle");
    assert_eq!(albedo_texture.id(), texture_entry.asset_id);
    assert!(albedo_texture.is_ready());
    assert_eq!(albedo_texture.get()?.width(), 1);
    assert!(
        assets
            .try_get_id::<TextureAsset>(texture_entry.asset_id)
            .is_some(),
        "loading the material should install its declared texture dependency"
    );
    Ok(())
}

#[test]
fn mesh_cooker_and_factory_load_runtime_asset() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let source = dir.path().join("triangle.skymesh");
    let vertices = [
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
    ];
    std::fs::write(
        &source,
        serde_json::to_vec_pretty(&serde_json::json!({
            "label": "triangle",
            "vertex_bytes": bytemuck::cast_slice::<Vertex, u8>(&vertices),
            "vertex_count": 3,
            "vertex_layout": {
                "stride": 20,
                "attributes": [
                    { "semantic": "position", "format": "float32x3", "offset": 0 },
                    { "semantic": "uv0", "format": "float32x2", "offset": 12 }
                ]
            },
            "indices": { "format": "u16", "data": [0, 1, 2] },
            "sub_meshes": [
                {
                    "index_offset": 0,
                    "index_count": 3,
                    "material_index": 0,
                    "bounding_sphere": { "center": [0.5, 0.5, 0.0], "radius": 0.75 }
                }
            ],
            "bounding_sphere": { "center": [0.5, 0.5, 0.0], "radius": 0.75 }
        }))?,
    )?;

    let config = AssetConfig::new(dir.path(), "native");
    let registry = render_cook_registry();
    let manifest = cook::cook_all_with_registry(&config, &registry)?;
    let entry = manifest
        .assets
        .iter()
        .find(|entry| entry.asset_type == MeshAsset::TYPE)
        .expect("mesh asset should be cooked");

    let assets = Assets::new(config)?;
    register_render_asset_factories(&assets);
    let mesh = assets.load_blocking::<MeshAsset>(entry.asset_id)?;

    assert_eq!(mesh.label(), "triangle");
    assert_eq!(mesh.vertex_count(), 3);
    assert_eq!(mesh.vertex_layout().stride(), 20);
    assert_eq!(mesh.vertex_layout().attributes().len(), 2);
    assert_eq!(mesh.indices().expect("indices should load").count(), 3);
    assert_eq!(mesh.sub_meshes().len(), 1);
    assert_eq!(mesh.sub_meshes()[0].index_count, 3);
    assert_eq!(mesh.bounding_sphere().center, [0.5, 0.5, 0.0]);
    Ok(())
}

#[test]
fn gltf_mesh_cooker_and_factory_load_runtime_asset() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let source = dir.path().join("triangle.gltf");
    let buffer_path = dir.path().join("triangle.bin");

    let positions = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
    let normals = [[0.0, 0.0, 1.0]; 3];
    let uvs = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]];
    let indices = [0u16, 1, 2];

    let mut buffer = Vec::new();
    let positions_offset = buffer.len();
    append_f32x3(&mut buffer, &positions);
    let normals_offset = buffer.len();
    append_f32x3(&mut buffer, &normals);
    let uvs_offset = buffer.len();
    append_f32x2(&mut buffer, &uvs);
    let indices_offset = buffer.len();
    append_u16(&mut buffer, &indices);
    std::fs::write(&buffer_path, &buffer)?;

    std::fs::write(
        &source,
        serde_json::to_vec_pretty(&serde_json::json!({
            "asset": { "version": "2.0" },
            "buffers": [
                { "uri": "triangle.bin", "byteLength": buffer.len() }
            ],
            "bufferViews": [
                { "buffer": 0, "byteOffset": positions_offset, "byteLength": 36 },
                { "buffer": 0, "byteOffset": normals_offset, "byteLength": 36 },
                { "buffer": 0, "byteOffset": uvs_offset, "byteLength": 24 },
                { "buffer": 0, "byteOffset": indices_offset, "byteLength": 6 }
            ],
            "accessors": [
                {
                    "bufferView": 0,
                    "componentType": 5126,
                    "count": 3,
                    "type": "VEC3",
                    "min": [0.0, 0.0, 0.0],
                    "max": [1.0, 1.0, 0.0]
                },
                { "bufferView": 1, "componentType": 5126, "count": 3, "type": "VEC3" },
                { "bufferView": 2, "componentType": 5126, "count": 3, "type": "VEC2" },
                { "bufferView": 3, "componentType": 5123, "count": 3, "type": "SCALAR" }
            ],
            "meshes": [
                {
                    "name": "triangle",
                    "primitives": [
                        {
                            "attributes": {
                                "POSITION": 0,
                                "NORMAL": 1,
                                "TEXCOORD_0": 2
                            },
                            "indices": 3,
                            "mode": 4
                        }
                    ]
                }
            ],
            "nodes": [{ "mesh": 0 }],
            "scenes": [{ "nodes": [0] }],
            "scene": 0
        }))?,
    )?;

    let config = AssetConfig::new(dir.path(), "native");
    let registry = render_cook_registry();
    let manifest = cook::cook_all_with_registry(&config, &registry)?;
    let entry = manifest
        .assets
        .iter()
        .find(|entry| entry.source_path == "triangle.gltf")
        .expect("glTF mesh should be cooked");

    assert_eq!(entry.asset_type, MeshAsset::TYPE);
    assert_eq!(entry.importer, "render.mesh");
    assert_eq!(entry.cooker, "render.mesh_json");
    assert!(entry.cooked_path.ends_with(".skymesh"));

    let assets = Assets::new(config)?;
    register_render_asset_factories(&assets);
    let mesh = assets.load_blocking::<MeshAsset>(entry.asset_id)?;

    assert_eq!(mesh.label(), "triangle");
    assert_eq!(mesh.vertex_count(), 3);
    assert_eq!(mesh.vertex_layout().stride(), 32);
    assert_eq!(mesh.vertex_layout().attributes().len(), 3);
    assert_eq!(mesh.indices().expect("indices should load").count(), 3);
    assert_eq!(mesh.sub_meshes().len(), 1);
    assert_eq!(mesh.sub_meshes()[0].index_count, 3);
    assert_eq!(mesh.bounding_sphere().center, [0.5, 0.5, 0.0]);
    Ok(())
}

#[test]
fn gltf_material_cooker_extracts_texture_dependencies() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let texture = dir.path().join("albedo.png");
    image::save_buffer(
        &texture,
        &[12, 128, 240, 255],
        1,
        1,
        image::ColorType::Rgba8,
    )?;
    let source = dir.path().join("material.gltf");
    std::fs::write(
        &source,
        serde_json::to_vec_pretty(&serde_json::json!({
            "asset": { "version": "2.0" },
            "images": [
                { "uri": "albedo.png" }
            ],
            "textures": [
                { "source": 0 }
            ],
            "materials": [
                {
                    "pbrMetallicRoughness": {
                        "baseColorFactor": [0.25, 0.5, 0.75, 0.8],
                        "baseColorTexture": { "index": 0 },
                        "metallicFactor": 0.2,
                        "roughnessFactor": 0.6
                    },
                    "emissiveFactor": [0.1, 0.2, 0.3],
                    "alphaMode": "BLEND"
                }
            ]
        }))?,
    )?;
    let material_id = AssetId::new();
    std::fs::write(
        source.with_file_name("material.gltf.meta"),
        serde_json::to_vec_pretty(&AssetMeta {
            asset_id: material_id,
            asset_type: StandardMaterialAsset::TYPE.to_string(),
            importer: "render.standard_material_json".to_string(),
            cooker: "render.standard_material_json".to_string(),
            version: 1,
            source_path: "material.gltf".to_string(),
            source_hash: None,
            meta_hash: None,
            cooked_hash: None,
            dependencies: Vec::new(),
            import_settings: serde_json::json!({
                "asset_type": StandardMaterialAsset::TYPE,
                "material_index": 0
            }),
        })?,
    )?;

    let config = AssetConfig::new(dir.path(), "native");
    let registry = render_cook_registry();
    let manifest = cook::cook_all_with_registry(&config, &registry)?;
    let material_entry = manifest
        .assets
        .iter()
        .find(|entry| entry.asset_id == material_id)
        .expect("glTF material should be cooked");
    let texture_entry = manifest
        .assets
        .iter()
        .find(|entry| entry.asset_type == TextureAsset::TYPE)
        .expect("glTF material texture should be cooked");

    assert_eq!(material_entry.asset_type, StandardMaterialAsset::TYPE);
    assert_eq!(material_entry.source_path, "material.gltf");
    assert_eq!(material_entry.dependencies, vec![texture_entry.asset_id]);
    assert!(material_entry.cooked_path.ends_with(".skymaterial"));

    let cooked = std::fs::read_to_string(config.cooked_root().join(&material_entry.cooked_path))?;
    assert!(cooked.contains(&texture_entry.asset_id.to_string()));
    assert!(!cooked.contains("albedo.png"));

    let assets = Assets::new(config)?;
    register_render_asset_factories(&assets);
    let material = assets.load_blocking::<StandardMaterialAsset>(material_entry.asset_id)?;

    assert_eq!(material.albedo.r, 0.25);
    assert_eq!(material.albedo.g, 0.5);
    assert_eq!(material.albedo.b, 0.75);
    assert_eq!(material.albedo.a, 0.8);
    assert_eq!(material.metallic, 0.2);
    assert_eq!(material.roughness, 0.6);
    assert_eq!(material.emissive.r, 0.1);
    assert_eq!(material.alpha_mode, AlphaMode::Blend);
    let albedo_texture = material
        .albedo_texture
        .as_ref()
        .expect("glTF material install should bind albedo texture");
    assert_eq!(albedo_texture.id(), texture_entry.asset_id);
    assert!(albedo_texture.is_ready());
    Ok(())
}

fn append_f32x2(bytes: &mut Vec<u8>, values: &[[f32; 2]]) {
    for value in values {
        for component in value {
            bytes.extend_from_slice(&component.to_le_bytes());
        }
    }
}

fn append_f32x3(bytes: &mut Vec<u8>, values: &[[f32; 3]]) {
    for value in values {
        for component in value {
            bytes.extend_from_slice(&component.to_le_bytes());
        }
    }
}

fn append_u16(bytes: &mut Vec<u8>, values: &[u16]) {
    for value in values {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
}
