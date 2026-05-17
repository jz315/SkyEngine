mod build;
#[cfg(test)]
mod tests;
mod write;

use std::fmt;

use crate::render::{TilemapHandle, TilemapRenderer, TilemapStorage};

use super::layer::LayerId;
use super::palette::PaletteId;

/// One renderer-ready layer produced from a tile scene.
#[derive(Clone, Debug)]
pub struct TileMapRenderLayer {
    pub source_layer: LayerId,
    pub palette: PaletteId,
    pub storage_layer: u32,
    pub renderer: TilemapRenderer,
}

/// CPU-side tilemap render data produced from a scene snapshot.
pub struct TileMapRenderData {
    pub storage: TilemapStorage,
    pub map: TilemapHandle,
    pub layers: Vec<TileMapRenderLayer>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TileMapRenderSyncError {
    MissingPalette(PaletteId),
    MissingTexture(PaletteId),
}

impl fmt::Display for TileMapRenderSyncError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingPalette(id) => write!(f, "tile scene palette {:?} is missing", id),
            Self::MissingTexture(id) => {
                write!(f, "tile scene palette {:?} does not have a texture", id)
            }
        }
    }
}

impl std::error::Error for TileMapRenderSyncError {}

/// Converts a tile scene snapshot into low-level tilemap storage and renderer
/// components. Layers using multiple palettes are split by palette.
pub struct TileMapRenderSync;
