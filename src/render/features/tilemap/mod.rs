mod cache;
mod component;
mod draw;
mod extract;
mod feature;
mod instance;
#[cfg(feature = "physics")]
mod physics;
mod storage;
mod tiled;

pub(crate) use cache::{
    SharedTilemapFrameCache, TilemapChunkState, TilemapFrameCache, TilemapGpuChunkKey,
    TilemapInstance, TilemapInstanceSpan, TilemapPreparedInstances,
};
pub(crate) use draw::{DrawTilemap, TilemapDrawData};
pub(crate) use extract::ExtractTilemaps;

pub use cache::TilemapCacheConfig;
pub use component::{
    TileAnimation, TileAnimationFrame, TilemapDepthSort, TilemapOrientation, TilemapRenderOrder,
    TilemapRenderer, TilemapStaggerAxis, TilemapStaggerIndex, TilesetGrid, TilesetTileRect,
};
pub use feature::TilemapFeature;
pub use instance::{TiledMapInstance, TiledMapInstanceError, TiledSpawnOptions, TiledSpawnOrigin};
#[cfg(feature = "physics")]
pub use physics::{TiledPhysicsError, TiledPhysicsInstance, TiledPhysicsOptions};
pub use storage::{
    Tile, TileChunkBounds, TileFlags, TileId, Tilemap, TilemapDescriptor, TilemapHandle,
    TilemapStorage,
};
pub use tiled::{
    TiledImport, TiledImportError, TiledLayer, TiledObject, TiledObjectLayer, TiledObjectShape,
    TiledProperty, TiledPropertyValue, TiledTileObject, TiledTileset, TiledTilesetImageSource,
};
