use super::super::grid::{CellCoord, CellRect};
use super::super::layer::{LayerId, SceneTile};
use super::super::object::{ObjectVisual, TileObject, TileObjectId};
use super::super::palette::PropertyValue;

/// Dirty cell recorded by an edit session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirtyCell {
    pub layer: LayerId,
    pub cell: CellCoord,
}

/// Dirty rectangle recorded by an edit session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirtyRegion {
    pub layer: LayerId,
    pub rect: CellRect,
}

/// Cell payload mutation record.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TileChange {
    pub layer: LayerId,
    pub cell: CellCoord,
    pub old: Option<SceneTile>,
    pub new: Option<SceneTile>,
}

/// Object create/remove/change record.
#[derive(Clone, Debug, PartialEq)]
pub enum ObjectChange {
    Created {
        object: TileObject,
    },
    Removed {
        object: TileObject,
    },
    Moved {
        id: TileObjectId,
        from_layer: LayerId,
        to_layer: LayerId,
        from_cell: CellCoord,
        to_cell: CellCoord,
    },
    VisualChanged {
        id: TileObjectId,
        old: ObjectVisual,
        new: ObjectVisual,
    },
}

/// Property mutation target.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PropertyTarget {
    Scene,
    Layer(LayerId),
    Object(TileObjectId),
}

/// Property mutation record.
#[derive(Clone, Debug, PartialEq)]
pub struct PropertyChange {
    pub target: PropertyTarget,
    pub key: String,
    pub old: Option<PropertyValue>,
    pub new: Option<PropertyValue>,
}

/// Accumulated edit session summary.
#[derive(Clone, Debug, Default)]
pub struct TileMapEditSummary {
    pub dirty_cells: Vec<DirtyCell>,
    pub dirty_regions: Vec<DirtyRegion>,
    pub tile_changes: Vec<TileChange>,
    pub changed_layers: Vec<LayerId>,
    pub object_changes: Vec<ObjectChange>,
    pub property_changes: Vec<PropertyChange>,
}

impl TileMapEditSummary {
    pub(super) fn mark_layer(&mut self, layer: LayerId) {
        if !self.changed_layers.contains(&layer) {
            self.changed_layers.push(layer);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.dirty_cells.is_empty()
            && self.dirty_regions.is_empty()
            && self.tile_changes.is_empty()
            && self.changed_layers.is_empty()
            && self.object_changes.is_empty()
            && self.property_changes.is_empty()
    }
}
