use std::collections::BTreeMap;

use super::super::{Tile, TileFlags, TileId, TilemapDepthSort, TilemapOrientation};
use super::error::TiledImportError;
use super::types::{TiledProperty, TiledTileset};

pub(super) const FLIPPED_HORIZONTALLY_FLAG: u32 = 0x8000_0000;
pub(super) const FLIPPED_VERTICALLY_FLAG: u32 = 0x4000_0000;
pub(super) const FLIPPED_DIAGONALLY_FLAG: u32 = 0x2000_0000;
const ROTATED_HEXAGONAL_120_FLAG: u32 = 0x1000_0000;
const GID_MASK: u32 = !(FLIPPED_HORIZONTALLY_FLAG
    | FLIPPED_VERTICALLY_FLAG
    | FLIPPED_DIAGONALLY_FLAG
    | ROTATED_HEXAGONAL_120_FLAG);

#[derive(Clone, Copy)]
pub(super) struct LayerContext {
    pub(super) visible: bool,
    pub(super) opacity: f32,
    pub(super) offset: [f32; 2],
    pub(super) parallax: [f32; 2],
}

impl Default for LayerContext {
    fn default() -> Self {
        Self {
            visible: true,
            opacity: 1.0,
            offset: [0.0, 0.0],
            parallax: [1.0, 1.0],
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct ParsedLayer {
    pub(super) name: String,
    pub(super) source_order: i32,
    pub(super) visible: bool,
    pub(super) opacity: f32,
    pub(super) offset: [f32; 2],
    pub(super) parallax: [f32; 2],
    pub(super) properties: Vec<TiledProperty>,
    pub(super) cells: Vec<RawCell>,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct RawCell {
    pub(super) x: i32,
    pub(super) y: i32,
    pub(super) gid: u32,
}

pub(super) struct LayerSplit {
    pub(super) source_layer: usize,
    pub(super) tileset_index: usize,
    pub(super) split_order: usize,
    pub(super) cells: Vec<RawCell>,
}

pub(super) fn split_layers_by_tileset(
    layers: &[ParsedLayer],
    tilesets: &[TiledTileset],
) -> Result<Vec<LayerSplit>, TiledImportError> {
    let mut split_layers = Vec::new();
    for (source_layer, layer) in layers.iter().enumerate() {
        let mut cells_by_tileset: BTreeMap<usize, Vec<RawCell>> = BTreeMap::new();
        for &cell in &layer.cells {
            let gid = gid_without_flags(cell.gid);
            if gid == 0 {
                continue;
            }
            let tileset_index = tileset_index_for_gid(gid, tilesets)?;
            cells_by_tileset
                .entry(tileset_index)
                .or_default()
                .push(cell);
        }
        for (split_order, (tileset_index, cells)) in cells_by_tileset.into_iter().enumerate() {
            split_layers.push(LayerSplit {
                source_layer,
                tileset_index,
                split_order,
                cells,
            });
        }
    }
    Ok(split_layers)
}

pub(super) fn tileset_index_for_gid(
    gid: u32,
    tilesets: &[TiledTileset],
) -> Result<usize, TiledImportError> {
    tilesets
        .iter()
        .position(|tileset| gid_in_tileset(gid, tileset))
        .ok_or(TiledImportError::TileGidOutOfRange { gid })
}

pub(super) fn tiled_depth_sort_for_layers(
    orientation: TilemapOrientation,
    map_tile_size: [u32; 2],
    tilesets: &[TiledTileset],
    layer_count: usize,
) -> TilemapDepthSort {
    if orientation == TilemapOrientation::Orthogonal
        && layer_count > 1
        && tilesets.iter().any(|tileset| {
            tileset.tile_size[0] > map_tile_size[0]
                || tileset.tile_size[1] > map_tile_size[1]
                || tileset.tile_offset != [0, 0]
        })
    {
        TilemapDepthSort::YThenLayer
    } else {
        TilemapDepthSort::Layer
    }
}

pub(super) fn tilemap_bounds(
    infinite: bool,
    width: u32,
    height: u32,
    layers: &[ParsedLayer],
) -> ((i32, i32), u32, u32) {
    if !infinite {
        return ((0, 0), width.max(1), height.max(1));
    }

    let mut min_x = i32::MAX;
    let mut min_y = i32::MAX;
    let mut max_x = i32::MIN;
    let mut max_y = i32::MIN;
    for layer in layers {
        for cell in &layer.cells {
            min_x = min_x.min(cell.x);
            min_y = min_y.min(cell.y);
            max_x = max_x.max(cell.x);
            max_y = max_y.max(cell.y);
        }
    }
    if min_x == i32::MAX {
        return ((0, 0), 1, 1);
    }
    let width = (max_x - min_x + 1).max(1) as u32;
    let height = (max_y - min_y + 1).max(1) as u32;
    ((min_x, min_y), width, height)
}

pub(super) fn decode_cell(
    cell: RawCell,
    tileset: &TiledTileset,
    orientation: TilemapOrientation,
    origin: (i32, i32),
    tilemap_width: u32,
    tilemap_height: u32,
) -> Result<Option<(Tile, u32, u32)>, TiledImportError> {
    let gid = gid_without_flags(cell.gid);
    if gid == 0 {
        return Ok(None);
    }
    if !gid_in_tileset(gid, tileset) {
        return Err(TiledImportError::TileGidOutOfRange { gid });
    }

    let flags = tile_flags_from_gid(cell.gid);

    let x = (cell.x - origin.0) as u32;
    let tiled_y = (cell.y - origin.1) as u32;
    let (x, y) = match orientation {
        TilemapOrientation::Isometric => (
            tilemap_height.saturating_sub(1).saturating_sub(tiled_y),
            tilemap_width.saturating_sub(1).saturating_sub(x),
        ),
        _ => (x, tilemap_height.saturating_sub(1).saturating_sub(tiled_y)),
    };
    let tile = Tile::new(TileId(gid - tileset.first_gid)).with_flags(flags);
    Ok(Some((tile, x, y)))
}

#[inline]
pub(super) fn gid_without_flags(gid: u32) -> u32 {
    gid & GID_MASK
}

pub(super) fn tile_flags_from_gid(gid: u32) -> TileFlags {
    let mut flags = TileFlags::empty();
    if gid & FLIPPED_HORIZONTALLY_FLAG != 0 {
        flags |= TileFlags::FLIP_X;
    }
    if gid & FLIPPED_VERTICALLY_FLAG != 0 {
        flags |= TileFlags::FLIP_Y;
    }
    if gid & FLIPPED_DIAGONALLY_FLAG != 0 {
        flags |= TileFlags::FLIP_DIAGONAL;
    }
    flags
}

#[inline]
fn gid_in_tileset(gid: u32, tileset: &TiledTileset) -> bool {
    gid >= tileset.first_gid && gid < tileset.first_gid.saturating_add(tileset.tile_count)
}
