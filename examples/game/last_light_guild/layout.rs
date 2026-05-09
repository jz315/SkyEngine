use sky_engine::render::Transform;

use crate::components::GridPos;

pub const TILE: f32 = 28.0;
pub const ORTHO_HEIGHT: f32 = 720.0;
pub const WINDOW_W: u32 = 1280;
pub const WINDOW_H: u32 = 720;
pub const GRID_W: i32 = 34;
pub const GRID_H: i32 = 20;

pub fn grid_to_world(pos: GridPos, z: f32) -> Transform {
    let origin_x = -(GRID_W as f32) * TILE * 0.5 + TILE * 0.5;
    let origin_y = (GRID_H as f32) * TILE * 0.5 - TILE * 0.5;
    Transform::from_xyz(
        origin_x + pos.x as f32 * TILE,
        origin_y - pos.y as f32 * TILE,
        z,
    )
}

pub fn tilemap_transform(z: f32) -> Transform {
    Transform::from_xyz(
        -(GRID_W as f32) * TILE * 0.5,
        (GRID_H as f32) * TILE * 0.5,
        z,
    )
    .with_scale(1.0, -1.0)
}

pub fn room_slots(index: i32, base_x: i32, base_y: i32, columns: i32) -> GridPos {
    GridPos {
        x: base_x + (index % columns) * 2,
        y: base_y + (index / columns) * 2,
    }
}
