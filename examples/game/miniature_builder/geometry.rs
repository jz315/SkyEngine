use sky_engine::math::Vec2;

pub const WINDOW_W: u32 = 1280;
pub const WINDOW_H: u32 = 760;
pub const ORTHO_HEIGHT: f32 = 760.0;
pub const MIN_ZOOM: f32 = 520.0;
pub const MAX_ZOOM: f32 = 980.0;
pub const ROWS: usize = 11;
pub const COLS: usize = 11;
pub const TILE_PIXEL_W: u32 = 256;
pub const TILE_PIXEL_H: u32 = 128;
pub const TILE_DRAW_PIXEL_H: u32 = 512;
pub const TILE_W: f32 = TILE_PIXEL_W as f32;
pub const TILE_H: f32 = TILE_PIXEL_H as f32;
pub const ASSET_SCALE: f32 = 1.0;

pub fn map_origin_y() -> f32 {
    (ROWS as f32 + COLS as f32 - 2.0) * TILE_H * 0.25
}

pub fn cell_center(row: usize, col: usize) -> Vec2 {
    Vec2::new(
        (col as f32 - row as f32) * TILE_W * 0.5,
        (col as f32 + row as f32) * TILE_H * 0.5 - map_origin_y(),
    )
}

pub fn cell_at_world(world: Vec2) -> Option<(usize, usize)> {
    let dx = world.x();
    let dy = world.y() + map_origin_y();
    let col_f = dx / TILE_W + dy / TILE_H;
    let row_f = dy / TILE_H - dx / TILE_W;
    let row = row_f.round() as i32;
    let col = col_f.round() as i32;
    if row < 0 || col < 0 || row >= ROWS as i32 || col >= COLS as i32 {
        return None;
    }
    let center = cell_center(row as usize, col as usize);
    let local_x = (world.x() - center.x()).abs() / (TILE_W * 0.5);
    let local_y = (world.y() - center.y()).abs() / (TILE_H * 0.5);
    if local_x + local_y <= 1.18 {
        Some((row as usize, col as usize))
    } else {
        None
    }
}

pub fn cell_index(row: usize, col: usize) -> usize {
    row * COLS + col
}

pub fn cell_sort_layer(row: usize, col: usize) -> i32 {
    (row + col) as i32 * 10
}

pub fn tiled_image_center(cell: Vec2, image_width: f32, image_height: f32) -> Vec2 {
    Vec2::new(
        cell.x() - TILE_W * 0.5 + image_width * 0.5,
        cell.y() - TILE_H * 0.5 + image_height * 0.5,
    )
}

pub fn neighbors(row: usize, col: usize) -> impl Iterator<Item = (usize, usize)> {
    let mut items = [(usize::MAX, usize::MAX); 4];
    let mut len = 0;
    if row > 0 {
        items[len] = (row - 1, col);
        len += 1;
    }
    if row + 1 < ROWS {
        items[len] = (row + 1, col);
        len += 1;
    }
    if col > 0 {
        items[len] = (row, col - 1);
        len += 1;
    }
    if col + 1 < COLS {
        items[len] = (row, col + 1);
        len += 1;
    }
    items.into_iter().take(len)
}
