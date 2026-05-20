use sky_engine::ecs::{EntityId, World};
use sky_engine::render::{Color, SortingLayer, SpriteRenderer, Transform};

use crate::assets::GameAssets;
use crate::board::BoardState;
use crate::geometry::{cell_center, cell_index, cell_sort_layer, tiled_image_center};
use crate::selection::{BuildSelection, HoverState};

#[derive(Clone, Copy)]
pub struct PreviewEntities {
    hover_tile: EntityId,
    ghost: EntityId,
}

pub fn spawn_preview_entities(world: &mut World, assets: &GameAssets) {
    let hover_tile = world.spawn((
        Transform::from_xyz(0.0, 0.0, 0.0),
            SpriteRenderer::new(assets.hover.width, assets.hover.height)
            .texture(assets.hover.handle.clone())
            .uv(
                assets.hover.uv[0],
                assets.hover.uv[1],
                assets.hover.uv[2],
                assets.hover.uv[3],
            )
            .color(Color::rgba8(150, 225, 255, 125))
            .visible(false),
        SortingLayer(10_000),
    ));
    let first = assets.blueprints[0].rotations[0].clone();
    let ghost = world.spawn((
        Transform::from_xyz(0.0, 0.0, 0.0),
        SpriteRenderer::new(first.width, first.height)
            .texture(first.handle.clone())
            .uv(first.uv[0], first.uv[1], first.uv[2], first.uv[3])
            .color(Color::rgba8(255, 255, 255, 165))
            .visible(false),
        SortingLayer(10_001),
    ));
    world.insert_resource(PreviewEntities { hover_tile, ghost });
}

pub fn update_preview(world: &mut World) {
    let Some(preview) = world.get_resource::<PreviewEntities>().copied() else {
        return;
    };
    let Some(assets) = world.get_resource::<GameAssets>().cloned() else {
        hide_preview(world, preview);
        return;
    };

    let Some(frame) = preview_frame(world, &assets) else {
        hide_preview(world, preview);
        return;
    };

    if let Some(transform) = world.get_mut::<Transform>(preview.hover_tile) {
        transform.position[0] = frame.hover_position.x();
        transform.position[1] = frame.hover_position.y();
    }
    if let Some(sprite_renderer) = world.get_mut::<SpriteRenderer>(preview.hover_tile) {
        sprite_renderer.visible = true;
        sprite_renderer.width = assets.hover.width * 1.06;
        sprite_renderer.height = assets.hover.height * 1.06;
        sprite_renderer.uv = assets.hover.uv;
        sprite_renderer.texture = Some(assets.hover.handle.clone());
        sprite_renderer.color = frame.hover_color;
    }
    if let Some(sort) = world.get_mut::<SortingLayer>(preview.hover_tile) {
        sort.0 = frame.base_sort_layer + 2;
    }

    if let Some(transform) = world.get_mut::<Transform>(preview.ghost) {
        transform.position[0] = frame.ghost_position.x();
        transform.position[1] = frame.ghost_position.y();
    }
    if let Some(sprite_renderer) = world.get_mut::<SpriteRenderer>(preview.ghost) {
        sprite_renderer.visible = true;
        sprite_renderer.width = frame.ghost_width;
        sprite_renderer.height = frame.ghost_height;
        sprite_renderer.uv = frame.ghost_uv;
        sprite_renderer.texture = Some(frame.ghost_texture);
        sprite_renderer.color = frame.ghost_color;
    }
    if let Some(sort) = world.get_mut::<SortingLayer>(preview.ghost) {
        sort.0 = frame.base_sort_layer + 8;
    }
}

fn set_sprite_visible(world: &mut World, entity: EntityId, visible: bool) {
    if let Some(sprite) = world.get_mut::<SpriteRenderer>(entity) {
        sprite.visible = visible;
    }
}

fn hide_preview(world: &mut World, preview: PreviewEntities) {
    set_sprite_visible(world, preview.hover_tile, false);
    set_sprite_visible(world, preview.ghost, false);
}

struct PreviewFrame {
    hover_position: sky_engine::math::Vec2,
    hover_color: Color,
    ghost_position: sky_engine::math::Vec2,
    ghost_width: f32,
    ghost_height: f32,
    ghost_uv: [f32; 4],
    ghost_texture: sky_engine::asset::Handle<sky_engine::asset::TextureAsset>,
    ghost_color: Color,
    base_sort_layer: i32,
}

fn preview_frame(world: &World, assets: &GameAssets) -> Option<PreviewFrame> {
    let selection = world.get_resource::<BuildSelection>()?;
    let board = world.get_resource::<BoardState>()?;
    let hover = world.get_resource::<HoverState>()?;
    let _mouse_world = hover.world_pos?;
    let (row, col) = hover.cell?;
    let center = cell_center(row, col);
    let base_sort_layer = cell_sort_layer(row, col);
    let occupied = board.cells[cell_index(row, col)].structure.is_some();
    let affordable = board.can_afford(selection.selected, row, col);
    let blueprint = &assets.blueprints[selection.selected];
    let sprite = &blueprint.rotations[selection.orientation];

    Some(PreviewFrame {
        hover_position: tiled_image_center(
            center,
            assets.hover.width * 1.06,
            assets.hover.height * 1.06,
        ),
        hover_color: if affordable {
            Color::rgba8(150, 225, 255, 125)
        } else {
            Color::rgba8(255, 105, 90, 125)
        },
        ghost_position: tiled_image_center(center, sprite.width, sprite.height),
        ghost_width: sprite.width,
        ghost_height: sprite.height,
        ghost_uv: sprite.uv,
        ghost_texture: sprite.handle.clone(),
        ghost_color: if occupied {
            Color::rgba8(255, 232, 128, 150)
        } else if affordable {
            Color::rgba8(255, 255, 255, 165)
        } else {
            Color::rgba8(255, 120, 110, 125)
        },
        base_sort_layer,
    })
}
