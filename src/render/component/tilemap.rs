use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crate::asset::{Handle, TextureAsset};
use crate::math::{Transform, Vec3};
use crate::render::tilemap::{TileId, TilemapHandle};
use crate::render::Color;

/// One frame in a tile animation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TileAnimationFrame {
    pub tile_id: TileId,
    pub duration_ms: u32,
}

impl TileAnimationFrame {
    #[inline]
    pub const fn new(tile_id: TileId, duration_ms: u32) -> Self {
        Self {
            tile_id,
            duration_ms,
        }
    }
}

/// Animation sequence attached to one tile in a tileset.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TileAnimation {
    pub tile_id: TileId,
    pub frames: Vec<TileAnimationFrame>,
    pub duration_ms: u32,
}

impl TileAnimation {
    pub fn new(tile_id: TileId, frames: impl Into<Vec<TileAnimationFrame>>) -> Self {
        let frames = frames.into();
        let duration_ms = frames
            .iter()
            .map(|frame| frame.duration_ms.max(1))
            .sum::<u32>()
            .max(1);
        Self {
            tile_id,
            frames,
            duration_ms,
        }
    }

    pub fn frame_at(&self, elapsed_seconds: f32) -> TileId {
        if self.frames.is_empty() {
            return self.tile_id;
        }

        let elapsed_ms = (elapsed_seconds.max(0.0) * 1000.0) as u64;
        let mut phase = (elapsed_ms % self.duration_ms as u64) as u32;
        for frame in &self.frames {
            let duration = frame.duration_ms.max(1);
            if phase < duration {
                return frame.tile_id;
            }
            phase -= duration;
        }
        self.frames
            .last()
            .map_or(self.tile_id, |frame| frame.tile_id)
    }
}

/// Pixel rectangle of one tile inside a tileset texture.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TilesetTileRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl TilesetTileRect {
    #[inline]
    pub const fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    #[inline]
    pub const fn size(self) -> [u32; 2] {
        [self.width, self.height]
    }
}

/// Fixed-grid or per-tile texture atlas used by a tilemap.
#[derive(Clone, Debug)]
pub struct TilesetGrid {
    pub texture: Handle<TextureAsset>,
    pub tile_size: [u32; 2],
    pub columns: u32,
    pub rows: u32,
    pub texture_size: [u32; 2],
    pub margin: u32,
    pub spacing: u32,
    pub animations: Vec<TileAnimation>,
    pub tile_rects: Vec<Option<TilesetTileRect>>,
}

impl TilesetGrid {
    #[inline]
    pub fn new(
        texture: Handle<TextureAsset>,
        tile_size: [u32; 2],
        columns: u32,
        rows: u32,
    ) -> Self {
        let tile_size = [tile_size[0].max(1), tile_size[1].max(1)];
        let columns = columns.max(1);
        let rows = rows.max(1);
        Self {
            texture,
            tile_size,
            columns,
            rows,
            texture_size: grid_texture_size(tile_size, columns, rows, 0, 0),
            margin: 0,
            spacing: 0,
            animations: Vec::new(),
            tile_rects: Vec::new(),
        }
    }

    #[inline]
    pub fn texture_size(mut self, texture_size: [u32; 2]) -> Self {
        self.texture_size = [texture_size[0].max(1), texture_size[1].max(1)];
        self
    }

    #[inline]
    pub fn margin(self, margin: u32) -> Self {
        let spacing = self.spacing;
        self.margin_spacing(margin, spacing)
    }

    #[inline]
    pub fn spacing(self, spacing: u32) -> Self {
        let margin = self.margin;
        self.margin_spacing(margin, spacing)
    }

    #[inline]
    pub fn margin_spacing(mut self, margin: u32, spacing: u32) -> Self {
        let old_default = grid_texture_size(
            self.tile_size,
            self.columns,
            self.rows,
            self.margin,
            self.spacing,
        );
        self.margin = margin;
        self.spacing = spacing;
        if self.texture_size == old_default {
            self.texture_size =
                grid_texture_size(self.tile_size, self.columns, self.rows, margin, spacing);
        }
        self
    }

    #[inline]
    pub fn animations(mut self, animations: impl Into<Vec<TileAnimation>>) -> Self {
        self.animations = animations.into();
        self
    }

    #[inline]
    pub fn tile_rects(mut self, tile_rects: impl Into<Vec<Option<TilesetTileRect>>>) -> Self {
        self.tile_rects = tile_rects.into();
        self
    }

    #[inline]
    pub fn tile_count(&self) -> u32 {
        if self.tile_rects.is_empty() {
            self.columns.saturating_mul(self.rows)
        } else {
            self.tile_rects.len() as u32
        }
    }

    pub fn uv_rect(&self, tile_id: TileId) -> Option<[f32; 4]> {
        if tile_id.is_empty() || tile_id.0 >= self.tile_count() {
            return None;
        }

        if !self.tile_rects.is_empty() {
            let rect = self
                .tile_rects
                .get(tile_id.0 as usize)
                .and_then(|rect| *rect)?;
            let inv_width = 1.0 / self.texture_size[0] as f32;
            let inv_height = 1.0 / self.texture_size[1] as f32;
            let x_min = rect.x as f32;
            let y_min = rect.y as f32;
            let x_max = x_min + rect.width as f32;
            let y_max = y_min + rect.height as f32;
            if x_max > self.texture_size[0] as f32 || y_max > self.texture_size[1] as f32 {
                return None;
            }
            return Some([
                x_min * inv_width,
                y_min * inv_height,
                x_max * inv_width,
                y_max * inv_height,
            ]);
        }

        let column = tile_id.0 % self.columns;
        let row = tile_id.0 / self.columns;
        let inv_width = 1.0 / self.texture_size[0] as f32;
        let inv_height = 1.0 / self.texture_size[1] as f32;
        let stride_x = self.tile_size[0].saturating_add(self.spacing);
        let stride_y = self.tile_size[1].saturating_add(self.spacing);
        let x_min = self.margin.saturating_add(column.saturating_mul(stride_x)) as f32;
        let y_min = self.margin.saturating_add(row.saturating_mul(stride_y)) as f32;
        let x_max = x_min + self.tile_size[0] as f32;
        let y_max = y_min + self.tile_size[1] as f32;
        if x_max > self.texture_size[0] as f32 || y_max > self.texture_size[1] as f32 {
            return None;
        }
        Some([
            x_min * inv_width,
            y_min * inv_height,
            x_max * inv_width,
            y_max * inv_height,
        ])
    }

    pub fn tile_draw_size(&self, tile_id: TileId) -> Option<[u32; 2]> {
        if tile_id.is_empty() || tile_id.0 >= self.tile_count() {
            return None;
        }
        if self.tile_rects.is_empty() {
            Some(self.tile_size)
        } else {
            self.tile_rects
                .get(tile_id.0 as usize)
                .and_then(|rect| rect.map(TilesetTileRect::size))
        }
    }

    pub fn animated_tile_id(&self, tile_id: TileId, elapsed_seconds: f32) -> TileId {
        self.animations
            .iter()
            .find(|animation| animation.tile_id == tile_id)
            .map_or(tile_id, |animation| animation.frame_at(elapsed_seconds))
    }

    pub fn animation_frame_key(&self, elapsed_seconds: f32) -> u64 {
        if self.animations.is_empty() {
            return 0;
        }
        let mut hasher = DefaultHasher::new();
        for animation in &self.animations {
            animation.tile_id.hash(&mut hasher);
            animation.frame_at(elapsed_seconds).hash(&mut hasher);
        }
        hasher.finish()
    }
}

fn grid_texture_size(
    tile_size: [u32; 2],
    columns: u32,
    rows: u32,
    margin: u32,
    spacing: u32,
) -> [u32; 2] {
    let columns = columns.max(1);
    let rows = rows.max(1);
    [
        margin
            .saturating_mul(2)
            .saturating_add(tile_size[0].saturating_mul(columns))
            .saturating_add(spacing.saturating_mul(columns.saturating_sub(1)))
            .max(1),
        margin
            .saturating_mul(2)
            .saturating_add(tile_size[1].saturating_mul(rows))
            .saturating_add(spacing.saturating_mul(rows.saturating_sub(1)))
            .max(1),
    ]
}

/// Axis shifted by staggered tilemap layouts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TilemapStaggerAxis {
    X,
    #[default]
    Y,
}

/// Row or column parity shifted by staggered tilemap layouts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TilemapStaggerIndex {
    #[default]
    Odd,
    Even,
}

impl TilemapStaggerIndex {
    #[inline]
    pub const fn inverted(self) -> Self {
        match self {
            Self::Odd => Self::Even,
            Self::Even => Self::Odd,
        }
    }

    #[inline]
    pub const fn matches(self, coordinate: i32) -> bool {
        let odd = coordinate & 1 != 0;
        match self {
            Self::Odd => odd,
            Self::Even => !odd,
        }
    }
}

/// Tilemap grid-to-world projection.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TilemapOrientation {
    /// Regular axis-aligned grid.
    #[default]
    Orthogonal,
    /// 45-degree isometric diamond layout.
    ///
    /// Tiles are still drawn as rectangular atlas sprites; use tileset images
    /// with transparent corners for classic diamond tiles.
    Isometric,
    /// Staggered isometric layout.
    ///
    /// The shifted axis and parity are configured by
    /// [`TilemapRenderer::stagger_axis`] and [`TilemapRenderer::stagger_index`].
    Staggered,
    /// Hexagonal layout using Tiled-style staggered rows or columns.
    Hexagonal,
}

/// Orthogonal tile draw order matching Tiled's `renderorder` values.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TilemapRenderOrder {
    #[default]
    RightDown,
    RightUp,
    LeftDown,
    LeftUp,
}

/// Tile ordering strategy used within transparent tilemap rendering.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TilemapDepthSort {
    /// Draw the renderer's selected layer as a normal tile layer.
    #[default]
    Layer,
    /// Sort individual tiles by map render order, then by layer.
    ///
    /// This is useful for Tiled maps made from several overhanging wall
    /// layers where foreground cells must cover higher layers behind them.
    YThenLayer,
}

/// ECS-facing tilemap renderer component.
#[derive(Clone, Debug)]
pub struct TilemapRenderer {
    pub map: TilemapHandle,
    pub layer: u32,
    pub tileset: TilesetGrid,
    pub tile_size: [f32; 2],
    pub tile_draw_size: [f32; 2],
    pub tile_offset: [f32; 2],
    pub orientation: TilemapOrientation,
    pub stagger_axis: TilemapStaggerAxis,
    pub stagger_index: TilemapStaggerIndex,
    pub hex_side_length: f32,
    pub render_order: TilemapRenderOrder,
    pub depth_sort: TilemapDepthSort,
    pub color: Color,
    pub visible: bool,
    pub cache_prewarm: bool,
    pub layer_mask: u32,
}

impl TilemapRenderer {
    #[inline]
    pub fn new(map: TilemapHandle, tileset: TilesetGrid) -> Self {
        Self {
            map,
            layer: 0,
            tile_size: [
                tileset.tile_size[0].max(1) as f32,
                tileset.tile_size[1].max(1) as f32,
            ],
            tile_draw_size: [
                tileset.tile_size[0].max(1) as f32,
                tileset.tile_size[1].max(1) as f32,
            ],
            tile_offset: [0.0, 0.0],
            orientation: TilemapOrientation::Orthogonal,
            stagger_axis: TilemapStaggerAxis::Y,
            stagger_index: TilemapStaggerIndex::Odd,
            hex_side_length: 0.0,
            render_order: TilemapRenderOrder::RightDown,
            depth_sort: TilemapDepthSort::Layer,
            tileset,
            color: Color::WHITE,
            visible: true,
            cache_prewarm: false,
            layer_mask: u32::MAX,
        }
    }

    #[inline]
    pub fn layer(mut self, layer: u32) -> Self {
        self.layer = layer;
        self
    }

    #[inline]
    pub fn render_order(mut self, render_order: TilemapRenderOrder) -> Self {
        self.render_order = render_order;
        self
    }

    #[inline]
    pub fn depth_sort(mut self, depth_sort: TilemapDepthSort) -> Self {
        self.depth_sort = depth_sort;
        self
    }

    #[inline]
    pub fn y_then_layer_sort(mut self) -> Self {
        self.depth_sort = TilemapDepthSort::YThenLayer;
        self
    }

    #[inline]
    pub fn tile_size(mut self, tile_size: [f32; 2]) -> Self {
        self.tile_size = [
            tile_size[0].max(f32::EPSILON),
            tile_size[1].max(f32::EPSILON),
        ];
        self
    }

    #[inline]
    pub fn tile_draw_size(mut self, tile_draw_size: [f32; 2]) -> Self {
        self.tile_draw_size = [
            tile_draw_size[0].max(f32::EPSILON),
            tile_draw_size[1].max(f32::EPSILON),
        ];
        self
    }

    #[inline]
    pub fn tile_offset(mut self, tile_offset: [f32; 2]) -> Self {
        self.tile_offset = tile_offset;
        self
    }

    #[inline]
    pub fn orientation(mut self, orientation: TilemapOrientation) -> Self {
        self.orientation = orientation;
        self
    }

    #[inline]
    pub fn isometric(mut self) -> Self {
        self.orientation = TilemapOrientation::Isometric;
        self
    }

    #[inline]
    pub fn staggered(mut self) -> Self {
        self.orientation = TilemapOrientation::Staggered;
        self
    }

    #[inline]
    pub fn hexagonal(mut self) -> Self {
        self.orientation = TilemapOrientation::Hexagonal;
        self
    }

    #[inline]
    pub fn stagger_axis(mut self, stagger_axis: TilemapStaggerAxis) -> Self {
        self.stagger_axis = stagger_axis;
        self
    }

    #[inline]
    pub fn stagger_index(mut self, stagger_index: TilemapStaggerIndex) -> Self {
        self.stagger_index = stagger_index;
        self
    }

    #[inline]
    pub fn hex_side_length(mut self, hex_side_length: f32) -> Self {
        self.hex_side_length = hex_side_length.max(0.0);
        self
    }

    #[inline]
    pub fn stagger(
        mut self,
        stagger_axis: TilemapStaggerAxis,
        stagger_index: TilemapStaggerIndex,
    ) -> Self {
        self.stagger_axis = stagger_axis;
        self.stagger_index = stagger_index;
        self
    }

    #[inline]
    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    #[inline]
    pub fn visible(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }

    #[inline]
    pub fn cache_prewarm(mut self, cache_prewarm: bool) -> Self {
        self.cache_prewarm = cache_prewarm;
        self
    }

    #[inline]
    pub fn layer_mask(mut self, layer_mask: u32) -> Self {
        self.layer_mask = layer_mask;
        self
    }

    #[inline]
    pub fn cell_to_local_center(&self, cell: [i32; 2]) -> [f32; 2] {
        let x = cell[0] as f32;
        let y = cell[1] as f32;
        let [tile_w, tile_h] = self.tile_size;
        match self.orientation {
            TilemapOrientation::Orthogonal => {
                [x * tile_w + tile_w * 0.5, y * tile_h + tile_h * 0.5]
            }
            TilemapOrientation::Isometric => [(x - y) * tile_w * 0.5, (x + y) * tile_h * 0.5],
            TilemapOrientation::Staggered => {
                let origin = self.staggered_or_hexagonal_cell_to_local_origin(cell);
                [origin[0] + tile_w * 0.5, origin[1] + tile_h * 0.5]
            }
            TilemapOrientation::Hexagonal => {
                let origin = self.staggered_or_hexagonal_cell_to_local_origin(cell);
                [origin[0] + tile_w * 0.5, origin[1] + tile_h * 0.5]
            }
        }
    }

    #[inline]
    pub fn cell_to_local_origin(&self, cell: [i32; 2]) -> [f32; 2] {
        let [tile_w, tile_h] = self.tile_size;
        match self.orientation {
            TilemapOrientation::Orthogonal => [cell[0] as f32 * tile_w, cell[1] as f32 * tile_h],
            TilemapOrientation::Isometric => {
                let center = self.cell_to_local_center(cell);
                [center[0] - tile_w * 0.5, center[1] - tile_h * 0.5]
            }
            TilemapOrientation::Staggered | TilemapOrientation::Hexagonal => {
                self.staggered_or_hexagonal_cell_to_local_origin(cell)
            }
        }
    }

    #[inline]
    pub fn cell_to_world_center(&self, cell: [i32; 2], transform: Transform) -> [f32; 2] {
        let center = self.cell_to_local_center(cell);
        let world = transform.transform_point(Vec3::new(center[0], center[1], 0.0));
        [world.x(), world.y()]
    }

    pub fn world_to_cell(&self, world: [f32; 2], transform: Transform) -> [i32; 2] {
        let local = transform
            .to_matrix4()
            .inverse()
            .transform_point3(Vec3::new(world[0], world[1], 0.0));
        let [tile_w, tile_h] = self.tile_size;
        match self.orientation {
            TilemapOrientation::Orthogonal => [
                (local.x() / tile_w.max(f32::EPSILON)).floor() as i32,
                (local.y() / tile_h.max(f32::EPSILON)).floor() as i32,
            ],
            TilemapOrientation::Isometric => {
                let gx = local.x() / (tile_w.max(f32::EPSILON) * 0.5);
                let gy = local.y() / (tile_h.max(f32::EPSILON) * 0.5);
                [
                    ((gy + gx) * 0.5).floor() as i32,
                    ((gy - gx) * 0.5).floor() as i32,
                ]
            }
            TilemapOrientation::Staggered | TilemapOrientation::Hexagonal => {
                match self.stagger_axis {
                    TilemapStaggerAxis::Y => {
                        let row_height = self.hexagonal_row_height().max(f32::EPSILON);
                        let row = (local.y() / row_height).floor() as i32;
                        let shift = if self.stagger_index.matches(row) {
                            self.hexagonal_column_width()
                        } else {
                            0.0
                        };
                        [
                            ((local.x() - shift) / tile_w.max(f32::EPSILON)).floor() as i32,
                            row,
                        ]
                    }
                    TilemapStaggerAxis::X => {
                        let column_width = self.hexagonal_column_width().max(f32::EPSILON);
                        let column = (local.x() / column_width).floor() as i32;
                        let shift = if self.stagger_index.matches(column) {
                            self.hexagonal_row_height()
                        } else {
                            0.0
                        };
                        [
                            column,
                            ((local.y() - shift) / tile_h.max(f32::EPSILON)).floor() as i32,
                        ]
                    }
                }
            }
        }
    }

    #[inline]
    fn staggered_or_hexagonal_cell_to_local_origin(&self, cell: [i32; 2]) -> [f32; 2] {
        let [tile_w, tile_h] = self.tile_size;
        let column_width = self.hexagonal_column_width();
        let row_height = self.hexagonal_row_height();
        match self.stagger_axis {
            TilemapStaggerAxis::Y => {
                let shift_x = if self.stagger_index.matches(cell[1]) {
                    column_width
                } else {
                    0.0
                };
                [
                    cell[0] as f32 * tile_w + shift_x,
                    cell[1] as f32 * row_height,
                ]
            }
            TilemapStaggerAxis::X => {
                let shift_y = if self.stagger_index.matches(cell[0]) {
                    row_height
                } else {
                    0.0
                };
                [
                    cell[0] as f32 * column_width,
                    cell[1] as f32 * tile_h + shift_y,
                ]
            }
        }
    }

    #[inline]
    fn hexagonal_column_width(&self) -> f32 {
        let tile_w = self.tile_size[0];
        match self.orientation {
            TilemapOrientation::Hexagonal if self.stagger_axis == TilemapStaggerAxis::X => {
                (tile_w + self.hex_side_length.min(tile_w)) * 0.5
            }
            _ => tile_w * 0.5,
        }
    }

    #[inline]
    fn hexagonal_row_height(&self) -> f32 {
        let tile_h = self.tile_size[1];
        match self.orientation {
            TilemapOrientation::Hexagonal if self.stagger_axis == TilemapStaggerAxis::Y => {
                (tile_h + self.hex_side_length.min(tile_h)) * 0.5
            }
            _ => tile_h * 0.5,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::asset::{AssetId, Handle, TextureAsset};
    use crate::math::Transform;
    use crate::render::tilemap::TilemapHandle;

    use super::{
        TileAnimation, TileAnimationFrame, TilemapDepthSort, TilemapOrientation,
        TilemapRenderOrder, TilemapRenderer, TilemapStaggerAxis, TilemapStaggerIndex, TilesetGrid,
    };

    fn renderer(orientation: TilemapOrientation) -> TilemapRenderer {
        TilemapRenderer::new(
            TilemapHandle::new(0, 0),
            TilesetGrid::new(Handle::<TextureAsset>::new(AssetId::new()), [64, 32], 1, 1),
        )
        .orientation(orientation)
    }

    #[test]
    fn isometric_cells_project_to_diamond_axes() {
        let renderer = renderer(TilemapOrientation::Isometric);

        assert_eq!(renderer.cell_to_local_center([1, 0]), [32.0, 16.0]);
        assert_eq!(renderer.cell_to_local_center([0, 1]), [-32.0, 16.0]);
    }

    #[test]
    fn isometric_world_to_cell_inverts_cell_centers() {
        let renderer = renderer(TilemapOrientation::Isometric);

        assert_eq!(
            renderer.world_to_cell([32.0, 16.0], Transform::default()),
            [1, 0]
        );
        assert_eq!(
            renderer.world_to_cell([-32.0, 16.0], Transform::default()),
            [0, 1]
        );
    }

    #[test]
    fn staggered_y_odd_cells_shift_odd_rows() {
        let renderer = renderer(TilemapOrientation::Staggered)
            .stagger(TilemapStaggerAxis::Y, TilemapStaggerIndex::Odd);

        assert_eq!(renderer.cell_to_local_origin([0, 0]), [0.0, 0.0]);
        assert_eq!(renderer.cell_to_local_origin([0, 1]), [32.0, 16.0]);
        assert_eq!(
            renderer.world_to_cell([32.0, 16.0], Transform::default()),
            [0, 1]
        );
    }

    #[test]
    fn hexagonal_y_odd_cells_use_hex_side_length_for_row_stride() {
        let renderer = renderer(TilemapOrientation::Hexagonal)
            .tile_size([14.0, 12.0])
            .tile_draw_size([18.0, 18.0])
            .hex_side_length(6.0)
            .stagger(TilemapStaggerAxis::Y, TilemapStaggerIndex::Odd);

        assert_eq!(renderer.cell_to_local_origin([0, 0]), [0.0, 0.0]);
        assert_eq!(renderer.cell_to_local_origin([0, 1]), [7.0, 9.0]);
        assert_eq!(renderer.cell_to_local_origin([1, 1]), [21.0, 9.0]);
    }

    #[test]
    fn layer_selects_tilemap_data_layer() {
        let renderer = renderer(TilemapOrientation::Orthogonal).layer(3);

        assert_eq!(renderer.layer, 3);
    }

    #[test]
    fn render_order_builder_sets_tile_iteration_order() {
        let renderer =
            renderer(TilemapOrientation::Orthogonal).render_order(TilemapRenderOrder::LeftUp);

        assert_eq!(renderer.render_order, TilemapRenderOrder::LeftUp);
    }

    #[test]
    fn depth_sort_builder_sets_tile_sorting_mode() {
        let renderer = renderer(TilemapOrientation::Orthogonal).y_then_layer_sort();

        assert_eq!(renderer.depth_sort, TilemapDepthSort::YThenLayer);
    }

    #[test]
    fn tileset_grid_uses_texture_pixel_size_for_uvs() {
        let grid = TilesetGrid::new(Handle::<TextureAsset>::new(AssetId::new()), [24, 24], 8, 9)
            .texture_size([192, 217]);

        let uv = grid.uv_rect(crate::render::tilemap::TileId(71)).unwrap();

        assert_eq!(uv[0], 168.0 / 192.0);
        assert_eq!(uv[1], 192.0 / 217.0);
        assert_eq!(uv[2], 1.0);
        assert_eq!(uv[3], 216.0 / 217.0);
    }

    #[test]
    fn tileset_grid_applies_tiled_margin_and_spacing_to_uvs() {
        let grid = TilesetGrid::new(Handle::<TextureAsset>::new(AssetId::new()), [32, 32], 8, 6)
            .margin_spacing(1, 1);

        let uv = grid.uv_rect(crate::render::tilemap::TileId(9)).unwrap();

        assert_eq!(grid.texture_size, [265, 199]);
        for (actual, expected) in
            uv.into_iter()
                .zip([34.0 / 265.0, 34.0 / 199.0, 66.0 / 265.0, 66.0 / 199.0])
        {
            assert!((actual - expected).abs() <= 1e-6);
        }
    }

    #[test]
    fn tileset_grid_maps_animated_tile_by_elapsed_time() {
        let grid = TilesetGrid::new(Handle::<TextureAsset>::new(AssetId::new()), [16, 16], 4, 1)
            .animations([TileAnimation::new(
                crate::render::tilemap::TileId(1),
                [
                    TileAnimationFrame::new(crate::render::tilemap::TileId(1), 100),
                    TileAnimationFrame::new(crate::render::tilemap::TileId(2), 100),
                    TileAnimationFrame::new(crate::render::tilemap::TileId(3), 200),
                ],
            )]);

        assert_eq!(
            grid.animated_tile_id(crate::render::tilemap::TileId(1), 0.05),
            crate::render::tilemap::TileId(1)
        );
        assert_eq!(
            grid.animated_tile_id(crate::render::tilemap::TileId(1), 0.15),
            crate::render::tilemap::TileId(2)
        );
        assert_eq!(
            grid.animated_tile_id(crate::render::tilemap::TileId(1), 0.30),
            crate::render::tilemap::TileId(3)
        );
        assert_eq!(
            grid.animated_tile_id(crate::render::tilemap::TileId(1), 0.45),
            crate::render::tilemap::TileId(1)
        );
        assert_ne!(
            grid.animation_frame_key(0.05),
            grid.animation_frame_key(0.15)
        );
    }
}
