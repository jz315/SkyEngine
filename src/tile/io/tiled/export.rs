use std::fmt;
use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

use crate::tile::model::MapLayer;
use crate::tile::{
    Color, GridOrientation, LayerData, LayerId, MapData, ObjectVisual, PaletteId, PropertyBag,
    PropertyValue, TileDef, TileDefId, TileFlags, TileObject, TilePalette, TileTextureSource,
};

const FLIPPED_HORIZONTALLY_FLAG: u32 = 0x8000_0000;
const FLIPPED_VERTICALLY_FLAG: u32 = 0x4000_0000;
const FLIPPED_DIAGONALLY_FLAG: u32 = 0x2000_0000;

#[derive(Debug)]
pub enum TiledExportError {
    MissingPalette(PaletteId),
    MissingTile {
        palette: PaletteId,
        tile: TileDefId,
    },
    UnsupportedPaletteTexture {
        palette: PaletteId,
        reason: &'static str,
    },
    UnsupportedObjectVisual {
        object: u64,
        reason: &'static str,
    },
    UnsupportedTileTint {
        layer: LayerId,
        cell: [i32; 2],
    },
    Json(serde_json::Error),
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}

impl fmt::Display for TiledExportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingPalette(id) => write!(f, "tile scene references missing palette {:?}", id),
            Self::MissingTile { palette, tile } => {
                write!(
                    f,
                    "tile scene references missing tile {:?} in {:?}",
                    tile, palette
                )
            }
            Self::UnsupportedPaletteTexture { palette, reason } => {
                write!(
                    f,
                    "palette {:?} cannot be exported to TMJ: {reason}",
                    palette
                )
            }
            Self::UnsupportedObjectVisual { object, reason } => {
                write!(f, "object {object} cannot be exported to TMJ: {reason}")
            }
            Self::UnsupportedTileTint { layer, cell } => write!(
                f,
                "tile tint on layer {:?} at [{}, {}] cannot be represented in TMJ tile data",
                layer, cell[0], cell[1]
            ),
            Self::Json(source) => write!(f, "failed to serialize TMJ: {source}"),
            Self::Io { path, source } => write!(f, "failed to write {}: {source}", path.display()),
        }
    }
}

impl std::error::Error for TiledExportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Json(source) => Some(source),
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<serde_json::Error> for TiledExportError {
    fn from(source: serde_json::Error) -> Self {
        Self::Json(source)
    }
}

/// Exports SkyEngine tile scenes into Tiled map formats.
pub struct TiledExporter;

impl TiledExporter {
    pub fn to_tmj_string(
        scene: &MapData,
        palettes: &[TilePalette],
    ) -> Result<String, TiledExportError> {
        let palette_order = scene_palette_order(scene, palettes)?;
        let map = export_tmj_value(scene, &palette_order)?;
        serde_json::to_string_pretty(&map).map_err(TiledExportError::Json)
    }

    pub fn write_tmj(
        path: impl AsRef<Path>,
        scene: &MapData,
        palettes: &[TilePalette],
    ) -> Result<(), TiledExportError> {
        let path = path.as_ref();
        let text = Self::to_tmj_string(scene, palettes)?;
        std::fs::write(path, text).map_err(|source| TiledExportError::Io {
            path: path.to_path_buf(),
            source,
        })
    }
}

fn export_tmj_value(
    scene: &MapData,
    palette_order: &[&TilePalette],
) -> Result<Value, TiledExportError> {
    let mut map = Map::new();
    map.insert("type".to_string(), json!("map"));
    map.insert("version".to_string(), json!("1.10"));
    map.insert("tiledversion".to_string(), json!("1.10.2"));
    map.insert(
        "orientation".to_string(),
        json!(orientation_name(scene.grid.orientation)),
    );
    map.insert(
        "renderorder".to_string(),
        json!(render_order_name(scene.grid.render_order)),
    );
    map.insert("width".to_string(), json!(scene.size.width));
    map.insert("height".to_string(), json!(scene.size.height));
    map.insert("tilewidth".to_string(), json!(scene.grid.cell_size[0]));
    map.insert("tileheight".to_string(), json!(scene.grid.cell_size[1]));
    map.insert("infinite".to_string(), json!(false));
    if let Some(axis) = stagger_axis_name(scene.grid.stagger_axis) {
        map.insert("staggeraxis".to_string(), json!(axis));
    }
    if let Some(index) = stagger_index_name(scene.grid.stagger_index) {
        map.insert("staggerindex".to_string(), json!(index));
    }
    if let Some(side) = scene.grid.hex_side_length {
        map.insert("hexsidelength".to_string(), json!(side));
    }
    if !scene.properties.is_empty() {
        map.insert(
            "properties".to_string(),
            export_properties(&scene.properties),
        );
    }
    map.insert(
        "tilesets".to_string(),
        Value::Array(
            palette_order
                .iter()
                .map(|palette| export_tileset(palette))
                .collect::<Result<Vec<_>, _>>()?,
        ),
    );
    map.insert(
        "layers".to_string(),
        Value::Array(
            scene
                .layers
                .iter()
                .map(|layer| export_layer(scene, layer, &palette_order))
                .collect::<Result<Vec<_>, _>>()?,
        ),
    );
    Ok(Value::Object(map))
}

fn scene_palette_order<'a>(
    scene: &MapData,
    palettes: &'a [TilePalette],
) -> Result<Vec<&'a TilePalette>, TiledExportError> {
    let mut ordered = Vec::new();
    for id in &scene.palettes {
        ordered.push(
            palettes
                .iter()
                .find(|palette| palette.id == *id)
                .ok_or(TiledExportError::MissingPalette(*id))?,
        );
    }
    Ok(ordered)
}

fn export_tileset(palette: &TilePalette) -> Result<Value, TiledExportError> {
    match &palette.texture {
        TileTextureSource::Image(path) => {
            let image = path.to_string_lossy().replace('\\', "/");
            let image_size = infer_image_size(palette);
            let tile_size = palette
                .tiles
                .first()
                .map(|tile| tile.draw_size)
                .unwrap_or([1, 1]);
            let columns = image_size[0]
                .checked_div(tile_size[0].max(1))
                .unwrap_or(1)
                .max(1);
            let tile_count = palette
                .tiles
                .iter()
                .map(|tile| tile.id.0)
                .max()
                .unwrap_or_default()
                .saturating_add(1)
                .max(1);

            let mut tileset = Map::new();
            tileset.insert("firstgid".to_string(), json!(palette.id.0));
            tileset.insert("name".to_string(), json!(palette.name));
            tileset.insert("image".to_string(), json!(image));
            tileset.insert("imagewidth".to_string(), json!(image_size[0]));
            tileset.insert("imageheight".to_string(), json!(image_size[1]));
            tileset.insert("tilewidth".to_string(), json!(tile_size[0]));
            tileset.insert("tileheight".to_string(), json!(tile_size[1]));
            tileset.insert("columns".to_string(), json!(columns));
            tileset.insert("tilecount".to_string(), json!(tile_count));
            if let Some(offset) = shared_tile_offset(&palette.tiles) {
                if offset != [0, 0] {
                    tileset.insert(
                        "tileoffset".to_string(),
                        json!({ "x": offset[0], "y": offset[1] }),
                    );
                }
            }
            if !palette.properties.is_empty() {
                tileset.insert(
                    "properties".to_string(),
                    export_properties(&palette.properties),
                );
            }
            let tile_entries = export_tile_entries(&palette.tiles);
            if !tile_entries.is_empty() {
                tileset.insert("tiles".to_string(), Value::Array(tile_entries));
            }
            Ok(Value::Object(tileset))
        }
        TileTextureSource::Texture { .. } => Err(TiledExportError::UnsupportedPaletteTexture {
            palette: palette.id,
            reason: "runtime texture palettes need preserved authoring metadata before TMJ export",
        }),
        TileTextureSource::None => Err(TiledExportError::UnsupportedPaletteTexture {
            palette: palette.id,
            reason: "palette has no texture source",
        }),
        TileTextureSource::ImageCollectionAtlas { .. } => export_image_collection_tileset(palette),
    }
}

fn export_image_collection_tileset(palette: &TilePalette) -> Result<Value, TiledExportError> {
    let TileTextureSource::ImageCollectionAtlas { tiles, .. } = &palette.texture else {
        return Err(TiledExportError::UnsupportedPaletteTexture {
            palette: palette.id,
            reason: "palette is not an image collection",
        });
    };

    let tile_size = palette
        .tiles
        .iter()
        .map(|tile| tile.draw_size)
        .fold([1, 1], |acc, size| {
            [acc[0].max(size[0]), acc[1].max(size[1])]
        });
    let tile_count = palette
        .tiles
        .iter()
        .map(|tile| tile.id.0)
        .max()
        .unwrap_or_default()
        .saturating_add(1)
        .max(1);

    let mut tileset = Map::new();
    tileset.insert("firstgid".to_string(), json!(palette.id.0));
    tileset.insert("name".to_string(), json!(palette.name));
    tileset.insert("tilewidth".to_string(), json!(tile_size[0]));
    tileset.insert("tileheight".to_string(), json!(tile_size[1]));
    tileset.insert("columns".to_string(), json!(tile_count));
    tileset.insert("tilecount".to_string(), json!(tile_count));
    if let Some(offset) = shared_tile_offset(&palette.tiles) {
        if offset != [0, 0] {
            tileset.insert(
                "tileoffset".to_string(),
                json!({ "x": offset[0], "y": offset[1] }),
            );
        }
    }
    if !palette.properties.is_empty() {
        tileset.insert(
            "properties".to_string(),
            export_properties(&palette.properties),
        );
    }
    let tile_entries = export_image_collection_tile_entries(palette, tiles);
    tileset.insert("tiles".to_string(), Value::Array(tile_entries));
    Ok(Value::Object(tileset))
}

fn export_tile_entries(tiles: &[TileDef]) -> Vec<Value> {
    tiles
        .iter()
        .filter(|tile| !tile.properties.is_empty() || tile.animation.is_some())
        .map(|tile| {
            let mut entry = Map::new();
            entry.insert("id".to_string(), json!(tile.id.0));
            if !tile.properties.is_empty() {
                entry.insert(
                    "properties".to_string(),
                    export_properties(&tile.properties),
                );
            }
            if let Some(animation) = &tile.animation {
                entry.insert(
                    "animation".to_string(),
                    Value::Array(
                        animation
                            .frames
                            .iter()
                            .map(|frame| {
                                json!({
                                    "tileid": frame.tile.0,
                                    "duration": frame.duration_ms,
                                })
                            })
                            .collect(),
                    ),
                );
            }
            Value::Object(entry)
        })
        .collect()
}

fn export_image_collection_tile_entries(
    palette: &TilePalette,
    images: &[crate::tile::TileAtlasImageSource],
) -> Vec<Value> {
    images
        .iter()
        .filter_map(|source| {
            let tile = palette.tile(source.tile)?;
            let mut entry = Map::new();
            entry.insert("id".to_string(), json!(source.tile.0));
            entry.insert(
                "image".to_string(),
                json!(source.image.to_string_lossy().replace('\\', "/")),
            );
            entry.insert("imagewidth".to_string(), json!(source.source_rect.width));
            entry.insert("imageheight".to_string(), json!(source.source_rect.height));
            entry.insert("width".to_string(), json!(source.source_rect.width));
            entry.insert("height".to_string(), json!(source.source_rect.height));
            if source.source_rect.x != 0 {
                entry.insert("x".to_string(), json!(source.source_rect.x));
            }
            if source.source_rect.y != 0 {
                entry.insert("y".to_string(), json!(source.source_rect.y));
            }
            if !tile.properties.is_empty() {
                entry.insert(
                    "properties".to_string(),
                    export_properties(&tile.properties),
                );
            }
            if let Some(animation) = &tile.animation {
                entry.insert(
                    "animation".to_string(),
                    Value::Array(
                        animation
                            .frames
                            .iter()
                            .map(|frame| {
                                json!({
                                    "tileid": frame.tile.0,
                                    "duration": frame.duration_ms,
                                })
                            })
                            .collect(),
                    ),
                );
            }
            Some(Value::Object(entry))
        })
        .collect()
}

fn export_layer(
    scene: &MapData,
    layer: &MapLayer,
    palettes: &[&TilePalette],
) -> Result<Value, TiledExportError> {
    match &layer.data {
        LayerData::Tiles(data) => {
            let mut layer_json = common_layer_fields(layer, "tilelayer");
            layer_json.insert("width".to_string(), json!(scene.size.width));
            layer_json.insert("height".to_string(), json!(scene.size.height));
            let mut cells = vec![0u32; scene.size.width.saturating_mul(scene.size.height) as usize];
            for (cell, tile) in data.tiles.iter() {
                if !is_white(tile.tint) {
                    return Err(TiledExportError::UnsupportedTileTint {
                        layer: layer.id,
                        cell: [cell.x, cell.y],
                    });
                }
                if cell.x < 0
                    || cell.y < 0
                    || cell.x >= scene.size.width as i32
                    || cell.y >= scene.size.height as i32
                {
                    continue;
                }
                let index = cell.y as usize * scene.size.width as usize + cell.x as usize;
                cells[index] = gid_for_tile_ref(
                    tile.tile_ref.palette,
                    tile.tile_ref.tile,
                    tile.flags,
                    palettes,
                )?;
            }
            layer_json.insert("data".to_string(), json!(cells));
            Ok(Value::Object(layer_json))
        }
        LayerData::Objects(data) => {
            let mut layer_json = common_layer_fields(layer, "objectgroup");
            layer_json.insert(
                "objects".to_string(),
                Value::Array(
                    data.objects
                        .iter()
                        .filter_map(|id| scene.objects.get(*id))
                        .map(|object| export_object(scene, object, palettes))
                        .collect::<Result<Vec<_>, _>>()?,
                ),
            );
            Ok(Value::Object(layer_json))
        }
        LayerData::Collision(_) | LayerData::Metadata(_) => {
            let mut layer_json = common_layer_fields(layer, "objectgroup");
            layer_json.insert("objects".to_string(), Value::Array(Vec::new()));
            Ok(Value::Object(layer_json))
        }
    }
}

fn common_layer_fields(layer: &MapLayer, layer_type: &'static str) -> Map<String, Value> {
    let mut fields = Map::new();
    fields.insert("id".to_string(), json!(layer.id.0));
    fields.insert("name".to_string(), json!(layer.name));
    fields.insert("type".to_string(), json!(layer_type));
    fields.insert("visible".to_string(), json!(layer.visible));
    fields.insert("opacity".to_string(), json!(layer.opacity));
    fields.insert("offsetx".to_string(), json!(layer.offset[0]));
    fields.insert("offsety".to_string(), json!(layer.offset[1]));
    fields.insert("parallaxx".to_string(), json!(layer.parallax[0]));
    fields.insert("parallaxy".to_string(), json!(layer.parallax[1]));
    if !layer.properties.is_empty() {
        fields.insert(
            "properties".to_string(),
            export_properties(&layer.properties),
        );
    }
    fields
}

fn export_object(
    scene: &MapData,
    object: &TileObject,
    palettes: &[&TilePalette],
) -> Result<Value, TiledExportError> {
    let width = object.footprint.size[0].max(1) as f32 * scene.grid.cell_size[0] as f32;
    let height = object.footprint.size[1].max(1) as f32 * scene.grid.cell_size[1] as f32;
    let x = object.cell.x as f32 * scene.grid.cell_size[0] as f32;
    let map_pixel_height = scene.size.height as f32 * scene.grid.cell_size[1] as f32;
    let y = map_pixel_height - object.cell.y as f32 * scene.grid.cell_size[1] as f32;

    let mut value = Map::new();
    value.insert("id".to_string(), json!(object.id.0));
    value.insert("x".to_string(), json!(x));
    value.insert("y".to_string(), json!(y));
    value.insert("width".to_string(), json!(width));
    value.insert("height".to_string(), json!(height));
    match &object.visual {
        ObjectVisual::Tile(tile_ref) => {
            value.insert(
                "gid".to_string(),
                json!(gid_for_tile_ref(
                    tile_ref.palette,
                    tile_ref.tile,
                    TileFlags::empty(),
                    palettes
                )?),
            );
        }
        ObjectVisual::None => {}
        ObjectVisual::Sprite(_) => {
            return Err(TiledExportError::UnsupportedObjectVisual {
                object: object.id.0,
                reason: "sprite visuals are not Tiled tile objects",
            });
        }
        ObjectVisual::MultiTile(_) => {
            return Err(TiledExportError::UnsupportedObjectVisual {
                object: object.id.0,
                reason: "multi-tile objects require expansion into multiple Tiled objects",
            });
        }
    }
    if !object.properties.is_empty() {
        value.insert(
            "properties".to_string(),
            export_properties(&object.properties),
        );
    }
    Ok(Value::Object(value))
}

fn gid_for_tile_ref(
    palette_id: PaletteId,
    tile_id: TileDefId,
    flags: TileFlags,
    palettes: &[&TilePalette],
) -> Result<u32, TiledExportError> {
    let palette = palettes
        .iter()
        .copied()
        .find(|palette| palette.id == palette_id)
        .ok_or(TiledExportError::MissingPalette(palette_id))?;
    if palette.tile(tile_id).is_none() {
        return Err(TiledExportError::MissingTile {
            palette: palette_id,
            tile: tile_id,
        });
    }
    let mut gid = palette_id.0.saturating_add(tile_id.0);
    if flags.contains(TileFlags::FLIP_X) {
        gid |= FLIPPED_HORIZONTALLY_FLAG;
    }
    if flags.contains(TileFlags::FLIP_Y) {
        gid |= FLIPPED_VERTICALLY_FLAG;
    }
    if flags.contains(TileFlags::FLIP_DIAGONAL) {
        gid |= FLIPPED_DIAGONALLY_FLAG;
    }
    Ok(gid)
}

fn export_properties(properties: &PropertyBag) -> Value {
    Value::Array(
        properties
            .iter()
            .map(|(name, value)| {
                let (value_type, value) = property_json_value(value);
                json!({
                    "name": name,
                    "type": value_type,
                    "value": value,
                })
            })
            .collect(),
    )
}

fn property_json_value(value: &PropertyValue) -> (&'static str, Value) {
    match value {
        PropertyValue::Bool(value) => ("bool", json!(value)),
        PropertyValue::Int(value) => ("int", json!(value)),
        PropertyValue::Float(value) => ("float", json!(value)),
        PropertyValue::String(value) => ("string", json!(value)),
        PropertyValue::Color(color) => ("color", json!(tiled_color(*color))),
        PropertyValue::File(path) => ("file", json!(path.to_string_lossy().replace('\\', "/"))),
        PropertyValue::Object(value) => ("object", json!(value)),
    }
}

fn infer_image_size(palette: &TilePalette) -> [u32; 2] {
    let mut width = 1u32;
    let mut height = 1u32;
    for tile in &palette.tiles {
        width = width.max(tile.source_rect.x.saturating_add(tile.source_rect.width));
        height = height.max(tile.source_rect.y.saturating_add(tile.source_rect.height));
    }
    [width, height]
}

fn shared_tile_offset(tiles: &[TileDef]) -> Option<[i32; 2]> {
    let first = tiles.first()?.draw_offset;
    tiles
        .iter()
        .all(|tile| tile.draw_offset == first)
        .then_some(first)
}

fn tiled_color(color: Color) -> String {
    let r = color_channel_to_u8(color.r);
    let g = color_channel_to_u8(color.g);
    let b = color_channel_to_u8(color.b);
    let a = color_channel_to_u8(color.a);
    format!("#{a:02X}{r:02X}{g:02X}{b:02X}")
}

fn color_channel_to_u8(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn is_white(color: Color) -> bool {
    color.to_array() == Color::WHITE.to_array()
}

fn orientation_name(orientation: GridOrientation) -> &'static str {
    match orientation {
        GridOrientation::Orthogonal => "orthogonal",
        GridOrientation::Isometric => "isometric",
        GridOrientation::Staggered => "staggered",
        GridOrientation::Hexagonal => "hexagonal",
    }
}

fn render_order_name(order: crate::tile::TileRenderOrder) -> &'static str {
    match order {
        crate::tile::TileRenderOrder::RightDown => "right-down",
        crate::tile::TileRenderOrder::RightUp => "right-up",
        crate::tile::TileRenderOrder::LeftDown => "left-down",
        crate::tile::TileRenderOrder::LeftUp => "left-up",
    }
}

fn stagger_axis_name(axis: Option<crate::tile::StaggerAxis>) -> Option<&'static str> {
    match axis {
        Some(crate::tile::StaggerAxis::X) => Some("x"),
        Some(crate::tile::StaggerAxis::Y) => Some("y"),
        None => None,
    }
}

fn stagger_index_name(index: Option<crate::tile::StaggerIndex>) -> Option<&'static str> {
    match index {
        Some(crate::tile::StaggerIndex::Odd) => Some("odd"),
        Some(crate::tile::StaggerIndex::Even) => Some("even"),
        None => None,
    }
}
