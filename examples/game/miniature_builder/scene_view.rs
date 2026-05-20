use sky_engine::ecs::World;
use sky_engine::render::{Color, SortingLayer, SpriteRenderer, Transform};
use sky_engine::tile::{MapId, TileCell, Tiles};

use crate::assets::GameAssets;
use crate::board::BoardState;
use crate::geometry::{cell_index, COLS, ROWS, TILE_H, TILE_W};
use crate::model::{ground_tile_for, zone_tint};

const GROUND_LAYER_NAME: &str = "Ground";

pub struct GroundSceneInstance {
    pub _map: MapId,
}

pub fn spawn_backdrop(world: &mut World) {
    world.spawn((
        Transform::from_xyz(0.0, 4.0, 0.0),
        SpriteRenderer::new(1260.0, 720.0).color(Color::rgba8(45, 54, 54, 255)),
        SortingLayer(-100),
    ));
    world.spawn((
        Transform::from_xyz(0.0, -8.0, 0.0),
        SpriteRenderer::new(1110.0, 620.0).color(Color::rgba8(69, 76, 69, 255)),
        SortingLayer(-90),
    ));
}

pub fn mount_ground_scene(world: &mut World, assets: &GameAssets, board: &BoardState) {
    let map_id = {
        let mut tiles = Tiles::new(world);
        let mut map = tiles
            .create("Miniature Builder Ground")
            .isometric([TILE_W as u32, TILE_H as u32])
            .size([COLS as u32, ROWS as u32])
            .palettes(assets.ground_palettes.iter().cloned())
            .tiles(GROUND_LAYER_NAME)
            .build()
            .expect("miniature builder ground tile scene should spawn");

        map.edit(|edit| {
            for row in 0..ROWS {
                for col in 0..COLS {
                    let zone = board.cells[cell_index(row, col)].zone;
                    let tile_ref = assets.ground_tile_ref(ground_tile_for(row, col, zone));
                    edit.tiles(GROUND_LAYER_NAME)?.set(
                        [col as i32, row as i32],
                        TileCell::tinted(tile_ref, zone_tint(zone)),
                    )?;
                }
            }
            Ok(())
        })
        .expect("miniature builder ground edits should apply");
        map.id()
    };

    world.insert_resource(GroundSceneInstance { _map: map_id });
}
