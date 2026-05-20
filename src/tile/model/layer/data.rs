use std::collections::BTreeMap;

use super::super::grid::{CellCoord, CellRect};
use super::super::object::TileObjectId;
use super::super::palette::PropertyBag;
use super::tile::ChunkedTileData;

/// Tile layer storage.
#[derive(Clone, Debug, Default)]
pub struct TileLayerData {
    pub tiles: ChunkedTileData,
}

/// Object layer storage.
#[derive(Clone, Debug, Default)]
pub struct ObjectLayerData {
    pub objects: Vec<TileObjectId>,
}

impl ObjectLayerData {
    pub fn remove_object(&mut self, id: TileObjectId) -> bool {
        let old_len = self.objects.len();
        self.objects.retain(|object_id| *object_id != id);
        self.objects.len() != old_len
    }
}

/// Collision layer storage placeholder.
#[derive(Clone, Debug, Default)]
pub struct CollisionLayerData {
    pub occupied: Vec<CellRect>,
}

/// Metadata layer storage placeholder.
#[derive(Clone, Debug, Default)]
pub struct MetadataLayerData {
    pub cells: BTreeMap<CellCoord, PropertyBag>,
}

/// Structured layer payload.
#[derive(Clone, Debug)]
pub enum LayerData {
    Tiles(TileLayerData),
    Objects(ObjectLayerData),
    Collision(CollisionLayerData),
    Metadata(MetadataLayerData),
}

impl LayerData {
    pub fn as_tiles(&self) -> Option<&TileLayerData> {
        match self {
            Self::Tiles(data) => Some(data),
            _ => None,
        }
    }

    pub fn as_tiles_mut(&mut self) -> Option<&mut TileLayerData> {
        match self {
            Self::Tiles(data) => Some(data),
            _ => None,
        }
    }

    pub fn as_objects_mut(&mut self) -> Option<&mut ObjectLayerData> {
        match self {
            Self::Objects(data) => Some(data),
            _ => None,
        }
    }

    pub fn as_objects(&self) -> Option<&ObjectLayerData> {
        match self {
            Self::Objects(data) => Some(data),
            _ => None,
        }
    }
}
