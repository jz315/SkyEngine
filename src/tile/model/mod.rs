pub(crate) mod color;
pub(crate) mod grid;
pub(crate) mod layer;
pub(crate) mod map;
pub(crate) mod object;
pub(crate) mod palette;

pub use color::Color;
pub use grid::{
    CellCoord, CellRect, GridOrientation, GridOrigin, GridSpec, StaggerAxis, StaggerIndex,
    TileDirection, TileRenderOrder,
};
pub(crate) use layer::LayerRole;
pub use layer::{
    ChunkedTileData, CollisionLayerData, LayerData, LayerId, LayerKind, MapLayer,
    MetadataLayerData, ObjectLayerData, TileCell, TileFlags, TileLayerData, TileRef,
};
pub use map::MapId;
pub(crate) use map::{MapData, MapSize};
pub(crate) use object::TileObject;
pub use object::TileObjectId;
pub use object::{Footprint, ObjectPrototypeId, ObjectVisual, ObjectVisualTile, SpriteVisualRef};
pub use palette::{
    AssetSource, PaletteId, PropertyBag, PropertyValue, RectU, TileAnimation, TileAnimationFrame,
    TileAtlasImageSource, TileCollision, TileDef, TileDefId, TilePalette, TilePaletteStore,
    TileTextureSource,
};
