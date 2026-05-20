use sky_engine::ecs::World;
use sky_engine::tile::{object, MapId, ObjectId, Tiles};

use crate::assets::GameAssets;
use crate::board::{BoardDelta, BoardDeltaQueue, BoardState};
use crate::geometry::{cell_index, COLS, ROWS, TILE_H, TILE_W};
use crate::hud::HudState;
use crate::model::PlacedStructure;

pub struct StructureProjection {
    pub map: MapId,
    object_ids_by_cell: Vec<Option<ObjectId>>,
}

pub fn setup_structure_projection(world: &mut World, assets: &GameAssets, board: &BoardState) {
    let mut object_ids_by_cell = vec![None; ROWS * COLS];
    let map_id = {
        let mut tiles = Tiles::new(world);
        let mut map = tiles
            .create("Miniature Builder Structures")
            .isometric([TILE_W as u32, TILE_H as u32])
            .size([COLS as u32, ROWS as u32])
            .palettes(assets.structure_palettes.iter().cloned())
            .objects("Props")
            .build()
            .expect("miniature builder structure scene should spawn");

        map.edit(|edit| {
            for row in 0..ROWS {
                for col in 0..COLS {
                    let Some(structure) = board.cells[cell_index(row, col)].structure else {
                        continue;
                    };
                    let object = place_structure(edit, assets, structure, row, col)?;
                    object_ids_by_cell[cell_index(row, col)] = Some(object);
                }
            }
            Ok(())
        })
        .expect("miniature builder structure edits should apply");
        map.id()
    };

    world.insert_resource(StructureProjection {
        map: map_id,
        object_ids_by_cell,
    });
}

pub fn apply_board_deltas(world: &mut World) {
    let Some(mut deltas) = world.remove_resource::<BoardDeltaQueue>() else {
        return;
    };
    if deltas.0.is_empty() {
        world.insert_resource(deltas);
        return;
    }
    let Some(assets) = world.get_resource::<GameAssets>().cloned() else {
        world.insert_resource(deltas);
        return;
    };
    let Some(mut projection) = world.remove_resource::<StructureProjection>() else {
        world.insert_resource(deltas);
        return;
    };

    let mut edit_error = None;
    {
        let mut tiles = Tiles::new(world);
        match tiles.map(projection.map) {
            Ok(mut map) => {
                for delta in deltas.0.drain(..) {
                    match delta {
                        BoardDelta::SetStructure {
                            row,
                            col,
                            structure,
                        } => {
                            if let Err(error) = apply_structure_delta(
                                &mut projection.object_ids_by_cell,
                                &mut map,
                                &assets,
                                row,
                                col,
                                structure,
                            ) {
                                edit_error = Some(error.to_string());
                                break;
                            }
                        }
                    }
                }
            }
            Err(error) => {
                edit_error = Some(error.to_string());
            }
        }
    }
    if let Some(error) = edit_error {
        if let Some(hud) = world.get_resource_mut::<HudState>() {
            hud.message = format!("scene edit failed: {error}");
        }
    }

    world.insert_resource(projection);
    world.insert_resource(deltas);
}

fn apply_structure_delta(
    object_ids_by_cell: &mut [Option<ObjectId>],
    map: &mut sky_engine::tile::Map<'_>,
    assets: &GameAssets,
    row: usize,
    col: usize,
    structure: Option<PlacedStructure>,
) -> Result<(), sky_engine::tile::TileError> {
    let index = cell_index(row, col);
    if let Some(object) = object_ids_by_cell[index].take() {
        map.objects("Props")?.remove(object)?;
    }
    if let Some(structure) = structure {
        let object = place_structure_map(map, assets, structure, row, col)?;
        object_ids_by_cell[index] = Some(object);
    }
    Ok(())
}

fn place_structure(
    edit: &mut sky_engine::tile::MapEditor<'_>,
    assets: &GameAssets,
    structure: PlacedStructure,
    row: usize,
    col: usize,
) -> Result<ObjectId, sky_engine::tile::TileError> {
    Ok(edit
        .objects("Props")?
        .place(
            object("structure")
                .at([col as i32, row as i32])
                .visual_tile(assets.structure_tile_ref(structure.blueprint, structure.orientation)),
        )?
        .id())
}

fn place_structure_map(
    map: &mut sky_engine::tile::Map<'_>,
    assets: &GameAssets,
    structure: PlacedStructure,
    row: usize,
    col: usize,
) -> Result<ObjectId, sky_engine::tile::TileError> {
    Ok(map
        .objects("Props")?
        .place(
            object("structure")
                .at([col as i32, row as i32])
                .visual_tile(assets.structure_tile_ref(structure.blueprint, structure.orientation)),
        )?
        .id())
}

#[cfg(test)]
mod tests {
    use sky_engine::asset::{AssetConfig, Assets, TextureAsset};
    use sky_engine::ecs::World;
    use sky_engine::tile::{
        RectU, TileDef, TileDefId, TilePalette, TilePaletteStore, TileTextureSource,
    };

    use super::{apply_board_deltas, setup_structure_projection, StructureProjection};
    use crate::assets::{GameAssets, LoadedBlueprint, SpriteAsset, STRUCTURE_PALETTE};
    use crate::board::{BoardDelta, BoardDeltaQueue, BoardState};
    use crate::hud::HudState;
    use crate::model::BLUEPRINTS;

    fn test_assets(world: &mut World) -> GameAssets {
        let server = world
            .get_resource::<Assets>()
            .expect("projection test assets");
        let texture = server.insert_runtime(TextureAsset::white_pixel());
        let sprite = SpriteAsset {
            handle: texture.clone(),
            width: 1.0,
            height: 1.0,
            uv: [0.0, 0.0, 1.0, 1.0],
        };
        let blueprints = BLUEPRINTS
            .iter()
            .map(|_| LoadedBlueprint {
                rotations: std::array::from_fn(|_| sprite.clone()),
            })
            .collect();
        let mut structure_palette = TilePalette::new(STRUCTURE_PALETTE, "Test structures");
        structure_palette.texture = TileTextureSource::Texture {
            handle: texture,
            size: [1, 1],
        };
        for index in 0..BLUEPRINTS.len() * crate::model::ORIENTATIONS.len() {
            let mut tile = TileDef::new(TileDefId(index as u32), RectU::new(0, 0, 1, 1));
            tile.draw_size = [1, 1];
            tile.draw_offset = [0, 0];
            structure_palette.tiles.push(tile);
        }
        let mut structure_palettes = TilePaletteStore::new();
        structure_palettes.insert(structure_palette);

        GameAssets {
            hover: SpriteAsset {
                handle: server.insert_runtime(TextureAsset::white_pixel()),
                width: 1.0,
                height: 1.0,
                uv: [0.0, 0.0, 1.0, 1.0],
            },
            blueprints,
            ground_palettes: TilePaletteStore::new(),
            structure_palettes,
        }
    }

    fn test_world() -> World {
        let mut world = World::new();
        world.insert_resource(Assets::with_empty_manifest(AssetConfig::default()));
        world.insert_resource(HudState::default());
        world
    }

    #[test]
    fn board_delta_creates_object_layer_edit() {
        let mut world = test_world();
        let assets = test_assets(&mut world);
        let board = BoardState::new();
        setup_structure_projection(&mut world, &assets, &board);
        world.insert_resource(assets);
        world.insert_resource(BoardDeltaQueue(vec![BoardDelta::SetStructure {
            row: 3,
            col: 4,
            structure: Some(crate::model::PlacedStructure {
                blueprint: 2,
                orientation: 1,
            }),
        }]));

        apply_board_deltas(&mut world);

        let projection = world
            .get_resource::<StructureProjection>()
            .expect("projection resource");
        assert!(projection.object_ids_by_cell[crate::geometry::cell_index(3, 4)].is_some());
    }

    #[test]
    fn replace_and_remove_keep_object_ids_in_sync() {
        let mut world = test_world();
        let assets = test_assets(&mut world);
        let mut board = BoardState::new();
        let _ = board.place_at(0, 0, 2, 2);
        setup_structure_projection(&mut world, &assets, &board);
        world.insert_resource(assets);

        world.insert_resource(BoardDeltaQueue(vec![BoardDelta::SetStructure {
            row: 2,
            col: 2,
            structure: Some(crate::model::PlacedStructure {
                blueprint: 4,
                orientation: 3,
            }),
        }]));
        apply_board_deltas(&mut world);
        let replaced = world
            .get_resource::<StructureProjection>()
            .unwrap()
            .object_ids_by_cell[crate::geometry::cell_index(2, 2)];
        assert!(replaced.is_some());

        world.insert_resource(BoardDeltaQueue(vec![BoardDelta::SetStructure {
            row: 2,
            col: 2,
            structure: None,
        }]));
        apply_board_deltas(&mut world);
        let projection = world.get_resource::<StructureProjection>().unwrap();
        assert!(projection.object_ids_by_cell[crate::geometry::cell_index(2, 2)].is_none());
    }

    #[test]
    fn board_delta_updates_live_map_projection() {
        let mut world = test_world();
        let assets = test_assets(&mut world);
        let board = BoardState::new();
        setup_structure_projection(&mut world, &assets, &board);
        world.insert_resource(assets);
        world.insert_resource(BoardDeltaQueue(vec![BoardDelta::SetStructure {
            row: 1,
            col: 1,
            structure: Some(crate::model::PlacedStructure {
                blueprint: 1,
                orientation: 2,
            }),
        }]));

        apply_board_deltas(&mut world);
        let projection = world.get_resource::<StructureProjection>().unwrap();
        assert!(projection.object_ids_by_cell[crate::geometry::cell_index(1, 1)].is_some());
    }
}
