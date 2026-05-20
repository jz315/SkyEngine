use crate::tile::model::MapLayer;
use crate::tile::{
    CellCoord, GridSpec, LayerId, MapData, MapId, MapSize, ObjectVisual, PaletteId, TileCell,
    TileDefId, TileObject, TileObjectId, TileRef,
};

use super::import::{TiledLayer, TiledMapSnapshot, TiledObjectShape};
use super::properties::{infer_layer_role, property_bag_from_tiled};

pub(super) fn import_map(import: &TiledMapSnapshot) -> MapData {
    let mut data = MapData::new(
        MapId(1),
        "Tiled Map",
        grid_from_import(import),
        MapSize::new(import.map_size[0], import.map_size[1]),
    );
    data.palettes = import
        .tilesets
        .iter()
        .map(|tileset| PaletteId(tileset.first_gid))
        .collect();
    data.properties = property_bag_from_tiled(&import.properties);

    for layer in &import.layers {
        data.layers.push(tile_layer_from_import(import, layer));
    }

    let mut next_layer_id = data.layers.len() as u32 + 1;
    for object_layer in &import.object_layers {
        let layer_id = LayerId(next_layer_id);
        next_layer_id = next_layer_id.saturating_add(1);
        let mut layer = MapLayer::objects(
            layer_id,
            object_layer.name.clone(),
            infer_layer_role(&object_layer.name),
        );
        layer.visible = object_layer.visible;
        layer.opacity = object_layer.opacity;
        layer.offset = object_layer.offset;
        layer.parallax = object_layer.parallax;
        layer.properties = property_bag_from_tiled(&object_layer.properties);

        for object in &object_layer.objects {
            let id = TileObjectId(object.id as u64);
            let mut map_object = TileObject::new(
                id,
                layer_id,
                CellCoord::new(
                    (object.position[0] / import.tile_size[0].max(1) as f32).floor() as i32,
                    (object.position[1] / import.tile_size[1].max(1) as f32).floor() as i32,
                ),
            );
            map_object.properties = property_bag_from_tiled(&object.properties);
            if let TiledObjectShape::Tile {
                tileset_index,
                tile_id,
                ..
            } = object.shape
            {
                if let Some(tileset) = import.tilesets.get(tileset_index) {
                    map_object.visual = ObjectVisual::Tile(TileRef::new(
                        PaletteId(tileset.first_gid),
                        TileDefId(tile_id),
                    ));
                }
            }
            let object_id = data.objects.insert(map_object);
            layer.add_object_id(object_id);
        }
        data.layers.push(layer);
    }

    data
}

fn grid_from_import(import: &TiledMapSnapshot) -> GridSpec {
    let mut grid = GridSpec::new(import.orientation, import.tile_size);
    grid.render_order = import.render_order;
    grid.stagger_axis = Some(import.stagger_axis);
    grid.stagger_index = Some(import.stagger_index);
    grid.hex_side_length = (import.hex_side_length > 0).then_some(import.hex_side_length);
    grid
}

fn tile_layer_from_import(import: &TiledMapSnapshot, layer: &TiledLayer) -> MapLayer {
    let mut map_layer = MapLayer::tiles(
        LayerId(layer.storage_layer + 1),
        layer.name.clone(),
        infer_layer_role(&layer.name),
    );
    map_layer.visible = layer.visible;
    map_layer.opacity = layer.opacity;
    map_layer.offset = layer.offset;
    map_layer.parallax = layer.parallax;
    map_layer.properties = property_bag_from_tiled(&layer.properties);

    let palette = import.tilesets[layer.tileset_index].first_gid;
    for tile in &layer.tiles {
        let _ = map_layer.set_tile(
            CellCoord::new(tile.x as i32, tile.y as i32),
            Some(TileCell {
                tile_ref: TileRef::new(PaletteId(palette), TileDefId(tile.tile_id)),
                flags: tile.flags,
                tint: tile.tint,
            }),
        );
    }
    map_layer
}
