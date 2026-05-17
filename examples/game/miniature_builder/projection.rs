use sky_engine::ecs::World;
use sky_engine::tile::{
    CellCoord, Footprint, LayerId, LayerRole, ObjectVisual, TileLayer, TileMapDocument,
    TileMapEditSession, TileMapEditSummary, TileMapInstance, TileMapSpawnOptions, TileObject,
    TileObjectId,
};

use crate::assets::GameAssets;
use crate::board::{BoardDelta, BoardDeltaQueue, BoardState};
use crate::geometry::{cell_index, map_origin_y, COLS, ROWS, TILE_H, TILE_W};
use crate::hud::HudState;
use crate::model::PlacedStructure;

pub const STRUCTURE_LAYER: LayerId = LayerId(2);

pub struct StructureProjection {
    pub document: TileMapDocument,
    pub instance: TileMapInstance,
    object_ids_by_cell: Vec<Option<TileObjectId>>,
    pending_summaries: Vec<TileMapEditSummary>,
}

pub fn setup_structure_projection(world: &mut World, assets: &GameAssets, board: &BoardState) {
    let mut document = TileMapDocument::builder("Miniature Builder Structures")
        .id(1)
        .isometric([TILE_W as u32, TILE_H as u32])
        .size([COLS as u32, ROWS as u32])
        .layer(TileLayer::objects(
            STRUCTURE_LAYER,
            "Props",
            LayerRole::Props,
        ))
        .build()
        .with_palette_store(assets.structure_palettes.clone());

    let mut object_ids_by_cell = vec![None; ROWS * COLS];
    {
        let mut edits = TileMapEditSession::new(&mut document.scene);
        for row in 0..ROWS {
            for col in 0..COLS {
                let Some(structure) = board.cells[cell_index(row, col)].structure else {
                    continue;
                };
                let object = place_structure(&mut edits, assets, structure, row, col);
                object_ids_by_cell[cell_index(row, col)] = Some(object);
            }
        }
        let _ = edits.finish();
    }

    let instance = TileMapInstance::spawn_document(
        world,
        &document,
        TileMapSpawnOptions::at([0.0, -map_origin_y()]),
    )
    .expect("miniature builder structure scene should spawn");

    world.insert_resource(StructureProjection {
        document,
        instance,
        object_ids_by_cell,
        pending_summaries: Vec::new(),
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

    let object_ids_by_cell = &mut projection.object_ids_by_cell;
    let mut edits = TileMapEditSession::new(&mut projection.document.scene);
    for delta in deltas.0.drain(..) {
        match delta {
            BoardDelta::SetStructure {
                row,
                col,
                structure,
            } => {
                apply_structure_delta(object_ids_by_cell, &mut edits, &assets, row, col, structure)
            }
        }
    }
    let summary = edits.finish();
    if !summary.is_empty() {
        projection.pending_summaries.push(summary);
    }

    world.insert_resource(projection);
    world.insert_resource(deltas);
}

pub fn sync_structure_instance(world: &mut World) {
    let Some(mut projection) = world.remove_resource::<StructureProjection>() else {
        return;
    };
    if projection.pending_summaries.is_empty() {
        world.insert_resource(projection);
        return;
    }

    let summaries = std::mem::take(&mut projection.pending_summaries);
    for summary in summaries {
        if let Err(error) =
            projection
                .instance
                .refresh_document_edit_summary(world, &projection.document, &summary)
        {
            if let Some(hud) = world.get_resource_mut::<HudState>() {
                hud.message = format!("scene refresh failed: {error}");
            }
            break;
        }
    }

    world.insert_resource(projection);
}

fn apply_structure_delta(
    object_ids_by_cell: &mut [Option<TileObjectId>],
    edits: &mut TileMapEditSession<'_>,
    assets: &GameAssets,
    row: usize,
    col: usize,
    structure: Option<PlacedStructure>,
) {
    let index = cell_index(row, col);
    if let Some(object) = object_ids_by_cell[index].take() {
        edits.remove_object(object);
    }
    if let Some(structure) = structure {
        let object = place_structure(edits, assets, structure, row, col);
        object_ids_by_cell[index] = Some(object);
    }
}

fn place_structure(
    edits: &mut TileMapEditSession<'_>,
    assets: &GameAssets,
    structure: PlacedStructure,
    row: usize,
    col: usize,
) -> TileObjectId {
    let mut object = TileObject::new(
        TileObjectId(0),
        STRUCTURE_LAYER,
        CellCoord::new(col as i32, row as i32),
    );
    object.footprint = Footprint::one_cell();
    object.visual =
        ObjectVisual::Tile(assets.structure_tile_ref(structure.blueprint, structure.orientation));
    edits.place_object(STRUCTURE_LAYER, object)
}

#[cfg(test)]
mod tests {
    use sky_engine::asset::{AssetConfig, AssetId, AssetServer, Handle, TextureAsset};
    use sky_engine::ecs::World;
    use sky_engine::render::TilemapStorage;
    use sky_engine::tile::{
        RectU, TileDef, TileDefId, TilePalette, TilePaletteStore, TileTextureSource,
    };

    use super::{
        apply_board_deltas, setup_structure_projection, sync_structure_instance,
        StructureProjection,
    };
    use crate::assets::{GameAssets, LoadedBlueprint, SpriteAsset, STRUCTURE_PALETTE};
    use crate::board::{BoardDelta, BoardDeltaQueue, BoardState};
    use crate::hud::HudState;
    use crate::model::BLUEPRINTS;

    fn test_assets(world: &mut World) -> GameAssets {
        let server = world
            .get_resource::<AssetServer>()
            .expect("projection test asset server");
        let texture = server.insert_runtime(TextureAsset::white_pixel());
        let sprite = SpriteAsset {
            handle: texture,
            width: 1.0,
            height: 1.0,
            uv: [0.0, 0.0, 1.0, 1.0],
        };
        let blueprints = BLUEPRINTS
            .iter()
            .map(|_| LoadedBlueprint {
                rotations: [sprite; 4],
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
                handle: Handle::new(AssetId::new()),
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
        world.insert_resource(AssetServer::with_empty_manifest(AssetConfig::default()));
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
        assert_eq!(projection.pending_summaries.len(), 1);
        assert!(!projection.pending_summaries[0].object_changes.is_empty());
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
    fn summary_refresh_updates_tilemap_storage() {
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
        sync_structure_instance(&mut world);

        let projection = world.get_resource::<StructureProjection>().unwrap();
        let storage = world
            .get_resource::<TilemapStorage>()
            .expect("tilemap storage");
        let tilemap = storage
            .get(projection.instance.scene)
            .expect("projection tilemap payload");
        let tile = tilemap.tile(0, 1, 1).expect("rendered tile");
        assert!(!tile.id.is_empty());
    }
}
