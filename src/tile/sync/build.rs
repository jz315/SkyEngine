use std::collections::BTreeMap;

use crate::render::{
    TilemapDescriptor, TilemapHandle, TilemapOrientation, TilemapRenderer, TilemapStaggerAxis,
    TilemapStaggerIndex, TilemapStorage, TilesetGrid,
};

use super::super::grid::{GridOrientation, StaggerAxis, StaggerIndex};
use super::super::layer::{LayerData, LayerId, TileLayer};
use super::super::object::ObjectVisual;
use super::super::palette::{PaletteId, TilePaletteStore};
use super::super::scene::TileMap;
use super::{TileMapRenderData, TileMapRenderLayer, TileMapRenderSync, TileMapRenderSyncError};

impl TileMapRenderSync {
    pub fn build(
        scene: &TileMap,
        palettes: &TilePaletteStore,
    ) -> Result<TileMapRenderData, TileMapRenderSyncError> {
        let mut storage = TilemapStorage::new();
        let (map, layers) = Self::insert(scene, palettes, &mut storage)?;
        Ok(TileMapRenderData {
            storage,
            map,
            layers,
        })
    }

    pub fn insert(
        scene: &TileMap,
        palettes: &TilePaletteStore,
        storage: &mut TilemapStorage,
    ) -> Result<(TilemapHandle, Vec<TileMapRenderLayer>), TileMapRenderSyncError> {
        let mut splits = Vec::<(LayerId, PaletteId)>::new();
        for layer in &scene.layers {
            for palette in collect_layer_palettes(scene, layer) {
                splits.push((layer.id, palette));
            }
        }

        let map = storage.create(TilemapDescriptor::new(
            scene.size.width.max(1),
            scene.size.height.max(1),
            splits.len().max(1) as u32,
        ));

        let tilemap = storage
            .get_mut(map)
            .expect("new tile scene render tilemap should exist");
        for (storage_layer, (layer_id, palette_id)) in splits.iter().copied().enumerate() {
            Self::write_layer_to_tilemap(
                scene,
                tilemap,
                layer_id,
                palette_id,
                storage_layer as u32,
            );
        }

        let mut layers = Vec::with_capacity(splits.len());
        for (storage_layer, (layer_id, palette_id)) in splits.into_iter().enumerate() {
            let layer = scene
                .layer(layer_id)
                .expect("split source layer came from scene");
            let palette = palettes
                .get(palette_id)
                .ok_or(TileMapRenderSyncError::MissingPalette(palette_id))?;
            let grid = palette
                .tileset_grid()
                .ok_or(TileMapRenderSyncError::MissingTexture(palette_id))?;
            let renderer = renderer_for_scene_layer(scene, layer, map, grid, storage_layer as u32);
            layers.push(TileMapRenderLayer {
                source_layer: layer_id,
                palette: palette_id,
                storage_layer: storage_layer as u32,
                renderer,
            });
        }

        Ok((map, layers))
    }

    pub(crate) fn palettes_for_layer(scene: &TileMap, layer_id: LayerId) -> Vec<PaletteId> {
        scene
            .layer(layer_id)
            .map_or_else(Vec::new, |layer| collect_layer_palettes(scene, layer))
    }
}

fn collect_layer_palettes(scene: &TileMap, layer: &TileLayer) -> Vec<PaletteId> {
    let mut palettes = BTreeMap::<PaletteId, ()>::new();
    match &layer.data {
        LayerData::Tiles(data) => {
            for (_, tile) in data.tiles.iter() {
                palettes.insert(tile.tile_ref.palette, ());
            }
        }
        LayerData::Objects(data) => {
            for object_id in &data.objects {
                let Some(object) = scene.objects.get(*object_id) else {
                    continue;
                };
                match &object.visual {
                    ObjectVisual::Tile(tile_ref) => {
                        palettes.insert(tile_ref.palette, ());
                    }
                    ObjectVisual::MultiTile(tiles) => {
                        for visual_tile in tiles {
                            palettes.insert(visual_tile.tile_ref.palette, ());
                        }
                    }
                    ObjectVisual::Sprite(_) | ObjectVisual::None => {}
                }
            }
        }
        LayerData::Collision(_) | LayerData::Metadata(_) => {}
    }
    palettes.keys().copied().collect()
}

fn renderer_for_scene_layer(
    scene: &TileMap,
    layer: &TileLayer,
    map: TilemapHandle,
    grid: TilesetGrid,
    storage_layer: u32,
) -> TilemapRenderer {
    let mut renderer = TilemapRenderer::new(map, grid)
        .layer(storage_layer)
        .tile_size([
            scene.grid.cell_size[0] as f32,
            scene.grid.cell_size[1] as f32,
        ])
        .orientation(match scene.grid.orientation {
            GridOrientation::Orthogonal => TilemapOrientation::Orthogonal,
            GridOrientation::Isometric => TilemapOrientation::Isometric,
            GridOrientation::Staggered => TilemapOrientation::Staggered,
            GridOrientation::Hexagonal => TilemapOrientation::Hexagonal,
        })
        .render_order(scene.grid.render_order)
        .visible(layer.visible)
        .color(crate::render::Color::new(1.0, 1.0, 1.0, layer.opacity));
    if let Some(axis) = scene.grid.stagger_axis {
        renderer = renderer.stagger_axis(match axis {
            StaggerAxis::X => TilemapStaggerAxis::X,
            StaggerAxis::Y => TilemapStaggerAxis::Y,
        });
    }
    if let Some(index) = scene.grid.stagger_index {
        renderer = renderer.stagger_index(match index {
            StaggerIndex::Odd => TilemapStaggerIndex::Odd,
            StaggerIndex::Even => TilemapStaggerIndex::Even,
        });
    }
    if let Some(side) = scene.grid.hex_side_length {
        renderer = renderer.hex_side_length(side as f32);
    }
    renderer
}
