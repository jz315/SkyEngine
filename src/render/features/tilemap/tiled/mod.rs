use std::path::Path;
#[cfg(test)]
use std::path::PathBuf;

use crate::asset::{Handle, TextureAsset, TextureColorSpace};
use crate::render::Color;

#[cfg(test)]
use super::TileFlags;
#[cfg(test)]
use super::TileId;
use super::{
    Tilemap, TilemapDepthSort, TilemapDescriptor, TilemapHandle, TilemapOrientation,
    TilemapRenderOrder, TilemapRenderer, TilemapStaggerAxis, TilemapStaggerIndex, TilesetGrid,
    TilesetTileRect,
};

mod data;
mod error;
mod json;
mod layer;
mod object;
mod properties;
mod tileset;
mod tmx;
mod types;
mod util;
use data::decode_json_gids;
pub use error::TiledImportError;
use json::{TiledJsonLayer, TiledJsonMap};
use layer::{
    decode_cell, split_layers_by_tileset, tiled_depth_sort_for_layers, tilemap_bounds,
    LayerContext, ParsedLayer, RawCell,
};
#[cfg(test)]
use layer::{FLIPPED_DIAGONALLY_FLAG, FLIPPED_HORIZONTALLY_FLAG, FLIPPED_VERTICALLY_FLAG};
use object::{collect_json_object, decode_object};
use properties::{collect_json_properties, collect_tmx_properties};
use tileset::{resolve_json_tilesets, resolve_tmx_tilesets};
use tmx::{collect_tmx_child_layers, ParsedObjectLayer};
pub use types::{
    TiledLayer, TiledObject, TiledObjectLayer, TiledObjectShape, TiledProperty, TiledPropertyValue,
    TiledTileObject, TiledTileset, TiledTilesetImageSource,
};
use util::{
    optional_bool_attr, optional_f32_attr, optional_u32_attr, required_attr, required_u32_attr,
};

/// Result of importing a Tiled map.
#[derive(Clone, Debug)]
pub struct TiledImport {
    pub map: Tilemap,
    pub orientation: TilemapOrientation,
    pub stagger_axis: TilemapStaggerAxis,
    pub stagger_index: TilemapStaggerIndex,
    pub hex_side_length: u32,
    pub render_order: TilemapRenderOrder,
    pub depth_sort: TilemapDepthSort,
    pub tile_size: [u32; 2],
    pub tile_origin: [i32; 2],
    pub parallax_origin: [f32; 2],
    pub properties: Vec<TiledProperty>,
    pub tilesets: Vec<TiledTileset>,
    pub tileset: TiledTileset,
    pub layers: Vec<TiledLayer>,
    pub object_layers: Vec<TiledObjectLayer>,
}

mod import;
mod parse;
use parse::*;
#[cfg(test)]
mod tests;
