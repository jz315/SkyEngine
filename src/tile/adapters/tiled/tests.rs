use std::path::Path;

use crate::render::TiledImport;
use crate::tile::{
    CellCoord, LayerRole, ObjectVisual, PaletteId, PropertyValue, RectU, TileDefId, TileObjectId,
    TileRef, TileTextureSource,
};

use super::*;

#[test]
fn tiled_importer_converts_import_to_scene_and_palettes() {
    let json = r#"{
        "orientation": "orthogonal",
        "width": 2,
        "height": 1,
        "tilewidth": 16,
        "tileheight": 16,
        "layers": [{
            "name": "Ground",
            "type": "tilelayer",
            "width": 2,
            "height": 1,
            "data": [1, 0]
        }],
        "tilesets": [{
            "firstgid": 1,
            "image": "tiles.png",
            "tilewidth": 16,
            "tileheight": 16,
            "columns": 1,
            "tilecount": 1
        }]
    }"#;
    let import = TiledImport::from_json_str(json, Path::new("assets")).expect("import");

    let scene = TiledImporter::import_scene(&import);
    let palettes = TiledImporter::import_palettes(&import);

    assert_eq!(scene.size.width, 2);
    assert_eq!(scene.layers.len(), 1);
    assert_eq!(scene.layers[0].role, LayerRole::Ground);
    assert_eq!(
        scene.layers[0]
            .tile(CellCoord::new(0, 0))
            .expect("scene tile")
            .tile_ref,
        TileRef::new(PaletteId(1), TileDefId(0))
    );
    assert!(scene.layers[0].tile(CellCoord::new(1, 0)).is_none());
    assert_eq!(palettes.len(), 1);
    assert_eq!(palettes[0].tiles[0].source_rect, RectU::new(0, 0, 16, 16));
    match &palettes[0].texture {
        TileTextureSource::Image(path) => assert!(path.ends_with("tiles.png")),
        other => panic!("unexpected texture source: {other:?}"),
    }
}

#[test]
fn tiled_importer_preserves_tile_object_visuals() {
    let json = r#"{
        "orientation": "orthogonal",
        "width": 1,
        "height": 1,
        "tilewidth": 16,
        "tileheight": 16,
        "layers": [{
            "name": "Props",
            "type": "objectgroup",
            "objects": [{
                "id": 9,
                "gid": 1,
                "x": 0,
                "y": 16,
                "width": 16,
                "height": 16
            }]
        }],
        "tilesets": [{
            "firstgid": 1,
            "image": "tiles.png",
            "tilewidth": 16,
            "tileheight": 16,
            "columns": 1,
            "tilecount": 1
        }]
    }"#;
    let import = TiledImport::from_json_str(json, Path::new("assets")).expect("import");

    let scene = TiledImporter::import_scene(&import);
    let object = scene.objects.get(TileObjectId(9)).expect("object");

    assert_eq!(scene.layers.len(), 1);
    assert!(matches!(
        object.visual,
        ObjectVisual::Tile(TileRef {
            palette: PaletteId(1),
            tile: TileDefId(0)
        })
    ));
}

#[test]
fn tiled_importer_preserves_image_collection_atlas_sources() {
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
    let import =
        TiledImport::from_tmx_str(tmx, temp.path()).expect("multi-image collection import");

    let scene = TiledImporter::import_scene(&import);
    let palettes = TiledImporter::import_palettes(&import);

    assert_eq!(
        scene.layers[0]
            .tile(CellCoord::new(1, 0))
            .expect("tile")
            .tile_ref,
        TileRef::new(PaletteId(1), TileDefId(1))
    );
    assert_eq!(palettes[0].tiles[0].source_rect, RectU::new(0, 0, 2, 2));
    assert_eq!(palettes[0].tiles[1].source_rect, RectU::new(3, 0, 3, 1));
    match &palettes[0].texture {
        TileTextureSource::ImageCollectionAtlas { size, tiles } => {
            assert_eq!(*size, [6, 2]);
            assert_eq!(tiles.len(), 2);
            assert_eq!(tiles[0].source_rect, RectU::new(0, 0, 2, 2));
            assert_eq!(tiles[1].source_rect, RectU::new(0, 0, 3, 1));
        }
        other => panic!("unexpected texture source: {other:?}"),
    }
}

#[test]
fn tiled_exporter_round_trips_scene_subset_without_import_shims() {
    let json = r#"{
        "orientation": "orthogonal",
        "width": 2,
        "height": 1,
        "tilewidth": 16,
        "tileheight": 16,
        "properties": [{ "name": "difficulty", "type": "int", "value": 2 }],
        "layers": [{
            "name": "Ground",
            "type": "tilelayer",
            "width": 2,
            "height": 1,
            "data": [1, 0],
            "properties": [{ "name": "walkable", "type": "bool", "value": true }]
        }, {
            "name": "Props",
            "type": "objectgroup",
            "objects": [{
                "id": 9,
                "gid": 1,
                "x": 0,
                "y": 16,
                "width": 16,
                "height": 16,
                "properties": [{ "name": "kind", "type": "string", "value": "crate" }]
            }]
        }],
        "tilesets": [{
            "firstgid": 1,
            "name": "terrain",
            "image": "tiles.png",
            "imagewidth": 16,
            "imageheight": 16,
            "tilewidth": 16,
            "tileheight": 16,
            "columns": 1,
            "tilecount": 1,
            "properties": [{ "name": "source", "type": "string", "value": "fixture" }]
        }]
    }"#;
    let import = TiledImport::from_json_str(json, Path::new("assets")).expect("import");
    let scene = TiledImporter::import_scene(&import);
    let palettes = TiledImporter::import_palettes(&import);

    let exported = TiledExporter::to_tmj_string(&scene, &palettes).expect("export tmj");
    let reimported = TiledImport::from_json_str(&exported, Path::new(".")).expect("reimport tmj");
    let reimported_scene = TiledImporter::import_scene(&reimported);
    let reimported_palettes = TiledImporter::import_palettes(&reimported);

    assert_eq!(
        reimported_scene.properties.get("difficulty"),
        Some(&PropertyValue::Int(2))
    );
    assert_eq!(
        reimported_scene.layers[0]
            .tile(CellCoord::new(0, 0))
            .expect("tile")
            .tile_ref,
        TileRef::new(PaletteId(1), TileDefId(0))
    );
    assert!(reimported_scene.layers[0]
        .tile(CellCoord::new(1, 0))
        .is_none());
    assert_eq!(
        reimported_scene.layers[0].properties.get("walkable"),
        Some(&PropertyValue::Bool(true))
    );
    assert!(matches!(
        reimported_scene
            .objects
            .get(TileObjectId(9))
            .expect("object")
            .visual,
        ObjectVisual::Tile(TileRef {
            palette: PaletteId(1),
            tile: TileDefId(0)
        })
    ));
    assert_eq!(
        reimported_scene
            .objects
            .get(TileObjectId(9))
            .expect("object")
            .properties
            .get("kind"),
        Some(&PropertyValue::String("crate".to_string()))
    );
    assert_eq!(
        reimported_palettes[0].properties.get("source"),
        Some(&PropertyValue::String("fixture".to_string()))
    );
}

#[test]
fn tiled_document_api_loads_and_exports_complete_scene_data() {
    let json = r#"{
        "orientation": "orthogonal",
        "width": 1,
        "height": 1,
        "tilewidth": 16,
        "tileheight": 16,
        "layers": [{
            "name": "Ground",
            "type": "tilelayer",
            "width": 1,
            "height": 1,
            "data": [1]
        }],
        "tilesets": [{
            "firstgid": 1,
            "name": "terrain",
            "image": "tiles.png",
            "imagewidth": 16,
            "imageheight": 16,
            "tilewidth": 16,
            "tileheight": 16,
            "columns": 1,
            "tilecount": 1
        }]
    }"#;
    let import = TiledImport::from_json_str(json, Path::new("assets")).expect("import");

    let document = TiledImporter::import_document(&import);
    let exported = TiledExporter::document_to_tmj_string(&document).expect("export document");
    let reimported = TiledImport::from_json_str(&exported, Path::new(".")).expect("reimport");
    let reimported_document = TiledImporter::import_document(&reimported);

    assert_eq!(document.scene.size.width, 1);
    assert_eq!(document.palettes.len(), 1);
    assert_eq!(
        document.scene.layers[0]
            .tile(CellCoord::new(0, 0))
            .expect("tile")
            .tile_ref,
        TileRef::new(PaletteId(1), TileDefId(0))
    );
    assert_eq!(reimported_document.palettes.len(), 1);
    assert_eq!(
        reimported_document.scene.layers[0]
            .tile(CellCoord::new(0, 0))
            .expect("tile")
            .tile_ref,
        TileRef::new(PaletteId(1), TileDefId(0))
    );
}

#[test]
fn tiled_exporter_round_trips_image_collection_tilesets() {
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
    let import =
        TiledImport::from_tmx_str(tmx, temp.path()).expect("multi-image collection import");
    let scene = TiledImporter::import_scene(&import);
    let palettes = TiledImporter::import_palettes(&import);

    let exported =
        TiledExporter::to_tmj_string(&scene, &palettes).expect("image collection export");
    let reimported = TiledImport::from_json_str(&exported, Path::new(".")).expect("reimport");
    let reimported_scene = TiledImporter::import_scene(&reimported);
    let reimported_palettes = TiledImporter::import_palettes(&reimported);

    assert_eq!(
        reimported_scene.layers[0]
            .tile(CellCoord::new(1, 0))
            .expect("tile")
            .tile_ref,
        TileRef::new(PaletteId(1), TileDefId(1))
    );
    assert_eq!(
        reimported_palettes[0].tiles[0].source_rect,
        RectU::new(0, 0, 2, 2)
    );
    assert_eq!(
        reimported_palettes[0].tiles[1].source_rect,
        RectU::new(3, 0, 3, 1)
    );
    match &reimported_palettes[0].texture {
        TileTextureSource::ImageCollectionAtlas { size, tiles } => {
            assert_eq!(*size, [6, 2]);
            assert_eq!(tiles.len(), 2);
        }
        other => panic!("unexpected texture source: {other:?}"),
    }
}

fn write_solid_png(path: &Path, width: u32, height: u32, color: [u8; 4]) {
    let mut image = image::RgbaImage::new(width, height);
    for pixel in image.pixels_mut() {
        *pixel = image::Rgba(color);
    }
    image.save(path).expect("write fixture png");
}
