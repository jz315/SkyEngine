use crate::asset::{AssetId, Handle, TextureAsset};
use crate::render::{Color, TileId};

use super::*;
use crate::tile::{
    CellCoord, GridSpec, LayerId, LayerRole, PaletteId, RectU, SceneTile, TileDef, TileDefId,
    TileLayer, TileMap, TileMapId, TileMapSize, TilePalette, TilePaletteStore, TileRef,
    TileTextureSource,
};

#[test]
fn scene_render_sync_splits_tile_layers_by_palette() {
    let texture = Handle::<TextureAsset>::new(AssetId::new());
    let mut palettes = TilePaletteStore::new();
    let mut palette_a = TilePalette::new(PaletteId(1), "ground");
    palette_a.texture = TileTextureSource::Texture {
        handle: texture,
        size: [32, 16],
    };
    palette_a
        .tiles
        .push(TileDef::new(TileDefId(0), RectU::new(0, 0, 16, 16)));
    palettes.insert(palette_a);
    let mut palette_b = TilePalette::new(PaletteId(2), "props");
    palette_b.texture = TileTextureSource::Texture {
        handle: texture,
        size: [32, 16],
    };
    palette_b
        .tiles
        .push(TileDef::new(TileDefId(0), RectU::new(16, 0, 16, 16)));
    palettes.insert(palette_b);

    let mut scene = TileMap::new(
        TileMapId(1),
        "test",
        GridSpec::orthogonal([16, 16]),
        TileMapSize::new(2, 1),
    );
    let mut layer = TileLayer::tiles(LayerId(7), "Ground", LayerRole::Ground);
    layer.set_tile(
        CellCoord::new(0, 0),
        Some(SceneTile::new(TileRef::new(PaletteId(1), TileDefId(0)))),
    );
    layer.set_tile(
        CellCoord::new(1, 0),
        Some(SceneTile {
            tile_ref: TileRef::new(PaletteId(2), TileDefId(0)),
            flags: crate::render::TileFlags::empty(),
            tint: Color::RED,
        }),
    );
    scene.layers.push(layer);

    let data = TileMapRenderSync::build(&scene, &palettes).expect("sync should work");

    assert_eq!(data.layers.len(), 2);
    assert_eq!(data.layers[0].source_layer, LayerId(7));
    assert_eq!(data.layers[0].storage_layer, 0);
    assert_eq!(data.layers[1].storage_layer, 1);
    let map = data.storage.get(data.map).expect("render tilemap");
    assert_eq!(map.tile(0, 0, 0).unwrap().id, TileId(0));
    assert!(map.tile(0, 1, 0).unwrap().is_empty());
    assert!(map.tile(1, 0, 0).unwrap().is_empty());
    assert_eq!(map.tile(1, 1, 0).unwrap().id, TileId(0));
}
