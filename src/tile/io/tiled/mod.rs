mod error;
mod export;
mod import;
mod map;
mod palette;
mod properties;

#[cfg(test)]
mod tests;

use std::path::Path;

use crate::render::features::tilemap::TiledImport;
use crate::tile::{MapData, TilePalette};

pub use error::TiledImportError;
pub use export::{TiledExportError, TiledExporter};
use import::map_snapshot;

/// Imports Tiled maps into SkyEngine tile map data.
pub struct TiledImporter;

impl TiledImporter {
    pub fn load(path: impl AsRef<Path>) -> Result<(MapData, Vec<TilePalette>), TiledImportError> {
        let path = path.as_ref();
        let import = TiledImport::from_file(path).map_err(TiledImportError::from)?;
        let snapshot = map_snapshot(&import);
        Ok((
            map::import_map(&snapshot),
            palette::import_palettes(&snapshot),
        ))
    }

    #[cfg(test)]
    pub fn import_map(import: &TiledImport) -> MapData {
        map::import_map(&map_snapshot(import))
    }

    #[cfg(test)]
    pub fn import_palettes(import: &TiledImport) -> Vec<TilePalette> {
        palette::import_palettes(&map_snapshot(import))
    }
}
