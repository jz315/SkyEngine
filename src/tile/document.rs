use std::path::Path;

use super::grid::GridSpec;
use super::layer::{LayerRole, TileLayer};
use super::palette::{AssetSource, PaletteId, PropertyBag, TilePalette, TilePaletteStore};
use super::persistence::{
    apply_delta_to_document, TileDocumentRevision, TileMapDelta, TileMapSnapshot,
    TilePersistenceError,
};
use super::scene::{TileMap, TileMapId, TileMapSize};
use super::{TileMapEditHistory, TileMapEditSession, TileMapEditSummary};

/// Source authoring format for a tile scene document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TileAuthoringFormat {
    Tiled,
    Custom(String),
}

/// Metadata that belongs to an authoring document, not to runtime gameplay.
#[derive(Clone, Debug, Default)]
pub struct TileAuthoringMetadata {
    pub format: Option<TileAuthoringFormat>,
    pub source: Option<AssetSource>,
    pub properties: PropertyBag,
}

/// A complete editable tile scene document.
///
/// `TileMap` remains the runtime truth. The document wraps that runtime scene
/// with the palette store and optional authoring metadata needed for clean
/// import, export, save, and editor workflows.
#[derive(Clone, Debug)]
pub struct TileMapDocument {
    pub scene: TileMap,
    pub palettes: TilePaletteStore,
    pub authoring: TileAuthoringMetadata,
    pub revision: TileDocumentRevision,
    pub history: TileMapEditHistory,
}

impl TileMapDocument {
    pub fn new(scene: TileMap) -> Self {
        Self {
            scene,
            palettes: TilePaletteStore::new(),
            authoring: TileAuthoringMetadata::default(),
            revision: TileDocumentRevision::default(),
            history: TileMapEditHistory::new(),
        }
    }

    pub fn builder(name: impl Into<String>) -> TileMapDocumentBuilder {
        TileMapDocumentBuilder::new(name)
    }

    pub fn with_palettes(mut self, palettes: impl IntoIterator<Item = TilePalette>) -> Self {
        self.palettes.extend(palettes);
        self
    }

    pub fn with_palette_store(mut self, palettes: TilePaletteStore) -> Self {
        self.palettes = palettes;
        self
    }

    pub fn insert_palette(&mut self, palette: TilePalette) {
        self.palettes.insert(palette);
    }

    pub fn palette(&self, id: PaletteId) -> Option<&TilePalette> {
        self.palettes.get(id)
    }

    pub fn edit(&mut self, edit: impl FnOnce(&mut TileMapEditSession<'_>)) -> TileMapEditSummary {
        let mut session = TileMapEditSession::new(&mut self.scene);
        edit(&mut session);
        session.finish()
    }

    pub fn edit_recorded(
        &mut self,
        edit: impl FnOnce(&mut TileMapEditSession<'_>),
    ) -> TileMapEditSummary {
        let summary = self.edit(edit);
        if self.history.record(summary.clone()) {
            self.revision = self.revision.next();
        }
        summary
    }

    pub fn undo(&mut self) -> Option<TileMapEditSummary> {
        let summary = self.history.undo(&mut self.scene)?;
        if !summary.is_empty() {
            self.revision = self.revision.next();
        }
        Some(summary)
    }

    pub fn redo(&mut self) -> Option<TileMapEditSummary> {
        let summary = self.history.redo(&mut self.scene)?;
        if !summary.is_empty() {
            self.revision = self.revision.next();
        }
        Some(summary)
    }

    pub fn clear_history(&mut self) {
        self.history.clear();
    }

    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    pub fn to_snapshot_json_string(&self) -> Result<String, TilePersistenceError> {
        TileMapSnapshot::from_document(self)?.to_json_string()
    }

    pub fn to_snapshot_json_string_pretty(&self) -> Result<String, TilePersistenceError> {
        TileMapSnapshot::from_document(self)?.to_json_string_pretty()
    }

    pub fn from_snapshot_json_str(input: &str) -> Result<Self, TilePersistenceError> {
        TileMapSnapshot::from_json_str(input)?.into_document()
    }

    pub fn from_snapshot_json_file(path: impl AsRef<Path>) -> Result<Self, TilePersistenceError> {
        TileMapSnapshot::from_json_file(path)?.into_document()
    }

    pub fn write_snapshot_json_file(
        &self,
        path: impl AsRef<Path>,
    ) -> Result<(), TilePersistenceError> {
        TileMapSnapshot::from_document(self)?.write_json_file(path)
    }

    pub fn edit_delta(&mut self, edit: impl FnOnce(&mut TileMapEditSession<'_>)) -> TileMapDelta {
        let base_scene = self.scene.id;
        let base_revision = self.revision;
        let summary = self.edit(edit);
        if !summary.is_empty() {
            self.revision = self.revision.next();
        }
        TileMapDelta::new(base_scene, base_revision, summary)
    }

    pub fn apply_delta(
        &mut self,
        delta: &TileMapDelta,
    ) -> Result<TileMapEditSummary, TilePersistenceError> {
        apply_delta_to_document(self, delta)
    }
}

/// Builder for the common "create a scene document, add palettes and layers"
/// path used by examples, games, and editor tests.
#[derive(Clone, Debug)]
pub struct TileMapDocumentBuilder {
    id: TileMapId,
    name: String,
    grid: GridSpec,
    size: TileMapSize,
    palettes: TilePaletteStore,
    layers: Vec<TileLayer>,
}

impl TileMapDocumentBuilder {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: TileMapId(1),
            name: name.into(),
            grid: GridSpec::default(),
            size: TileMapSize::new(1, 1),
            palettes: TilePaletteStore::new(),
            layers: Vec::new(),
        }
    }

    pub fn id(mut self, id: impl Into<TileMapId>) -> Self {
        self.id = id.into();
        self
    }

    pub fn grid(mut self, grid: GridSpec) -> Self {
        self.grid = grid;
        self
    }

    pub fn orthogonal(mut self, cell_size: [u32; 2]) -> Self {
        self.grid = GridSpec::orthogonal(cell_size);
        self
    }

    pub fn isometric(mut self, cell_size: [u32; 2]) -> Self {
        self.grid = GridSpec::isometric(cell_size);
        self
    }

    pub fn size(mut self, size: impl Into<TileMapSize>) -> Self {
        self.size = size.into();
        self
    }

    pub fn palette(mut self, palette: TilePalette) -> Self {
        self.palettes.insert(palette);
        self
    }

    pub fn palettes(mut self, palettes: impl IntoIterator<Item = TilePalette>) -> Self {
        self.palettes.extend(palettes);
        self
    }

    pub fn layer(mut self, layer: TileLayer) -> Self {
        self.layers.push(layer);
        self
    }

    pub fn tile_layer(mut self, name: impl Into<String>, role: LayerRole) -> Self {
        let id = self.next_layer_id();
        self.layers.push(TileLayer::tiles(id, name, role));
        self
    }

    pub fn object_layer(mut self, name: impl Into<String>, role: LayerRole) -> Self {
        let id = self.next_layer_id();
        self.layers.push(TileLayer::objects(id, name, role));
        self
    }

    pub fn build(self) -> TileMapDocument {
        let mut scene = TileMap::new(self.id, self.name, self.grid, self.size);
        scene.layers = self.layers;
        TileMapDocument::new(scene).with_palette_store(self.palettes)
    }

    fn next_layer_id(&self) -> super::LayerId {
        super::LayerId(
            self.layers
                .iter()
                .map(|layer| layer.id.0)
                .max()
                .unwrap_or_default()
                .saturating_add(1),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tile::{
        CellCoord, GridSpec, LayerId, LayerRole, SceneTile, TileDefId, TileLayer, TileMapId,
        TileMapSize, TileRef,
    };

    #[test]
    fn document_edit_finishes_session_and_returns_summary() {
        let layer = LayerId(1);
        let mut scene = TileMap::new(
            TileMapId(1),
            "document",
            GridSpec::orthogonal([16, 16]),
            TileMapSize::new(2, 2),
        );
        scene
            .layers
            .push(TileLayer::tiles(layer, "Ground", LayerRole::Ground));
        let mut document = TileMapDocument::new(scene);
        let tile = SceneTile::new(TileRef::new(PaletteId(1), TileDefId(0)));

        let summary = document.edit(|edit| {
            edit.set_tile(layer, CellCoord::new(1, 0), Some(tile));
        });

        assert_eq!(
            document.scene.layers[0].tile(CellCoord::new(1, 0)),
            Some(tile)
        );
        assert_eq!(summary.changed_layers, vec![layer]);
        assert_eq!(summary.tile_changes.len(), 1);
        assert!(!summary.is_empty());
    }

    #[test]
    fn builder_creates_named_layers_and_easy_edits() {
        let mut document = TileMapDocument::builder("builder")
            .id(7)
            .orthogonal([16, 16])
            .size([4, 3])
            .tile_layer("Ground", LayerRole::Ground)
            .object_layer("Props", LayerRole::Props)
            .build();

        assert_eq!(document.scene.id, TileMapId(7));
        assert_eq!(document.scene.size, TileMapSize::new(4, 3));

        let ground = document
            .scene
            .layer_id_by_name("Ground")
            .expect("ground layer should exist");

        let summary = document.edit_recorded(|edit| {
            edit.set_named("Ground", [1, 2], (0, 3));
            edit.fill(ground, ([0, 0], [2, 1]), (0, 4));
        });

        assert_eq!(summary.changed_layers, vec![ground]);
        assert_eq!(
            document
                .scene
                .layer(ground)
                .and_then(|layer| layer.tile(CellCoord::new(1, 2)))
                .map(|tile| tile.tile_ref.tile),
            Some(TileDefId(3))
        );
        assert_eq!(
            document
                .scene
                .layer(ground)
                .and_then(|layer| layer.tile(CellCoord::new(0, 0)))
                .map(|tile| tile.tile_ref.tile),
            Some(TileDefId(4))
        );
    }

    #[test]
    fn document_edit_returns_empty_summary_for_noop_edit() {
        let scene = TileMap::new(
            TileMapId(1),
            "document",
            GridSpec::orthogonal([16, 16]),
            TileMapSize::new(2, 2),
        );
        let mut document = TileMapDocument::new(scene);

        let summary = document.edit(|_| {});

        assert!(summary.is_empty());
    }

    #[test]
    fn document_recorded_edit_supports_undo_and_redo() {
        let layer = LayerId(1);
        let first_tile = SceneTile::new(TileRef::new(PaletteId(1), TileDefId(1)));
        let second_tile = SceneTile::new(TileRef::new(PaletteId(1), TileDefId(2)));
        let mut scene = TileMap::new(
            TileMapId(1),
            "document",
            GridSpec::orthogonal([16, 16]),
            TileMapSize::new(2, 2),
        );
        let mut tile_layer = TileLayer::tiles(layer, "Ground", LayerRole::Ground);
        tile_layer.set_tile(CellCoord::new(0, 0), Some(first_tile));
        scene.layers.push(tile_layer);
        let mut document = TileMapDocument::new(scene);

        let edit_summary = document.edit_recorded(|edit| {
            edit.set_tile(layer, CellCoord::new(0, 0), Some(second_tile));
        });

        assert!(!edit_summary.is_empty());
        assert!(document.can_undo());
        assert!(!document.can_redo());
        assert_eq!(
            document.scene.layers[0].tile(CellCoord::new(0, 0)),
            Some(second_tile)
        );

        let undo_summary = document.undo().expect("undo summary");
        assert!(!undo_summary.is_empty());
        assert_eq!(
            document.scene.layers[0].tile(CellCoord::new(0, 0)),
            Some(first_tile)
        );
        assert!(!document.can_undo());
        assert!(document.can_redo());

        let redo_summary = document.redo().expect("redo summary");
        assert!(!redo_summary.is_empty());
        assert_eq!(
            document.scene.layers[0].tile(CellCoord::new(0, 0)),
            Some(second_tile)
        );
        assert!(document.can_undo());
        assert!(!document.can_redo());
    }

    #[test]
    fn document_recorded_edit_skips_noop_history_entries() {
        let scene = TileMap::new(
            TileMapId(1),
            "document",
            GridSpec::orthogonal([16, 16]),
            TileMapSize::new(2, 2),
        );
        let mut document = TileMapDocument::new(scene);

        let summary = document.edit_recorded(|_| {});

        assert!(summary.is_empty());
        assert!(!document.can_undo());
        assert!(document.undo().is_none());
    }
}
