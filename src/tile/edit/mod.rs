mod error;
mod layers;
mod map;

use super::model::{CellCoord, CellRect};

pub use super::model::MapId;
pub use super::model::TileObjectId as ObjectId;
pub use super::model::TileRef;
pub use error::TileError;
pub use layers::{
    object, Brush, CollisionLayer, CollisionValue, MetadataLayer, ObjectHandle, ObjectLayer,
    ObjectSpec, ObjectView, TileLayer,
};
pub use map::{Map, MapBuilder, MapEditor, Tiles};

pub type Cell = CellCoord;
pub type Rect = CellRect;
