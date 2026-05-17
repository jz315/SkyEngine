mod export;
mod palette;
mod properties;
mod scene;

#[cfg(test)]
mod tests;

use std::path::Path;

use crate::render::{TiledImport, TiledImportError};
use crate::tile::{AssetSource, TileAuthoringFormat, TileMap, TileMapDocument, TilePalette};

pub use export::{TiledExportError, TiledExporter};

/// Imports Tiled maps into SkyEngine tile scene documents.
pub struct TiledImporter;

impl TiledImporter {
    pub fn load_scene(path: impl AsRef<Path>) -> Result<TileMap, TiledImportError> {
        scene::load_scene(path)
    }

    pub fn load_document(path: impl AsRef<Path>) -> Result<TileMapDocument, TiledImportError> {
        let path = path.as_ref();
        let import = TiledImport::from_file(path)?;
        let mut document = Self::import_document(&import);
        document.authoring.source = Some(AssetSource::new(path.to_path_buf()));
        Ok(document)
    }

    pub fn import_scene(import: &TiledImport) -> TileMap {
        scene::import_scene(import)
    }

    pub fn import_palettes(import: &TiledImport) -> Vec<TilePalette> {
        palette::import_palettes(import)
    }

    pub fn import_document(import: &TiledImport) -> TileMapDocument {
        let mut document = TileMapDocument::new(scene::import_scene(import))
            .with_palettes(palette::import_palettes(import));
        document.authoring.format = Some(TileAuthoringFormat::Tiled);
        document
    }
}
