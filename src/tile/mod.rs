//! High-level tilemap API.
//!
//! The standard user path is `Tiles -> Map -> typed layers`. Maps are backed by
//! ECS-owned runtime state; render storage and imported documents are
//! implementation details of that live map.

mod edit;
pub(crate) mod io;
pub(crate) mod model;
pub(crate) mod render_bridge;
mod runtime;

pub use edit::{
    object, Brush, Cell, CollisionLayer, CollisionValue, Map, MapBuilder, MapEditor, MapId,
    MetadataLayer, ObjectHandle, ObjectId, ObjectLayer, ObjectSpec, ObjectView, Rect, TileError,
    TileLayer, TileRef, Tiles,
};
pub use model::{
    AssetSource, CellCoord, CellRect, ChunkedTileData, CollisionLayerData, Color, Footprint,
    GridOrientation, GridOrigin, GridSpec, LayerData, LayerId, LayerKind, MetadataLayerData,
    ObjectLayerData, ObjectPrototypeId, ObjectVisual, ObjectVisualTile, PaletteId, PropertyBag,
    PropertyValue, RectU, SpriteVisualRef, StaggerAxis, StaggerIndex, TileAnimation,
    TileAnimationFrame, TileAtlasImageSource, TileCell, TileCollision, TileDef, TileDefId,
    TileDirection, TileFlags, TileLayerData, TilePalette, TilePaletteStore, TileRenderOrder,
    TileTextureSource,
};

pub(crate) use model::{LayerRole, MapData, MapSize, TileObject, TileObjectId};
