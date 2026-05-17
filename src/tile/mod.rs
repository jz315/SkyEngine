//! Format-neutral tile scene runtime types.
//!
//! This module is intentionally separate from the Tiled importer. Authoring
//! adapters should convert into these types; rendering backends can then sync
//! them into lower-level tilemap storage.

pub mod adapters;
mod document;
mod edit;
mod grid;
mod instance;
mod layer;
mod object;
mod palette;
mod persistence;
mod scene;
mod sync;

pub use document::{
    TileAuthoringFormat, TileAuthoringMetadata, TileMapDocument, TileMapDocumentBuilder,
};
pub use edit::{
    DirtyCell, DirtyRegion, ObjectChange, PropertyChange, PropertyTarget, TileChange,
    TileMapEditHistory, TileMapEditSession, TileMapEditSummary,
};
pub use grid::{
    CellCoord, CellRect, GridOrientation, GridOrigin, GridSpec, StaggerAxis, StaggerIndex,
    TileDirection,
};
pub use instance::{
    TileMapInstance, TileMapInstanceError, TileMapSpawnOptions, TileMapSpawnOrigin,
};
pub use layer::{
    ChunkedTileData, CollisionLayerData, LayerData, LayerId, LayerKind, LayerRole,
    MetadataLayerData, ObjectLayerData, SceneTile, TileLayer, TileLayerData, TileRef,
};
pub use object::{
    Footprint, ObjectPrototypeId, ObjectVisual, ObjectVisualTile, SpriteVisualRef, TileObject,
    TileObjectId, TileObjectStore,
};
pub use palette::{
    AssetSource, PaletteId, PropertyBag, PropertyValue, RectU, TileAnimation, TileAnimationFrame,
    TileAtlasImageSource, TileCollision, TileDef, TileDefId, TilePalette, TilePaletteStore,
    TileTextureSource,
};
pub use persistence::{TileDocumentRevision, TileMapDelta, TileMapSnapshot, TilePersistenceError};
pub use scene::{TileMap, TileMapId, TileMapSize, TileWorld};
pub use sync::{TileMapRenderData, TileMapRenderLayer, TileMapRenderSync, TileMapRenderSyncError};
