use sky_engine::ecs::World;
use sky_engine::render::{Color, SortingLayer, SpriteRenderer, Transform};
use sky_engine::tile::{
    LayerRole, SceneTile, TileMapDocument, TileMapInstance, TileMapSpawnOptions,
};

use crate::assets::GameAssets;
use crate::board::BoardState;
use crate::geometry::{cell_index, map_origin_y, COLS, ROWS, TILE_H, TILE_W};
use crate::model::{ground_tile_for, zone_tint};

const GROUND_LAYER_NAME: &str = "Ground";

pub struct GroundSceneInstance {
    pub _instance: TileMapInstance,
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
    let mut document = TileMapDocument::builder("Miniature Builder Ground")
        .id(2)
        .isometric([TILE_W as u32, TILE_H as u32])
        .size([COLS as u32, ROWS as u32])
        .tile_layer(GROUND_LAYER_NAME, LayerRole::Ground)
        .build()
        .with_palette_store(assets.ground_palettes.clone());

    document.edit_recorded(|edit| {
        for row in 0..ROWS {
            for col in 0..COLS {
                let zone = board.cells[cell_index(row, col)].zone;
                let tile_ref = assets.ground_tile_ref(ground_tile_for(row, col, zone));
                edit.set_named(
                    GROUND_LAYER_NAME,
                    [col as i32, row as i32],
                    SceneTile::tinted(tile_ref, zone_tint(zone)),
                );
            }
        }
    });

    let instance = TileMapInstance::spawn_document(
        world,
        &document,
        TileMapSpawnOptions::at([0.0, -map_origin_y()]),
    )
    .expect("miniature builder ground tile scene should spawn");
    for entity in &instance.entities {
        if let Some(layer) = world.get_mut::<SortingLayer>(*entity) {
            layer.0 = -20;
        }
    }
    world.insert_resource(GroundSceneInstance {
        _instance: instance,
    });
}
