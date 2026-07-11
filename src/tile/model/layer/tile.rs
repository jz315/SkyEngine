use std::collections::BTreeMap;

use super::super::color::Color;
use super::super::grid::{CellCoord, CellRect};
use super::super::palette::{PaletteId, TileDefId};

bitflags::bitflags! {
    /// Per-tile texture transform flags in the tile scene model.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct TileFlags: u8 {
        const FLIP_X = 1 << 0;
        const FLIP_Y = 1 << 1;
        const FLIP_DIAGONAL = 1 << 2;
    }
}

impl From<crate::render::features::tilemap::TileFlags> for TileFlags {
    fn from(value: crate::render::features::tilemap::TileFlags) -> Self {
        let mut flags = Self::empty();
        if value.contains(crate::render::features::tilemap::TileFlags::FLIP_X) {
            flags |= Self::FLIP_X;
        }
        if value.contains(crate::render::features::tilemap::TileFlags::FLIP_Y) {
            flags |= Self::FLIP_Y;
        }
        if value.contains(crate::render::features::tilemap::TileFlags::FLIP_DIAGONAL) {
            flags |= Self::FLIP_DIAGONAL;
        }
        flags
    }
}

impl From<TileFlags> for crate::render::features::tilemap::TileFlags {
    fn from(value: TileFlags) -> Self {
        let mut flags = Self::empty();
        if value.contains(TileFlags::FLIP_X) {
            flags |= Self::FLIP_X;
        }
        if value.contains(TileFlags::FLIP_Y) {
            flags |= Self::FLIP_Y;
        }
        if value.contains(TileFlags::FLIP_DIAGONAL) {
            flags |= Self::FLIP_DIAGONAL;
        }
        flags
    }
}

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
pub struct TileCell {
    pub tile_ref: TileRef,
    pub flags: TileFlags,
    pub tint: Color,
}

impl PartialEq for TileCell {
    fn eq(&self, other: &Self) -> bool {
        self.tile_ref == other.tile_ref
            && self.flags == other.flags
            && self.tint.to_array() == other.tint.to_array()
    }
}

impl TileCell {
    pub fn new(tile_ref: TileRef) -> Self {
        Self {
            tile_ref,
            flags: TileFlags::empty(),
            tint: Color::WHITE,
        }
    }

    pub fn tinted(tile_ref: impl Into<TileRef>, tint: impl Into<Color>) -> Self {
        Self {
            tile_ref: tile_ref.into(),
            flags: TileFlags::empty(),
            tint: tint.into(),
        }
    }

    pub fn with_flags(mut self, flags: TileFlags) -> Self {
        self.flags = flags;
        self
    }
}

impl From<TileRef> for TileCell {
    #[inline]
    fn from(value: TileRef) -> Self {
        Self::new(value)
    }
}

impl From<(PaletteId, TileDefId)> for TileCell {
    #[inline]
    fn from(value: (PaletteId, TileDefId)) -> Self {
        Self::new(value.into())
    }
}

impl From<(u32, u32)> for TileCell {
    #[inline]
    fn from(value: (u32, u32)) -> Self {
        Self::new(value.into())
    }
}

/// Sparse chunk-friendly tile storage placeholder.
#[derive(Clone, Debug, Default)]
pub struct ChunkedTileData {
    tiles: BTreeMap<CellCoord, TileCell>,
}

impl ChunkedTileData {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&mut self, cell: CellCoord, tile: Option<TileCell>) -> Option<TileCell> {
        match tile {
            Some(tile) => self.tiles.insert(cell, tile),
            None => self.tiles.remove(&cell),
        }
    }

    pub fn get(&self, cell: CellCoord) -> Option<TileCell> {
        self.tiles.get(&cell).copied()
    }

    pub fn iter(&self) -> impl Iterator<Item = (CellCoord, TileCell)> + '_ {
        self.tiles.iter().map(|(cell, tile)| (*cell, *tile))
    }

    pub fn cells_in_rect(
        &self,
        rect: CellRect,
    ) -> impl Iterator<Item = (CellCoord, TileCell)> + '_ {
        self.iter().filter(move |(cell, _)| rect.contains(*cell))
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.tiles.is_empty()
    }
}
