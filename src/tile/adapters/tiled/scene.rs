use std::path::Path;

use crate::render::{
    TiledImport, TiledImportError, TiledLayer, TiledObjectShape, TilemapOrientation,
    TilemapStaggerAxis, TilemapStaggerIndex,
};
use crate::tile::{
    CellCoord, GridOrientation, GridSpec, LayerId, ObjectVisual, PaletteId, SceneTile, StaggerAxis,
    StaggerIndex, TileDefId, TileLayer, TileMap, TileMapId, TileMapSize, TileObject, TileObjectId,
    TileRef,
};

use super::properties::{infer_layer_role, property_bag_from_tiled};

pub(super) fn load_scene(path: impl AsRef<Path>) -> Result<TileMap, TiledImportError> {
    let import = TiledImport::from_file(path)?;
    Ok(import_scene(&import))
}

pub(super) fn import_scene(import: &TiledImport) -> TileMap {
    let mut scene = TileMap::new(
        TileMapId(1),
        "Tiled Map",
        grid_from_import(import),
        TileMapSize::new(import.map.width(), import.map.height()),
    );
    scene.palettes = import
        .tilesets
        .iter()
        .map(|tileset| PaletteId(tileset.first_gid))
        .collect();
    scene.properties = property_bag_from_tiled(&import.properties);

    for layer in &import.layers {
        scene.layers.push(tile_layer_from_import(import, layer));
    }

    let mut next_layer_id = scene.layers.len() as u32 + 1;
    for object_layer in &import.object_layers {
        let layer_id = LayerId(next_layer_id);
        next_layer_id = next_layer_id.saturating_add(1);
        let mut layer = TileLayer::objects(
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
            let mut scene_object = TileObject::new(
                id,
                layer_id,
                CellCoord::new(
                    (object.position[0] / import.tile_size[0].max(1) as f32).floor() as i32,
                    (object.position[1] / import.tile_size[1].max(1) as f32).floor() as i32,
                ),
            );
            scene_object.properties = property_bag_from_tiled(&object.properties);
            if let TiledObjectShape::Tile {
                tileset_index,
                tile_id,
                ..
            } = object.shape
            {
                if let Some(tileset) = import.tilesets.get(tileset_index) {
                    scene_object.visual = ObjectVisual::Tile(TileRef::new(
                        PaletteId(tileset.first_gid),
                        TileDefId(tile_id.0),
                    ));
                }
            }
            let object_id = scene.objects.insert(scene_object);
            layer.add_object_id(object_id);
        }
        scene.layers.push(layer);
    }

    scene
}

fn grid_from_import(import: &TiledImport) -> GridSpec {
    let mut grid = GridSpec::new(
        match import.orientation {
            TilemapOrientation::Orthogonal => GridOrientation::Orthogonal,
            TilemapOrientation::Isometric => GridOrientation::Isometric,
            TilemapOrientation::Staggered => GridOrientation::Staggered,
            TilemapOrientation::Hexagonal => GridOrientation::Hexagonal,
        },
        import.tile_size,
    );
    grid.render_order = import.render_order;
    grid.stagger_axis = Some(match import.stagger_axis {
        TilemapStaggerAxis::X => StaggerAxis::X,
        TilemapStaggerAxis::Y => StaggerAxis::Y,
    });
    grid.stagger_index = Some(match import.stagger_index {
        TilemapStaggerIndex::Odd => StaggerIndex::Odd,
        TilemapStaggerIndex::Even => StaggerIndex::Even,
    });
    grid.hex_side_length = (import.hex_side_length > 0).then_some(import.hex_side_length);
    grid
}

fn tile_layer_from_import(import: &TiledImport, layer: &TiledLayer) -> TileLayer {
    let mut scene_layer = TileLayer::tiles(
        LayerId(layer.storage_layer + 1),
        layer.name.clone(),
        infer_layer_role(&layer.name),
    );
    scene_layer.visible = layer.visible;
    scene_layer.opacity = layer.opacity;
    scene_layer.offset = layer.offset;
    scene_layer.parallax = layer.parallax;
    scene_layer.properties = property_bag_from_tiled(&layer.properties);

    let palette = import.tilesets[layer.tileset_index].first_gid;
    for y in 0..import.map.height() {
        for x in 0..import.map.width() {
            let Some(tile) = import.map.tile(layer.storage_layer, x, y) else {
                continue;
            };
            if tile.is_empty() {
                continue;
            }
            let _ = scene_layer.set_tile(
                CellCoord::new(x as i32, y as i32),
                Some(SceneTile {
                    tile_ref: TileRef::new(PaletteId(palette), TileDefId(tile.id.0)),
                    flags: tile.flags,
                    tint: tile.tint,
                }),
            );
        }
    }
    scene_layer
}
