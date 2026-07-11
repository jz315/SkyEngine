use std::path::PathBuf;

use crate::render::features::tilemap::{
    TiledImport, TiledLayer as RenderTiledLayer, TiledObject as RenderTiledObject,
    TiledObjectLayer as RenderTiledObjectLayer, TiledObjectShape as RenderTiledObjectShape,
    TiledProperty as RenderTiledProperty, TiledPropertyValue as RenderTiledPropertyValue,
    TiledTileset as RenderTiledTileset,
};
use crate::tile::{
    Color, GridOrientation, RectU, StaggerAxis, StaggerIndex, TileAnimation, TileAnimationFrame,
    TileDefId, TileFlags, TileRenderOrder,
};

#[derive(Clone, Debug)]
pub struct TiledMapSnapshot {
    pub map_size: [u32; 2],
    pub orientation: GridOrientation,
    pub stagger_axis: StaggerAxis,
    pub stagger_index: StaggerIndex,
    pub hex_side_length: u32,
    pub render_order: TileRenderOrder,
    pub tile_size: [u32; 2],
    pub properties: Vec<TiledProperty>,
    pub tilesets: Vec<TiledTileset>,
    pub layers: Vec<TiledLayer>,
    pub object_layers: Vec<TiledObjectLayer>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TiledLayer {
    pub name: String,
    pub tileset_index: usize,
    pub storage_layer: u32,
    pub visible: bool,
    pub opacity: f32,
    pub offset: [f32; 2],
    pub parallax: [f32; 2],
    pub properties: Vec<TiledProperty>,
    pub tiles: Vec<TiledCell>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TiledCell {
    pub x: u32,
    pub y: u32,
    pub tile_id: u32,
    pub flags: TileFlags,
    pub tint: Color,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TiledObjectLayer {
    pub name: String,
    pub visible: bool,
    pub opacity: f32,
    pub offset: [f32; 2],
    pub parallax: [f32; 2],
    pub properties: Vec<TiledProperty>,
    pub objects: Vec<TiledObject>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TiledObject {
    pub id: u32,
    pub position: [f32; 2],
    pub properties: Vec<TiledProperty>,
    pub shape: TiledObjectShape,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TiledObjectShape {
    Rectangle,
    Point,
    Ellipse,
    Polygon(Vec<[f32; 2]>),
    Polyline(Vec<[f32; 2]>),
    Tile {
        tileset_index: usize,
        tile_id: u32,
        flags: TileFlags,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct TiledProperty {
    pub name: String,
    pub value: TiledPropertyValue,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TiledPropertyValue {
    Bool(bool),
    Int(i32),
    Float(f32),
    String(String),
    Color(Color),
    File(PathBuf),
    Object(u32),
}

#[derive(Clone, Debug, PartialEq)]
pub struct TiledTilesetImageSource {
    pub image: PathBuf,
    pub source_rect: RectU,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TiledTileAnimation {
    pub tile: TileDefId,
    pub animation: TileAnimation,
}

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
    pub tile_rects: Vec<Option<RectU>>,
    pub tile_images: Vec<Option<TiledTilesetImageSource>>,
    pub tile_offset: [i32; 2],
    pub animations: Vec<TiledTileAnimation>,
    pub properties: Vec<TiledProperty>,
    pub tile_properties: Vec<Vec<TiledProperty>>,
    pub transparent_color: Option<[u8; 3]>,
}

impl TiledTileset {
    pub fn tile_draw_size(&self, tile_id: u32) -> [u32; 2] {
        if self.tile_rects.is_empty() {
            self.tile_size
        } else {
            self.tile_rects
                .get(tile_id as usize)
                .and_then(|rect| rect.map(RectU::size))
                .unwrap_or(self.tile_size)
        }
    }

    pub fn animation(&self, tile_id: TileDefId) -> Option<TileAnimation> {
        self.animations
            .iter()
            .find(|animation| animation.tile == tile_id)
            .map(|animation| animation.animation.clone())
    }
}

pub fn map_snapshot(import: &TiledImport) -> TiledMapSnapshot {
    TiledMapSnapshot {
        map_size: [import.map.width(), import.map.height()],
        orientation: match import.orientation {
            crate::render::features::tilemap::TilemapOrientation::Orthogonal => {
                GridOrientation::Orthogonal
            }
            crate::render::features::tilemap::TilemapOrientation::Isometric => {
                GridOrientation::Isometric
            }
            crate::render::features::tilemap::TilemapOrientation::Staggered => {
                GridOrientation::Staggered
            }
            crate::render::features::tilemap::TilemapOrientation::Hexagonal => {
                GridOrientation::Hexagonal
            }
        },
        stagger_axis: match import.stagger_axis {
            crate::render::features::tilemap::TilemapStaggerAxis::X => StaggerAxis::X,
            crate::render::features::tilemap::TilemapStaggerAxis::Y => StaggerAxis::Y,
        },
        stagger_index: match import.stagger_index {
            crate::render::features::tilemap::TilemapStaggerIndex::Odd => StaggerIndex::Odd,
            crate::render::features::tilemap::TilemapStaggerIndex::Even => StaggerIndex::Even,
        },
        hex_side_length: import.hex_side_length,
        render_order: import.render_order.into(),
        tile_size: import.tile_size,
        properties: import.properties.iter().map(snapshot_property).collect(),
        tilesets: import.tilesets.iter().map(snapshot_tileset).collect(),
        layers: import
            .layers
            .iter()
            .map(|layer| snapshot_layer(import, layer))
            .collect(),
        object_layers: import
            .object_layers
            .iter()
            .map(snapshot_object_layer)
            .collect(),
    }
}

fn snapshot_layer(import: &TiledImport, layer: &RenderTiledLayer) -> TiledLayer {
    let mut tiles = Vec::new();
    for y in 0..import.map.height() {
        for x in 0..import.map.width() {
            let Some(tile) = import.map.tile(layer.storage_layer, x, y) else {
                continue;
            };
            if tile.is_empty() {
                continue;
            }
            tiles.push(TiledCell {
                x,
                y,
                tile_id: tile.id.0,
                flags: tile.flags.into(),
                tint: tile.tint,
            });
        }
    }
    TiledLayer {
        name: layer.name.clone(),
        tileset_index: layer.tileset_index,
        storage_layer: layer.storage_layer,
        visible: layer.visible,
        opacity: layer.opacity,
        offset: layer.offset,
        parallax: layer.parallax,
        properties: layer.properties.iter().map(snapshot_property).collect(),
        tiles,
    }
}

fn snapshot_object_layer(layer: &RenderTiledObjectLayer) -> TiledObjectLayer {
    TiledObjectLayer {
        name: layer.name.clone(),
        visible: layer.visible,
        opacity: layer.opacity,
        offset: layer.offset,
        parallax: layer.parallax,
        properties: layer.properties.iter().map(snapshot_property).collect(),
        objects: layer.objects.iter().map(snapshot_object).collect(),
    }
}

fn snapshot_object(object: &RenderTiledObject) -> TiledObject {
    TiledObject {
        id: object.id,
        position: object.position,
        properties: object.properties.iter().map(snapshot_property).collect(),
        shape: match &object.shape {
            RenderTiledObjectShape::Rectangle => TiledObjectShape::Rectangle,
            RenderTiledObjectShape::Point => TiledObjectShape::Point,
            RenderTiledObjectShape::Ellipse => TiledObjectShape::Ellipse,
            RenderTiledObjectShape::Polygon(points) => TiledObjectShape::Polygon(points.clone()),
            RenderTiledObjectShape::Polyline(points) => TiledObjectShape::Polyline(points.clone()),
            RenderTiledObjectShape::Tile {
                tileset_index,
                tile_id,
                flags,
            } => TiledObjectShape::Tile {
                tileset_index: *tileset_index,
                tile_id: tile_id.0,
                flags: (*flags).into(),
            },
        },
    }
}

fn snapshot_tileset(tileset: &RenderTiledTileset) -> TiledTileset {
    TiledTileset {
        first_gid: tileset.first_gid,
        image: tileset.image.clone(),
        tile_size: tileset.tile_size,
        columns: tileset.columns,
        rows: tileset.rows,
        tile_count: tileset.tile_count,
        image_size: tileset.image_size,
        margin: tileset.margin,
        spacing: tileset.spacing,
        tile_rects: tileset
            .tile_rects
            .iter()
            .map(|rect| rect.map(|rect| RectU::new(rect.x, rect.y, rect.width, rect.height)))
            .collect(),
        tile_images: tileset
            .tile_images
            .iter()
            .map(|source| {
                source.as_ref().map(|source| TiledTilesetImageSource {
                    image: source.image.clone(),
                    source_rect: RectU::new(
                        source.source_rect.x,
                        source.source_rect.y,
                        source.source_rect.width,
                        source.source_rect.height,
                    ),
                })
            })
            .collect(),
        tile_offset: tileset.tile_offset,
        animations: tileset
            .animations
            .iter()
            .map(|animation| TiledTileAnimation {
                tile: TileDefId(animation.tile_id.0),
                animation: TileAnimation {
                    frames: animation
                        .frames
                        .iter()
                        .map(|frame| TileAnimationFrame {
                            tile: TileDefId(frame.tile_id.0),
                            duration_ms: frame.duration_ms,
                        })
                        .collect(),
                },
            })
            .collect(),
        properties: tileset.properties.iter().map(snapshot_property).collect(),
        tile_properties: tileset
            .tile_properties
            .iter()
            .map(|properties| properties.iter().map(snapshot_property).collect())
            .collect(),
        transparent_color: tileset.transparent_color,
    }
}

fn snapshot_property(property: &RenderTiledProperty) -> TiledProperty {
    TiledProperty {
        name: property.name.clone(),
        value: match &property.value {
            RenderTiledPropertyValue::Bool(value) => TiledPropertyValue::Bool(*value),
            RenderTiledPropertyValue::Int(value) => TiledPropertyValue::Int(*value),
            RenderTiledPropertyValue::Float(value) => TiledPropertyValue::Float(*value),
            RenderTiledPropertyValue::String(value) => TiledPropertyValue::String(value.clone()),
            RenderTiledPropertyValue::Color(color) => TiledPropertyValue::Color(*color),
            RenderTiledPropertyValue::File(path) => TiledPropertyValue::File(path.clone()),
            RenderTiledPropertyValue::Object(value) => TiledPropertyValue::Object(*value),
        },
    }
}
