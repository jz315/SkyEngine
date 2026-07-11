use std::path::PathBuf;

use crate::render::Color;

use super::super::{TileAnimation, TileFlags, TileId, TilesetTileRect};

/// One imported Tiled tile layer.
#[derive(Clone, Debug, PartialEq)]
pub struct TiledLayer {
    pub name: String,
    pub source_layer: usize,
    pub tileset_index: usize,
    pub storage_layer: u32,
    pub source_order: i32,
    pub sorting_layer: i32,
    pub visible: bool,
    pub opacity: f32,
    pub offset: [f32; 2],
    pub parallax: [f32; 2],
    pub properties: Vec<TiledProperty>,
}

/// One imported Tiled object layer.
#[derive(Clone, Debug, PartialEq)]
pub struct TiledObjectLayer {
    pub name: String,
    pub source_order: i32,
    pub sorting_layer: i32,
    pub visible: bool,
    pub opacity: f32,
    pub offset: [f32; 2],
    pub parallax: [f32; 2],
    pub properties: Vec<TiledProperty>,
    pub objects: Vec<TiledObject>,
}

/// One imported Tiled object.
#[derive(Clone, Debug, PartialEq)]
pub struct TiledObject {
    pub id: u32,
    pub name: String,
    pub class: String,
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub shape: TiledObjectShape,
    pub properties: Vec<TiledProperty>,
    pub template: Option<PathBuf>,
}

/// Shape/data payload for one imported Tiled object.
#[derive(Clone, Debug, PartialEq)]
pub enum TiledObjectShape {
    Rectangle,
    Point,
    Ellipse,
    Polygon(Vec<[f32; 2]>),
    Polyline(Vec<[f32; 2]>),
    Tile {
        tileset_index: usize,
        tile_id: TileId,
        flags: TileFlags,
    },
}

/// One custom property imported from Tiled.
#[derive(Clone, Debug, PartialEq)]
pub struct TiledProperty {
    pub name: String,
    pub value: TiledPropertyValue,
}

/// Tiled custom property value.
#[derive(Clone, Debug)]
pub enum TiledPropertyValue {
    Bool(bool),
    Int(i32),
    Float(f32),
    String(String),
    Color(Color),
    File(PathBuf),
    Object(u32),
}

impl PartialEq for TiledPropertyValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Bool(a), Self::Bool(b)) => a == b,
            (Self::Int(a), Self::Int(b)) => a == b,
            (Self::Float(a), Self::Float(b)) => a.to_bits() == b.to_bits(),
            (Self::String(a), Self::String(b)) => a == b,
            (Self::Color(a), Self::Color(b)) => {
                a.r.to_bits() == b.r.to_bits()
                    && a.g.to_bits() == b.g.to_bits()
                    && a.b.to_bits() == b.b.to_bits()
                    && a.a.to_bits() == b.a.to_bits()
            }
            (Self::File(a), Self::File(b)) => a == b,
            (Self::Object(a), Self::Object(b)) => a == b,
            _ => false,
        }
    }
}

impl TiledObject {
    pub fn tile_id(&self) -> Option<TileId> {
        match self.shape {
            TiledObjectShape::Tile { tile_id, .. } => Some(tile_id),
            _ => None,
        }
    }

    pub fn tile_flags(&self) -> Option<TileFlags> {
        match self.shape {
            TiledObjectShape::Tile { flags, .. } => Some(flags),
            _ => None,
        }
    }

    pub fn tile_tileset_index(&self) -> Option<usize> {
        match self.shape {
            TiledObjectShape::Tile { tileset_index, .. } => Some(tileset_index),
            _ => None,
        }
    }

    pub fn center(&self) -> [f32; 2] {
        [
            self.position[0] + self.size[0] * 0.5,
            self.position[1] + self.size[1] * 0.5,
        ]
    }
}

impl TiledObject {
    pub fn is_tile(&self) -> bool {
        matches!(self.shape, TiledObjectShape::Tile { .. })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TiledTileObject {
    pub tileset_index: usize,
    pub tile_id: TileId,
    pub flags: TileFlags,
}

/// Per-tile source image metadata for a Tiled image collection tileset packed
/// into a runtime atlas.
#[derive(Clone, Debug, PartialEq)]
pub struct TiledTilesetImageSource {
    pub image: PathBuf,
    pub source_rect: TilesetTileRect,
}

/// Tileset metadata imported from Tiled.
#[derive(Clone, Debug, PartialEq)]
pub struct TiledTileset {
    pub first_gid: u32,
    pub image: PathBuf,
    pub tile_size: [u32; 2],
    pub columns: u32,
    pub rows: u32,
    pub tile_count: u32,
    pub image_size: [u32; 2],
    pub margin: u32,
    pub spacing: u32,
    pub tile_rects: Vec<Option<TilesetTileRect>>,
    pub tile_images: Vec<Option<TiledTilesetImageSource>>,
    pub tile_offset: [i32; 2],
    pub animations: Vec<TileAnimation>,
    pub properties: Vec<TiledProperty>,
    pub tile_properties: Vec<Vec<TiledProperty>>,
    pub transparent_color: Option<[u8; 3]>,
}

impl TiledTileset {
    pub fn tile_draw_size(&self, tile_id: TileId) -> [u32; 2] {
        if self.tile_rects.is_empty() {
            self.tile_size
        } else {
            self.tile_rects
                .get(tile_id.0 as usize)
                .and_then(|rect| rect.map(TilesetTileRect::size))
                .unwrap_or(self.tile_size)
        }
    }
}
