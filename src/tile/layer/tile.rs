use std::collections::BTreeMap;

use crate::render::{Color, TileFlags};

use super::super::grid::{CellCoord, CellRect};
use super::super::palette::{PaletteId, TileDefId};

/// Reference to one tile definition in one palette.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TileRef {
    pub palette: PaletteId,
    pub tile: TileDefId,
}

impl TileRef {
    #[inline]
    pub const fn new(palette: PaletteId, tile: TileDefId) -> Self {
        Self { palette, tile }
    }
}

impl From<(PaletteId, TileDefId)> for TileRef {
    #[inline]
    fn from(value: (PaletteId, TileDefId)) -> Self {
        Self::new(value.0, value.1)
    }
}

impl From<(u32, u32)> for TileRef {
    #[inline]
    fn from(value: (u32, u32)) -> Self {
        Self::new(PaletteId(value.0), TileDefId(value.1))
    }
}

/// Tile cell payload stored in a scene layer.
#[derive(Clone, Copy, Debug)]
pub struct SceneTile {
    pub tile_ref: TileRef,
    pub flags: TileFlags,
    pub tint: Color,
}

impl PartialEq for SceneTile {
    fn eq(&self, other: &Self) -> bool {
        self.tile_ref == other.tile_ref
            && self.flags == other.flags
            && self.tint.to_array() == other.tint.to_array()
    }
}

impl SceneTile {
    pub fn new(tile_ref: TileRef) -> Self {
        Self {
            tile_ref,
            flags: TileFlags::empty(),
            tint: Color::WHITE,
        }
    }

    pub fn tinted(tile_ref: impl Into<TileRef>, tint: Color) -> Self {
        Self {
            tile_ref: tile_ref.into(),
            flags: TileFlags::empty(),
            tint,
        }
    }

    pub fn with_flags(mut self, flags: TileFlags) -> Self {
        self.flags = flags;
        self
    }
}

impl From<TileRef> for SceneTile {
    #[inline]
    fn from(value: TileRef) -> Self {
        Self::new(value)
    }
}

impl From<(PaletteId, TileDefId)> for SceneTile {
    #[inline]
    fn from(value: (PaletteId, TileDefId)) -> Self {
        Self::new(value.into())
    }
}

impl From<(u32, u32)> for SceneTile {
    #[inline]
    fn from(value: (u32, u32)) -> Self {
        Self::new(value.into())
    }
}

/// Sparse chunk-friendly tile storage placeholder.
#[derive(Clone, Debug, Default)]
pub struct ChunkedTileData {
    tiles: BTreeMap<CellCoord, SceneTile>,
}

impl ChunkedTileData {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&mut self, cell: CellCoord, tile: Option<SceneTile>) -> Option<SceneTile> {
        match tile {
            Some(tile) => self.tiles.insert(cell, tile),
            None => self.tiles.remove(&cell),
        }
    }

    pub fn get(&self, cell: CellCoord) -> Option<SceneTile> {
        self.tiles.get(&cell).copied()
    }

    pub fn iter(&self) -> impl Iterator<Item = (CellCoord, SceneTile)> + '_ {
        self.tiles.iter().map(|(cell, tile)| (*cell, *tile))
    }

    pub fn cells_in_rect(
        &self,
        rect: CellRect,
    ) -> impl Iterator<Item = (CellCoord, SceneTile)> + '_ {
        self.iter().filter(move |(cell, _)| rect.contains(*cell))
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.tiles.is_empty()
    }
}
