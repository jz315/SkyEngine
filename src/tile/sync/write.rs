use crate::render::{Tile, TileId};

use super::super::grid::{CellCoord, CellRect};
use super::super::layer::{LayerData, LayerId, SceneTile, TileRef};
use super::super::object::ObjectVisual;
use super::super::palette::PaletteId;
use super::super::scene::{TileMap, TileMapSize};
use super::TileMapRenderSync;

impl TileMapRenderSync {
    pub(crate) fn write_layer_to_tilemap(
        scene: &TileMap,
        tilemap: &mut crate::render::Tilemap,
        layer_id: LayerId,
        palette_id: PaletteId,
        storage_layer: u32,
    ) {
        let Some(layer) = scene.layer(layer_id) else {
            return;
        };
        match &layer.data {
            LayerData::Tiles(data) => {
                for (cell, tile) in data.tiles.iter() {
                    write_scene_tile(tilemap, storage_layer, scene.size, cell, tile, palette_id);
                }
            }
            LayerData::Objects(data) => {
                for object_id in &data.objects {
                    let Some(object) = scene.objects.get(*object_id) else {
                        continue;
                    };
                    match &object.visual {
                        ObjectVisual::Tile(tile_ref) => {
                            write_scene_tile(
                                tilemap,
                                storage_layer,
                                scene.size,
                                object.cell,
                                SceneTile::new(*tile_ref),
                                palette_id,
                            );
                        }
                        ObjectVisual::MultiTile(tiles) => {
                            for visual_tile in tiles {
                                let cell = CellCoord::new(
                                    object.cell.x + visual_tile.offset[0],
                                    object.cell.y + visual_tile.offset[1],
                                );
                                write_scene_tile(
                                    tilemap,
                                    storage_layer,
                                    scene.size,
                                    cell,
                                    SceneTile::new(visual_tile.tile_ref),
                                    palette_id,
                                );
                            }
                        }
                        ObjectVisual::Sprite(_) | ObjectVisual::None => {}
                    }
                }
            }
            LayerData::Collision(_) | LayerData::Metadata(_) => {}
        }
    }

    pub(crate) fn clear_cell_in_tilemap(
        scene: &TileMap,
        tilemap: &mut crate::render::Tilemap,
        storage_layer: u32,
        cell: CellCoord,
    ) {
        if cell.x < 0 || cell.y < 0 {
            return;
        }
        let x = cell.x as u32;
        let y = cell.y as u32;
        if x >= scene.size.width || y >= scene.size.height {
            return;
        }
        let _ = tilemap.set_tile(storage_layer, x, y, Tile::EMPTY);
    }

    pub(crate) fn write_cell_to_tilemap(
        scene: &TileMap,
        tilemap: &mut crate::render::Tilemap,
        layer_id: LayerId,
        palette_id: PaletteId,
        storage_layer: u32,
        cell: CellCoord,
    ) {
        let Some(layer) = scene.layer(layer_id) else {
            return;
        };
        if let LayerData::Tiles(data) = &layer.data {
            if let Some(tile) = data.tiles.get(cell) {
                write_scene_tile(tilemap, storage_layer, scene.size, cell, tile, palette_id);
            }
        }
    }

    pub(crate) fn write_rect_to_tilemap(
        scene: &TileMap,
        tilemap: &mut crate::render::Tilemap,
        layer_id: LayerId,
        palette_id: PaletteId,
        storage_layer: u32,
        rect: CellRect,
    ) {
        for cell in rect.cells() {
            Self::write_cell_to_tilemap(scene, tilemap, layer_id, palette_id, storage_layer, cell);
        }
    }
}

fn write_scene_tile(
    tilemap: &mut crate::render::Tilemap,
    layer: u32,
    size: TileMapSize,
    cell: CellCoord,
    tile: SceneTile,
    palette: PaletteId,
) {
    if tile.tile_ref.palette != palette || cell.x < 0 || cell.y < 0 {
        return;
    }
    let x = cell.x as u32;
    let y = cell.y as u32;
    if x >= size.width || y >= size.height {
        return;
    }
    let _ = tilemap.set_tile(
        layer,
        x,
        y,
        render_tile(tile.tile_ref, tile.flags, tile.tint),
    );
}

fn render_tile(
    tile_ref: TileRef,
    flags: crate::render::TileFlags,
    tint: crate::render::Color,
) -> Tile {
    Tile::tinted(TileId(tile_ref.tile.0), tint).with_flags(flags)
}
