use super::super::super::grid::{CellCoord, CellRect};
use super::super::super::layer::{LayerId, SceneTile};
use super::super::summary::{DirtyCell, DirtyRegion, TileChange};
use super::TileMapEditSession;

impl TileMapEditSession<'_> {
    pub fn set_tile(&mut self, layer: LayerId, cell: CellCoord, tile: Option<SceneTile>) {
        let Some(layer_ref) = self.scene.layer_mut(layer) else {
            return;
        };
        let old = layer_ref.tile(cell);
        if old == tile {
            return;
        }
        let _ = layer_ref.set_tile(cell, tile);
        self.summary.dirty_cells.push(DirtyCell { layer, cell });
        self.summary.tile_changes.push(TileChange {
            layer,
            cell,
            old,
            new: tile,
        });
        self.summary.mark_layer(layer);
    }

    pub fn fill_rect(&mut self, layer: LayerId, rect: CellRect, tile: Option<SceneTile>) {
        if rect.is_empty() {
            return;
        }
        let Some(layer_ref) = self.scene.layer_mut(layer) else {
            return;
        };
        let mut changes = Vec::new();
        for cell in rect.cells() {
            let old = layer_ref.tile(cell);
            if old == tile {
                continue;
            }
            let _ = layer_ref.set_tile(cell, tile);
            changes.push(TileChange {
                layer,
                cell,
                old,
                new: tile,
            });
        }
        if changes.is_empty() {
            return;
        }
        self.summary.dirty_regions.push(DirtyRegion { layer, rect });
        self.summary.tile_changes.extend(changes);
        self.summary.mark_layer(layer);
    }

    pub fn set(
        &mut self,
        layer: impl Into<LayerId>,
        cell: impl Into<CellCoord>,
        tile: impl Into<SceneTile>,
    ) {
        self.set_tile(layer.into(), cell.into(), Some(tile.into()));
    }

    pub fn clear(&mut self, layer: impl Into<LayerId>, cell: impl Into<CellCoord>) {
        self.set_tile(layer.into(), cell.into(), None);
    }

    pub fn fill(
        &mut self,
        layer: impl Into<LayerId>,
        rect: impl Into<CellRect>,
        tile: impl Into<SceneTile>,
    ) {
        self.fill_rect(layer.into(), rect.into(), Some(tile.into()));
    }

    pub fn clear_rect(&mut self, layer: impl Into<LayerId>, rect: impl Into<CellRect>) {
        self.fill_rect(layer.into(), rect.into(), None);
    }

    pub fn set_named(
        &mut self,
        layer: &str,
        cell: impl Into<CellCoord>,
        tile: impl Into<SceneTile>,
    ) {
        if let Some(layer) = self.scene.layer_id_by_name(layer) {
            self.set_tile(layer, cell.into(), Some(tile.into()));
        }
    }

    pub fn clear_named(&mut self, layer: &str, cell: impl Into<CellCoord>) {
        if let Some(layer) = self.scene.layer_id_by_name(layer) {
            self.set_tile(layer, cell.into(), None);
        }
    }

    pub fn fill_named(
        &mut self,
        layer: &str,
        rect: impl Into<CellRect>,
        tile: impl Into<SceneTile>,
    ) {
        if let Some(layer) = self.scene.layer_id_by_name(layer) {
            self.fill_rect(layer, rect.into(), Some(tile.into()));
        }
    }

    pub fn clear_rect_named(&mut self, layer: &str, rect: impl Into<CellRect>) {
        if let Some(layer) = self.scene.layer_id_by_name(layer) {
            self.fill_rect(layer, rect.into(), None);
        }
    }
}
