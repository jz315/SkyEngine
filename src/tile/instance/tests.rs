use std::path::Path;

use crate::asset::{AssetConfig, AssetId, AssetServer, Handle, TextureAsset};
use crate::ecs::World;
use crate::render::{TiledImport, TilemapRenderer, TilemapStorage};
use crate::tile::adapters::tiled::TiledImporter;
use crate::tile::{
    CellCoord, GridSpec, LayerId, LayerRole, ObjectVisual, PaletteId, RectU, SceneTile, TileDef,
    TileDefId, TileLayer, TileMap, TileMapDocument, TileMapEditSession, TileMapId, TileMapSize,
    TileObject, TilePaletteStore, TileRef, TileTextureSource,
};

use super::*;

#[test]
fn spawns_tiled_scene_through_tile_scene_runtime() {
    let mut world = World::new();
    world.insert_resource(AssetServer::with_empty_manifest(AssetConfig::default()));
    let import =
        TiledImport::from_tmx_file("examples/assets/tiled/tiled/examples/forest/forest.tmx")
            .expect("forest map imports");
    let scene = TiledImporter::import_scene(&import);
    let palettes = import_palettes(&import);

    let instance = TileMapInstance::spawn(
        &mut world,
        &scene,
        &palettes,
        TileMapSpawnOptions::default(),
    )
    .expect("tile scene instance should spawn");

    assert!(!instance.entities.is_empty());
    assert!(world.get_resource::<TilemapStorage>().is_some());
    assert!(instance
        .entities
        .iter()
        .any(|entity| world.get::<TilemapRenderer>(*entity).is_some()));
    instance.despawn(&mut world);
}

#[test]
fn spawns_tiled_image_collection_scene_with_runtime_atlas() {
    let temp = tempfile::tempdir().expect("tempdir");
    write_solid_png(&temp.path().join("red.png"), 2, 2, [255, 0, 0, 255]);
    write_solid_png(&temp.path().join("green.png"), 3, 1, [0, 255, 0, 255]);
    std::fs::write(
        temp.path().join("collection.tsx"),
        r#"<?xml version="1.0" encoding="UTF-8"?>
        <tileset name="collection" tilewidth="3" tileheight="2" tilecount="2" columns="0">
            <tile id="0" width="2" height="2">
                <image width="2" height="2" source="red.png"/>
            </tile>
            <tile id="1" width="3" height="1">
                <image width="3" height="1" source="green.png"/>
            </tile>
        </tileset>"#,
    )
    .expect("write tsx");
    let tmx = r#"<?xml version="1.0" encoding="UTF-8"?>
    <map version="1.10" orientation="orthogonal" renderorder="right-down" width="2" height="1" tilewidth="3" tileheight="2">
        <tileset firstgid="1" source="collection.tsx"/>
        <layer name="Ground" width="2" height="1">
            <data><tile gid="1"/><tile gid="2"/></data>
        </layer>
    </map>"#;

    let mut world = World::new();
    world.insert_resource(AssetServer::with_empty_manifest(AssetConfig::default()));
    let import =
        TiledImport::from_tmx_str(tmx, temp.path()).expect("multi-image collection import");
    let scene = TiledImporter::import_scene(&import);
    let palettes = import_palettes(&import);

    let instance = TileMapInstance::spawn(
        &mut world,
        &scene,
        &palettes,
        TileMapSpawnOptions::default(),
    )
    .expect("tile scene instance should spawn");

    assert_eq!(instance.textures.len(), 1);
    assert_eq!(instance.layers[0].renderer.tileset.texture_size, [6, 2]);
    assert_eq!(
        instance.layers[0]
            .renderer
            .tileset
            .tile_draw_size(crate::render::TileId(1)),
        Some([3, 1])
    );
    let uv = instance.layers[0]
        .renderer
        .tileset
        .uv_rect(crate::render::TileId(1))
        .expect("second tile uv");
    for (actual, expected) in uv.into_iter().zip([0.5, 0.0, 1.0, 0.5]) {
        assert!((actual - expected).abs() <= 1e-6);
    }
    instance.despawn(&mut world);
}

#[test]
fn refresh_rebuilds_scene_instance_after_scene_edit() {
    let mut world = World::new();
    world.insert_resource(AssetServer::with_empty_manifest(AssetConfig::default()));
    let import =
        TiledImport::from_tmx_file("examples/assets/tiled/tiled/examples/forest/forest.tmx")
            .expect("forest map imports");
    let mut scene = TiledImporter::import_scene(&import);
    let palettes = import_palettes(&import);
    let mut instance = TileMapInstance::spawn(
        &mut world,
        &scene,
        &palettes,
        TileMapSpawnOptions::default(),
    )
    .expect("tile scene instance should spawn");
    let old_map = instance.scene;

    scene.layers[0].visible = false;
    instance
        .refresh(&mut world, &scene)
        .expect("refresh should rebuild render map");

    assert_ne!(instance.scene, old_map);
    assert!(world
        .get_resource::<TilemapStorage>()
        .expect("storage")
        .get(old_map)
        .is_none());
    let edited_layer = scene.layers[0].id;
    let edited_entity = instance
        .layers
        .iter()
        .position(|layer| layer.source_layer == edited_layer)
        .and_then(|index| instance.entities.get(index).copied())
        .expect("edited layer renderer entity");
    assert!(world
        .get::<TilemapRenderer>(edited_entity)
        .is_some_and(|renderer| !renderer.visible));
    instance.despawn(&mut world);
}

#[test]
fn refresh_edit_summary_rewrites_dirty_tile_cells() {
    let mut world = World::new();
    world.insert_resource(AssetServer::with_empty_manifest(AssetConfig::default()));

    let layer = LayerId(7);
    let palette_id = PaletteId(3);
    let mut palette = super::super::palette::TilePalette::new(palette_id, "props");
    palette.texture = TileTextureSource::Texture {
        handle: Handle::<TextureAsset>::new(AssetId::new()),
        size: [32, 16],
    };
    palette
        .tiles
        .push(TileDef::new(TileDefId(0), RectU::new(0, 0, 16, 16)));
    palette
        .tiles
        .push(TileDef::new(TileDefId(1), RectU::new(16, 0, 16, 16)));
    let mut palettes = TilePaletteStore::new();
    palettes.insert(palette);

    let mut scene = TileMap::new(
        TileMapId(1),
        "edit",
        GridSpec::orthogonal([16, 16]),
        TileMapSize::new(3, 1),
    );
    let mut tile_layer = TileLayer::tiles(layer, "Ground", LayerRole::Ground);
    tile_layer.set_tile(
        CellCoord::new(0, 0),
        Some(SceneTile::new(TileRef::new(palette_id, TileDefId(0)))),
    );
    scene.layers.push(tile_layer);

    let mut instance = TileMapInstance::spawn(
        &mut world,
        &scene,
        &palettes,
        TileMapSpawnOptions::default(),
    )
    .expect("scene should spawn");
    let original_map = instance.scene;

    let summary = {
        let mut session = TileMapEditSession::new(&mut scene);
        session.set_tile(
            layer,
            CellCoord::new(0, 0),
            Some(SceneTile::new(TileRef::new(palette_id, TileDefId(1)))),
        );
        session.finish()
    };

    instance
        .refresh_edit_summary(&mut world, &scene, &summary)
        .expect("dirty cell should refresh");

    assert_eq!(instance.scene, original_map);
    let storage = world.get_resource::<TilemapStorage>().expect("storage");
    let map = storage.get(instance.scene).expect("map");
    assert_eq!(
        map.tile(0, 0, 0).expect("edited cell").id,
        crate::render::TileId(1)
    );
    instance.despawn(&mut world);
}

#[test]
fn document_edit_summary_refreshes_scene_instance() {
    let mut world = World::new();
    world.insert_resource(AssetServer::with_empty_manifest(AssetConfig::default()));

    let layer = LayerId(7);
    let palette_id = PaletteId(3);
    let mut palettes = TilePaletteStore::new();
    palettes.insert(test_palette(palette_id, [16, 16]));

    let mut scene = TileMap::new(
        TileMapId(1),
        "document",
        GridSpec::orthogonal([16, 16]),
        TileMapSize::new(2, 1),
    );
    let mut tile_layer = TileLayer::tiles(layer, "Ground", LayerRole::Ground);
    tile_layer.set_tile(
        CellCoord::new(0, 0),
        Some(SceneTile::new(TileRef::new(palette_id, TileDefId(0)))),
    );
    scene.layers.push(tile_layer);
    let mut document = TileMapDocument::new(scene).with_palette_store(palettes);

    let mut instance =
        TileMapInstance::spawn_document(&mut world, &document, TileMapSpawnOptions::default())
            .expect("document should spawn");
    let original_map = instance.scene;

    let summary = document.edit_recorded(|edit| {
        edit.set_tile(
            layer,
            CellCoord::new(1, 0),
            Some(SceneTile::new(TileRef::new(palette_id, TileDefId(0)))),
        );
    });
    instance
        .refresh_document_edit_summary(&mut world, &document, &summary)
        .expect("document summary should refresh");

    assert_eq!(instance.scene, original_map);
    let storage = world.get_resource::<TilemapStorage>().expect("storage");
    let map = storage.get(instance.scene).expect("map");
    assert_eq!(
        map.tile(0, 1, 0).expect("edited cell").id,
        crate::render::TileId(0)
    );

    let undo_summary = document.undo().expect("undo summary");
    instance
        .refresh_document_edit_summary(&mut world, &document, &undo_summary)
        .expect("undo summary should refresh");
    let storage = world.get_resource::<TilemapStorage>().expect("storage");
    let map = storage.get(instance.scene).expect("map");
    assert!(map.tile(0, 1, 0).expect("undone cell").is_empty());
    instance.despawn(&mut world);
}

#[test]
fn refresh_edit_summary_rebuilds_when_tile_palette_splits_change() {
    let mut world = World::new();
    world.insert_resource(AssetServer::with_empty_manifest(AssetConfig::default()));

    let layer = LayerId(7);
    let palette_a = PaletteId(3);
    let palette_b = PaletteId(4);
    let mut palettes = TilePaletteStore::new();
    palettes.insert(test_palette(palette_a, [16, 16]));
    palettes.insert(test_palette(palette_b, [16, 16]));

    let mut scene = TileMap::new(
        TileMapId(1),
        "edit",
        GridSpec::orthogonal([16, 16]),
        TileMapSize::new(2, 1),
    );
    let mut tile_layer = TileLayer::tiles(layer, "Ground", LayerRole::Ground);
    tile_layer.set_tile(
        CellCoord::new(0, 0),
        Some(SceneTile::new(TileRef::new(palette_a, TileDefId(0)))),
    );
    scene.layers.push(tile_layer);

    let mut instance = TileMapInstance::spawn(
        &mut world,
        &scene,
        &palettes,
        TileMapSpawnOptions::default(),
    )
    .expect("scene should spawn");
    let original_map = instance.scene;

    let summary = {
        let mut session = TileMapEditSession::new(&mut scene);
        session.set_tile(
            layer,
            CellCoord::new(0, 0),
            Some(SceneTile::new(TileRef::new(palette_b, TileDefId(0)))),
        );
        session.finish()
    };

    instance
        .refresh_edit_summary(&mut world, &scene, &summary)
        .expect("palette split change should rebuild");

    assert_ne!(instance.scene, original_map);
    assert_eq!(instance.layers.len(), 1);
    assert_eq!(instance.layers[0].palette, palette_b);
    assert!(world
        .get_resource::<TilemapStorage>()
        .expect("storage")
        .get(original_map)
        .is_none());
    instance.despawn(&mut world);
}

#[test]
fn refresh_edit_summary_rewrites_object_layers() {
    let mut world = World::new();
    world.insert_resource(AssetServer::with_empty_manifest(AssetConfig::default()));

    let layer = LayerId(7);
    let palette_id = PaletteId(3);
    let mut palette = super::super::palette::TilePalette::new(palette_id, "props");
    palette.texture = TileTextureSource::Texture {
        handle: Handle::<TextureAsset>::new(AssetId::new()),
        size: [32, 16],
    };
    palette
        .tiles
        .push(TileDef::new(TileDefId(0), RectU::new(0, 0, 16, 16)));
    palette
        .tiles
        .push(TileDef::new(TileDefId(1), RectU::new(16, 0, 16, 16)));
    let mut palettes = TilePaletteStore::new();
    palettes.insert(palette);

    let mut scene = TileMap::new(
        TileMapId(1),
        "edit",
        GridSpec::orthogonal([16, 16]),
        TileMapSize::new(3, 1),
    );
    scene
        .layers
        .push(TileLayer::objects(layer, "Props", LayerRole::Props));
    let initial_object = scene_object(layer, CellCoord::new(0, 0), palette_id, TileDefId(0));
    {
        let mut session = TileMapEditSession::new(&mut scene);
        session.place_object(layer, initial_object);
    }

    let mut instance = TileMapInstance::spawn(
        &mut world,
        &scene,
        &palettes,
        TileMapSpawnOptions::default(),
    )
    .expect("scene should spawn");
    let original_map = instance.scene;

    let summary = {
        let existing = scene.objects.iter().next().expect("initial object").id;
        let mut session = TileMapEditSession::new(&mut scene);
        session.remove_object(existing);
        session.place_object(
            layer,
            scene_object(layer, CellCoord::new(1, 0), palette_id, TileDefId(1)),
        );
        session.finish()
    };

    instance
        .refresh_edit_summary(&mut world, &scene, &summary)
        .expect("changed layer should refresh");

    assert_eq!(instance.scene, original_map);
    let storage = world.get_resource::<TilemapStorage>().expect("storage");
    let map = storage.get(instance.scene).expect("map");
    assert!(map.tile(0, 0, 0).expect("old cell").is_empty());
    assert_eq!(
        map.tile(0, 1, 0).expect("new cell").id,
        crate::render::TileId(1)
    );
    instance.despawn(&mut world);
}

#[test]
fn refresh_edit_summary_rewrites_moved_objects() {
    let mut world = World::new();
    world.insert_resource(AssetServer::with_empty_manifest(AssetConfig::default()));

    let layer = LayerId(7);
    let palette_id = PaletteId(3);
    let mut palettes = TilePaletteStore::new();
    palettes.insert(test_palette(palette_id, [16, 16]));

    let mut scene = TileMap::new(
        TileMapId(1),
        "edit",
        GridSpec::orthogonal([16, 16]),
        TileMapSize::new(3, 1),
    );
    scene
        .layers
        .push(TileLayer::objects(layer, "Props", LayerRole::Props));
    let object_id = {
        let mut session = TileMapEditSession::new(&mut scene);
        session.place_object(
            layer,
            scene_object(layer, CellCoord::new(0, 0), palette_id, TileDefId(0)),
        )
    };

    let mut instance = TileMapInstance::spawn(
        &mut world,
        &scene,
        &palettes,
        TileMapSpawnOptions::default(),
    )
    .expect("scene should spawn");
    let original_map = instance.scene;

    let summary = {
        let mut session = TileMapEditSession::new(&mut scene);
        session.move_object(object_id, layer, CellCoord::new(2, 0));
        session.finish()
    };

    instance
        .refresh_edit_summary(&mut world, &scene, &summary)
        .expect("moved object should refresh");

    assert_eq!(instance.scene, original_map);
    let storage = world.get_resource::<TilemapStorage>().expect("storage");
    let map = storage.get(instance.scene).expect("map");
    assert!(map.tile(0, 0, 0).expect("old cell").is_empty());
    assert_eq!(
        map.tile(0, 2, 0).expect("new cell").id,
        crate::render::TileId(0)
    );
    instance.despawn(&mut world);
}

fn import_palettes(import: &TiledImport) -> TilePaletteStore {
    let mut palettes = TilePaletteStore::new();
    for palette in TiledImporter::import_palettes(import) {
        palettes.insert(palette);
    }
    palettes
}

fn scene_object(
    layer: LayerId,
    cell: CellCoord,
    palette: PaletteId,
    tile: TileDefId,
) -> TileObject {
    let mut object = TileObject::new(crate::tile::TileObjectId(0), layer, cell);
    object.visual = ObjectVisual::Tile(TileRef::new(palette, tile));
    object
}

fn test_palette(id: PaletteId, tile_size: [u32; 2]) -> super::super::palette::TilePalette {
    let mut palette = super::super::palette::TilePalette::new(id, "test");
    palette.texture = TileTextureSource::Texture {
        handle: Handle::<TextureAsset>::new(AssetId::new()),
        size: tile_size,
    };
    palette.tiles.push(TileDef::new(
        TileDefId(0),
        RectU::new(0, 0, tile_size[0], tile_size[1]),
    ));
    palette
}

fn write_solid_png(path: &Path, width: u32, height: u32, color: [u8; 4]) {
    let mut image = image::RgbaImage::new(width, height);
    for pixel in image.pixels_mut() {
        *pixel = image::Rgba(color);
    }
    image.save(path).expect("write fixture png");
}
