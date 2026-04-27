use std::collections::BTreeMap;
use std::fmt;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::asset::{Handle, TextureAsset, TextureColorSpace};
use crate::render::component::{
    TileAnimation, TileAnimationFrame, TilemapDepthSort, TilemapOrientation, TilemapRenderOrder,
    TilemapRenderer, TilemapStaggerAxis, TilemapStaggerIndex, TilesetGrid, TilesetTileRect,
};
use crate::render::Color;

use super::{Tile, TileFlags, TileId, Tilemap, TilemapDescriptor, TilemapHandle};

const FLIPPED_HORIZONTALLY_FLAG: u32 = 0x8000_0000;
const FLIPPED_VERTICALLY_FLAG: u32 = 0x4000_0000;
const FLIPPED_DIAGONALLY_FLAG: u32 = 0x2000_0000;
const ROTATED_HEXAGONAL_120_FLAG: u32 = 0x1000_0000;
const GID_MASK: u32 = !(FLIPPED_HORIZONTALLY_FLAG
    | FLIPPED_VERTICALLY_FLAG
    | FLIPPED_DIAGONALLY_FLAG
    | ROTATED_HEXAGONAL_120_FLAG);

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

impl TiledImport {
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, TiledImportError> {
        let path = path.as_ref();
        match path
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str()
        {
            "json" | "tmj" => Self::from_json_file(path),
            "tmx" => Self::from_tmx_file(path),
            _ => Err(TiledImportError::UnsupportedFileExtension {
                path: path.to_path_buf(),
            }),
        }
    }

    pub fn from_json_file(path: impl AsRef<Path>) -> Result<Self, TiledImportError> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path).map_err(|source| TiledImportError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let base_dir = path.parent().unwrap_or_else(|| Path::new("."));
        Self::from_json_str(&text, base_dir)
    }

    pub fn from_json_str(text: &str, base_dir: impl AsRef<Path>) -> Result<Self, TiledImportError> {
        let raw: TiledJsonMap = serde_json::from_str(text).map_err(TiledImportError::Json)?;
        Self::from_raw(raw, base_dir.as_ref())
    }

    pub fn from_tmx_file(path: impl AsRef<Path>) -> Result<Self, TiledImportError> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path).map_err(|source| TiledImportError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let base_dir = path.parent().unwrap_or_else(|| Path::new("."));
        Self::from_tmx_str(&text, base_dir)
    }

    pub fn from_tmx_str(text: &str, base_dir: impl AsRef<Path>) -> Result<Self, TiledImportError> {
        let document = roxmltree::Document::parse(text).map_err(TiledImportError::Xml)?;
        let root = document.root_element();
        if !root.has_tag_name("map") {
            return Err(TiledImportError::MalformedMap(
                "TMX root element must be <map>".to_string(),
            ));
        }

        let orientation = parse_orientation(required_attr(root, "orientation")?)?;
        let stagger_axis = parse_stagger_axis(root.attribute("staggeraxis").unwrap_or("y"))?;
        let stagger_index = parse_stagger_index(root.attribute("staggerindex").unwrap_or("odd"))?;
        let render_order =
            parse_render_order(root.attribute("renderorder").unwrap_or("right-down"))?;
        let width = required_u32_attr(root, "width")?;
        let height = required_u32_attr(root, "height")?;
        let tile_width = required_u32_attr(root, "tilewidth")?;
        let tile_height = required_u32_attr(root, "tileheight")?;
        let hex_side_length = optional_u32_attr(root, "hexsidelength")?.unwrap_or_default();
        let infinite = optional_bool_attr(root, "infinite")?.unwrap_or(false);
        let parallax_origin = [
            optional_f32_attr(root, "parallaxoriginx")?.unwrap_or_default(),
            optional_f32_attr(root, "parallaxoriginy")?.unwrap_or_default(),
        ];
        let tilesets = resolve_tmx_tilesets(root, base_dir.as_ref())?;

        let mut layers = Vec::new();
        let mut object_layers = Vec::new();
        let mut source_order = 0i32;
        collect_tmx_child_layers(
            root,
            LayerContext::default(),
            &mut source_order,
            &mut layers,
            &mut object_layers,
            base_dir.as_ref(),
        )?;

        Self::from_parts(ParsedMap {
            orientation,
            stagger_axis,
            stagger_index,
            hex_side_length,
            render_order,
            width,
            height,
            tile_width,
            tile_height,
            infinite,
            parallax_origin,
            properties: collect_tmx_properties(root, base_dir.as_ref())?,
            layers,
            object_layers,
            tilesets,
        })
    }

    pub fn primary_tileset(&self) -> &TiledTileset {
        self.tilesets.first().unwrap_or(&self.tileset)
    }

    pub fn load_tileset_texture(&self) -> Result<TextureAsset, TiledImportError> {
        load_tiled_tileset_texture(self.primary_tileset())
    }

    pub fn load_tileset_textures(&self) -> Result<Vec<TextureAsset>, TiledImportError> {
        self.tilesets
            .iter()
            .map(load_tiled_tileset_texture)
            .collect()
    }

    pub(crate) fn tileset_grid_for(
        &self,
        tileset_index: usize,
        texture: Handle<TextureAsset>,
    ) -> Option<TilesetGrid> {
        let tileset = self.tilesets.get(tileset_index)?;
        Some(
            TilesetGrid::new(texture, tileset.tile_size, tileset.columns, tileset.rows)
                .texture_size(tileset.image_size)
                .margin_spacing(tileset.margin, tileset.spacing)
                .tile_rects(tileset.tile_rects.clone())
                .animations(tileset.animations.clone()),
        )
    }

    #[inline]
    pub fn tileset_grid(&self, texture: Handle<TextureAsset>) -> TilesetGrid {
        self.tileset_grid_for(0, texture).unwrap_or_else(|| {
            TilesetGrid::new(
                texture,
                self.tileset.tile_size,
                self.tileset.columns,
                self.tileset.rows,
            )
            .texture_size(self.tileset.image_size)
            .margin_spacing(self.tileset.margin, self.tileset.spacing)
            .tile_rects(self.tileset.tile_rects.clone())
            .animations(self.tileset.animations.clone())
        })
    }

    pub fn renderer_for_layer(
        &self,
        map: TilemapHandle,
        textures: &[Handle<TextureAsset>],
        layer: u32,
    ) -> Option<TilemapRenderer> {
        let layer_info = self.layers.get(layer as usize)?;
        let texture = *textures.get(layer_info.tileset_index)?;
        let tileset = self.tilesets.get(layer_info.tileset_index)?;
        let mut renderer = TilemapRenderer::new(
            map,
            self.tileset_grid_for(layer_info.tileset_index, texture)?,
        )
        .layer(layer_info.storage_layer)
        .tile_size([self.tile_size[0] as f32, self.tile_size[1] as f32])
        .tile_draw_size([tileset.tile_size[0] as f32, tileset.tile_size[1] as f32])
        .tile_offset([
            tileset.tile_offset[0] as f32,
            -tileset.tile_offset[1] as f32,
        ])
        .orientation(self.orientation)
        .stagger_axis(self.stagger_axis)
        .stagger_index(self.renderer_stagger_index())
        .hex_side_length(self.hex_side_length as f32)
        .render_order(self.render_order)
        .depth_sort(self.depth_sort)
        .visible(layer_info.visible);
        if layer_info.opacity < 1.0 {
            renderer = renderer.color(Color::new(1.0, 1.0, 1.0, layer_info.opacity));
        }
        Some(renderer)
    }

    pub fn renderer_for_layer_with_texture(
        &self,
        map: TilemapHandle,
        texture: Handle<TextureAsset>,
        layer: u32,
    ) -> Option<TilemapRenderer> {
        self.renderer_for_layer(map, &[texture], layer)
    }
}

fn load_tiled_tileset_texture(tileset: &TiledTileset) -> Result<TextureAsset, TiledImportError> {
    let image = image::open(&tileset.image).map_err(|source| TiledImportError::Image {
        path: tileset.image.clone(),
        source,
    })?;
    let mut image = image.to_rgba8();
    if let Some([r, g, b]) = tileset.transparent_color {
        for pixel in image.as_mut().chunks_exact_mut(4) {
            if pixel[0] == r && pixel[1] == g && pixel[2] == b {
                pixel[3] = 0;
            }
        }
    }
    let (width, height) = image.dimensions();
    Ok(TextureAsset::new(
        width,
        height,
        TextureColorSpace::Srgb,
        image.into_raw(),
    ))
}

impl TiledImport {
    fn from_raw(raw: TiledJsonMap, base_dir: &Path) -> Result<Self, TiledImportError> {
        let orientation = parse_orientation(&raw.orientation)?;
        let stagger_axis = parse_stagger_axis(&raw.staggeraxis)?;
        let stagger_index = parse_stagger_index(&raw.staggerindex)?;
        let render_order = parse_render_order(&raw.renderorder)?;
        let parsed_tilesets = resolve_tilesets(&raw.tilesets, base_dir)?;

        let mut layers = Vec::new();
        let mut object_layers = Vec::new();
        let mut source_order = 0i32;
        let map_pixel_height = raw.height as f32 * raw.tileheight as f32;
        collect_tile_layers(
            &raw.layers,
            LayerContext::default(),
            &mut source_order,
            &mut layers,
            &mut object_layers,
            base_dir,
            map_pixel_height,
        )?;

        Self::from_parts(ParsedMap {
            orientation,
            stagger_axis,
            stagger_index,
            hex_side_length: raw.hexsidelength,
            render_order,
            width: raw.width,
            height: raw.height,
            tile_width: raw.tilewidth,
            tile_height: raw.tileheight,
            infinite: raw.infinite,
            parallax_origin: [raw.parallaxoriginx, raw.parallaxoriginy],
            properties: collect_json_properties(&raw.properties, base_dir)?,
            layers,
            object_layers,
            tilesets: parsed_tilesets,
        })
    }

    fn from_parts(parts: ParsedMap) -> Result<Self, TiledImportError> {
        if parts.tilesets.is_empty() {
            return Err(TiledImportError::MissingTileset);
        }

        let primary_tileset = parts.tilesets[0].clone();
        let split_layers = split_layers_by_tileset(&parts.layers, &parts.tilesets)?;
        let (origin, source_width, source_height) =
            tilemap_bounds(parts.infinite, parts.width, parts.height, &parts.layers);
        let (width, height) = match parts.orientation {
            TilemapOrientation::Isometric => (source_height, source_width),
            _ => (source_width, source_height),
        };
        let mut map = Tilemap::new(TilemapDescriptor::new(
            width,
            height,
            split_layers.len() as u32,
        ));
        let map_pixel_height = source_height as f32 * parts.tile_height as f32;
        let parallax_origin = [
            parts.parallax_origin[0],
            map_pixel_height - parts.parallax_origin[1],
        ];

        let depth_sort = tiled_depth_sort_for_layers(
            parts.orientation,
            [parts.tile_width, parts.tile_height],
            &parts.tilesets,
            split_layers.len(),
        );
        let mut imported_layers = Vec::with_capacity(split_layers.len());
        for (layer_index, split_layer) in split_layers.into_iter().enumerate() {
            let parsed_layer = &parts.layers[split_layer.source_layer];
            let tileset = &parts.tilesets[split_layer.tileset_index];
            for cell in split_layer.cells {
                let Some((tile, x, y)) = decode_cell(
                    cell,
                    tileset,
                    parts.orientation,
                    origin,
                    source_width,
                    source_height,
                )?
                else {
                    continue;
                };
                let _ = map.set_tile(layer_index as u32, x, y, tile);
            }
            imported_layers.push(TiledLayer {
                name: parsed_layer.name.clone(),
                source_layer: split_layer.source_layer,
                tileset_index: split_layer.tileset_index,
                storage_layer: layer_index as u32,
                source_order: parsed_layer.source_order,
                sorting_layer: match depth_sort {
                    TilemapDepthSort::Layer => parsed_layer
                        .source_order
                        .saturating_mul(256)
                        .saturating_add(split_layer.split_order as i32),
                    TilemapDepthSort::YThenLayer => 0,
                },
                visible: parsed_layer.visible,
                opacity: parsed_layer.opacity,
                offset: parsed_layer.offset,
                parallax: parsed_layer.parallax,
                properties: parsed_layer.properties.clone(),
            });
        }
        let mut object_layers = Vec::with_capacity(parts.object_layers.len());
        for parsed_layer in parts.object_layers {
            let mut objects = Vec::with_capacity(parsed_layer.objects.len());
            for object in parsed_layer.objects {
                if let Some(object) = decode_object(object, &parts.tilesets, map_pixel_height)? {
                    objects.push(object);
                }
            }
            object_layers.push(TiledObjectLayer {
                name: parsed_layer.name,
                source_order: parsed_layer.source_order,
                sorting_layer: parsed_layer.source_order,
                visible: parsed_layer.visible,
                opacity: parsed_layer.opacity,
                offset: parsed_layer.offset,
                parallax: parsed_layer.parallax,
                properties: parsed_layer.properties,
                objects,
            });
        }

        Ok(Self {
            map,
            orientation: parts.orientation,
            stagger_axis: parts.stagger_axis,
            stagger_index: parts.stagger_index,
            hex_side_length: parts.hex_side_length,
            render_order: parts.render_order,
            depth_sort,
            tile_size: [parts.tile_width, parts.tile_height],
            tile_origin: [origin.0, origin.1],
            parallax_origin,
            properties: parts.properties,
            tilesets: parts.tilesets,
            tileset: primary_tileset,
            layers: imported_layers,
            object_layers,
        })
    }

    fn renderer_stagger_index(&self) -> TilemapStaggerIndex {
        if !matches!(
            self.orientation,
            TilemapOrientation::Staggered | TilemapOrientation::Hexagonal
        ) {
            return self.stagger_index;
        }

        let parity_source = match self.stagger_axis {
            TilemapStaggerAxis::X => self.tile_origin[0],
            TilemapStaggerAxis::Y => {
                self.tile_origin[1].saturating_add(self.map.height().saturating_sub(1) as i32)
            }
        };
        if parity_source & 1 != 0 {
            self.stagger_index.inverted()
        } else {
            self.stagger_index
        }
    }
}

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

/// Single image tileset metadata imported from Tiled.
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

#[derive(Debug)]
pub enum TiledImportError {
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    UnsupportedFileExtension {
        path: PathBuf,
    },
    Json(serde_json::Error),
    Xml(roxmltree::Error),
    Image {
        path: PathBuf,
        source: image::ImageError,
    },
    DecodeLayerData {
        layer: String,
        source: base64::DecodeError,
    },
    InflateLayerData {
        layer: String,
        source: std::io::Error,
    },
    MalformedMap(String),
    UnsupportedOrientation(String),
    UnsupportedLayerData {
        layer: String,
        reason: &'static str,
    },
    UnsupportedExternalTileset {
        source: PathBuf,
    },
    UnsupportedTileset {
        source: Option<PathBuf>,
        reason: &'static str,
    },
    MissingTileset,
    MultipleTilesetsUsed,
    TileGidOutOfRange {
        gid: u32,
    },
    UnsupportedTileFlip {
        gid: u32,
    },
}

impl fmt::Display for TiledImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => {
                write!(f, "failed to read Tiled file {}: {source}", path.display())
            }
            Self::UnsupportedFileExtension { path } => {
                write!(f, "unsupported Tiled file extension for {}", path.display())
            }
            Self::Json(error) => write!(f, "failed to parse Tiled JSON: {error}"),
            Self::Xml(error) => write!(f, "failed to parse Tiled TMX: {error}"),
            Self::Image { path, source } => {
                write!(
                    f,
                    "failed to load Tiled tileset image {}: {source}",
                    path.display()
                )
            }
            Self::DecodeLayerData { layer, source } => {
                write!(
                    f,
                    "failed to decode base64 tile data for `{layer}`: {source}"
                )
            }
            Self::InflateLayerData { layer, source } => {
                write!(f, "failed to decompress tile data for `{layer}`: {source}")
            }
            Self::MalformedMap(reason) => write!(f, "malformed Tiled map: {reason}"),
            Self::UnsupportedOrientation(orientation) => {
                write!(f, "unsupported Tiled orientation `{orientation}`")
            }
            Self::UnsupportedLayerData { layer, reason } => {
                write!(f, "unsupported Tiled layer `{layer}`: {reason}")
            }
            Self::UnsupportedExternalTileset { source } => {
                write!(f, "unsupported external Tiled tileset {}", source.display())
            }
            Self::UnsupportedTileset { source, reason } => match source {
                Some(source) => {
                    write!(
                        f,
                        "unsupported Tiled tileset {}: {reason}",
                        source.display()
                    )
                }
                None => write!(f, "unsupported embedded Tiled tileset: {reason}"),
            },
            Self::MissingTileset => write!(f, "Tiled map does not contain a usable tileset"),
            Self::MultipleTilesetsUsed => {
                write!(f, "Tiled map uses multiple tilesets in tile layers")
            }
            Self::TileGidOutOfRange { gid } => {
                write!(
                    f,
                    "Tiled tile gid {gid} does not belong to the imported tileset"
                )
            }
            Self::UnsupportedTileFlip { gid } => {
                write!(f, "Tiled tile gid {gid} uses diagonal/rotation flags")
            }
        }
    }
}

impl std::error::Error for TiledImportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Json(source) => Some(source),
            Self::Xml(source) => Some(source),
            Self::Image { source, .. } => Some(source),
            Self::DecodeLayerData { source, .. } => Some(source),
            Self::InflateLayerData { source, .. } => Some(source),
            _ => None,
        }
    }
}

#[derive(Deserialize)]
struct TiledJsonMap {
    orientation: String,
    #[serde(default = "default_render_order")]
    renderorder: String,
    #[serde(default = "default_stagger_axis")]
    staggeraxis: String,
    #[serde(default = "default_stagger_index")]
    staggerindex: String,
    #[serde(default)]
    hexsidelength: u32,
    width: u32,
    height: u32,
    tilewidth: u32,
    tileheight: u32,
    #[serde(default)]
    infinite: bool,
    #[serde(default)]
    parallaxoriginx: f32,
    #[serde(default)]
    parallaxoriginy: f32,
    #[serde(default)]
    layers: Vec<TiledJsonLayer>,
    #[serde(default)]
    tilesets: Vec<TiledJsonTilesetRef>,
    #[serde(default)]
    properties: Vec<TiledJsonProperty>,
}

#[derive(Deserialize)]
struct TiledJsonLayer {
    name: String,
    #[serde(rename = "type")]
    layer_type: String,
    #[serde(default)]
    x: i32,
    #[serde(default)]
    y: i32,
    width: Option<u32>,
    height: Option<u32>,
    #[serde(default = "default_visible")]
    visible: bool,
    #[serde(default = "default_opacity")]
    opacity: f32,
    #[serde(default)]
    offsetx: f32,
    #[serde(default)]
    offsety: f32,
    #[serde(default = "default_parallax")]
    parallaxx: f32,
    #[serde(default = "default_parallax")]
    parallaxy: f32,
    #[serde(default)]
    encoding: Option<String>,
    #[serde(default)]
    compression: Option<String>,
    #[serde(default)]
    data: Option<TiledLayerData>,
    #[serde(default)]
    chunks: Vec<TiledJsonChunk>,
    #[serde(default)]
    layers: Vec<TiledJsonLayer>,
    #[serde(default)]
    objects: Vec<TiledJsonObject>,
    #[serde(default)]
    properties: Vec<TiledJsonProperty>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum TiledLayerData {
    Array(Vec<u32>),
    Encoded(String),
}

#[derive(Deserialize)]
struct TiledJsonChunk {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    #[serde(default)]
    encoding: Option<String>,
    #[serde(default)]
    compression: Option<String>,
    data: TiledLayerData,
}

#[derive(Deserialize)]
struct TiledJsonTilesetRef {
    firstgid: u32,
    source: Option<String>,
    image: Option<String>,
    tilewidth: Option<u32>,
    tileheight: Option<u32>,
    columns: Option<u32>,
    tilecount: Option<u32>,
    imagewidth: Option<u32>,
    imageheight: Option<u32>,
    transparentcolor: Option<String>,
    tileoffset: Option<TiledJsonTileOffset>,
    #[serde(default)]
    tiles: Vec<TiledJsonTile>,
    #[serde(default)]
    margin: u32,
    #[serde(default)]
    spacing: u32,
    #[serde(default)]
    properties: Vec<TiledJsonProperty>,
}

#[derive(Deserialize)]
struct TiledJsonTilesetFile {
    image: Option<String>,
    tilewidth: Option<u32>,
    tileheight: Option<u32>,
    columns: Option<u32>,
    tilecount: Option<u32>,
    imagewidth: Option<u32>,
    imageheight: Option<u32>,
    transparentcolor: Option<String>,
    tileoffset: Option<TiledJsonTileOffset>,
    #[serde(default)]
    tiles: Vec<TiledJsonTile>,
    #[serde(default)]
    margin: u32,
    #[serde(default)]
    spacing: u32,
    #[serde(default)]
    properties: Vec<TiledJsonProperty>,
}

#[derive(Deserialize)]
struct TiledJsonTile {
    id: u32,
    #[serde(default)]
    animation: Vec<TiledJsonAnimationFrame>,
    #[serde(default)]
    properties: Vec<TiledJsonProperty>,
}

#[derive(Deserialize)]
struct TiledJsonObject {
    id: u32,
    #[serde(default)]
    name: String,
    #[serde(default, rename = "type")]
    object_type: String,
    #[serde(default, rename = "class")]
    class_name: String,
    #[serde(default)]
    x: f32,
    #[serde(default)]
    y: f32,
    #[serde(default)]
    width: f32,
    #[serde(default)]
    height: f32,
    #[serde(default)]
    gid: Option<u32>,
    #[serde(default)]
    point: bool,
    #[serde(default)]
    ellipse: bool,
    #[serde(default)]
    polygon: Vec<TiledJsonPoint>,
    #[serde(default)]
    polyline: Vec<TiledJsonPoint>,
    #[serde(default)]
    template: Option<String>,
    #[serde(default)]
    properties: Vec<TiledJsonProperty>,
}

#[derive(Deserialize)]
struct TiledJsonPoint {
    x: f32,
    y: f32,
}

#[derive(Deserialize)]
struct TiledJsonProperty {
    name: String,
    #[serde(default, rename = "type")]
    value_type: Option<String>,
    #[serde(default)]
    value: serde_json::Value,
}

#[derive(Deserialize)]
struct TiledJsonTileOffset {
    #[serde(default)]
    x: i32,
    #[serde(default)]
    y: i32,
}

#[derive(Deserialize)]
struct TiledJsonAnimationFrame {
    tileid: u32,
    duration: u32,
}

#[derive(Clone, Copy)]
struct LayerContext {
    visible: bool,
    opacity: f32,
    offset: [f32; 2],
    parallax: [f32; 2],
}

impl Default for LayerContext {
    fn default() -> Self {
        Self {
            visible: true,
            opacity: 1.0,
            offset: [0.0, 0.0],
            parallax: [1.0, 1.0],
        }
    }
}

#[derive(Clone, Debug)]
struct ParsedLayer {
    name: String,
    source_order: i32,
    visible: bool,
    opacity: f32,
    offset: [f32; 2],
    parallax: [f32; 2],
    properties: Vec<TiledProperty>,
    cells: Vec<RawCell>,
}

#[derive(Clone, Debug)]
struct ParsedObjectLayer {
    name: String,
    source_order: i32,
    visible: bool,
    opacity: f32,
    offset: [f32; 2],
    parallax: [f32; 2],
    properties: Vec<TiledProperty>,
    objects: Vec<RawObject>,
}

#[derive(Clone, Copy, Debug)]
struct RawCell {
    x: i32,
    y: i32,
    gid: u32,
}

#[derive(Clone, Debug)]
struct RawObject {
    id: u32,
    name: String,
    class: String,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    gid: Option<u32>,
    shape: RawObjectShape,
    properties: Vec<TiledProperty>,
    template: Option<PathBuf>,
}

#[derive(Clone, Debug)]
enum RawObjectShape {
    Rectangle,
    Point,
    Ellipse,
    Polygon(Vec<[f32; 2]>),
    Polyline(Vec<[f32; 2]>),
    Tile,
}

struct ParsedMap {
    orientation: TilemapOrientation,
    stagger_axis: TilemapStaggerAxis,
    stagger_index: TilemapStaggerIndex,
    hex_side_length: u32,
    render_order: TilemapRenderOrder,
    width: u32,
    height: u32,
    tile_width: u32,
    tile_height: u32,
    infinite: bool,
    parallax_origin: [f32; 2],
    properties: Vec<TiledProperty>,
    layers: Vec<ParsedLayer>,
    object_layers: Vec<ParsedObjectLayer>,
    tilesets: Vec<TiledTileset>,
}

struct LayerSplit {
    source_layer: usize,
    tileset_index: usize,
    split_order: usize,
    cells: Vec<RawCell>,
}

fn default_visible() -> bool {
    true
}

fn default_opacity() -> f32 {
    1.0
}

fn default_parallax() -> f32 {
    1.0
}

fn default_render_order() -> String {
    "right-down".to_string()
}

fn default_stagger_axis() -> String {
    "y".to_string()
}

fn default_stagger_index() -> String {
    "odd".to_string()
}

fn required_attr<'a, 'input>(
    node: roxmltree::Node<'a, 'input>,
    name: &str,
) -> Result<&'a str, TiledImportError> {
    node.attribute(name).ok_or_else(|| {
        TiledImportError::MalformedMap(format!(
            "<{}> is missing required `{}` attribute",
            node.tag_name().name(),
            name
        ))
    })
}

fn required_u32_attr(node: roxmltree::Node<'_, '_>, name: &str) -> Result<u32, TiledImportError> {
    parse_u32_attr(node, name, required_attr(node, name)?)
}

fn optional_u32_attr(
    node: roxmltree::Node<'_, '_>,
    name: &str,
) -> Result<Option<u32>, TiledImportError> {
    node.attribute(name)
        .map(|value| parse_u32_attr(node, name, value))
        .transpose()
}

fn parse_u32_attr(
    node: roxmltree::Node<'_, '_>,
    name: &str,
    value: &str,
) -> Result<u32, TiledImportError> {
    value.parse::<u32>().map_err(|_| {
        TiledImportError::MalformedMap(format!(
            "<{}> attribute `{}` must be an unsigned integer",
            node.tag_name().name(),
            name
        ))
    })
}

fn required_i32_attr(node: roxmltree::Node<'_, '_>, name: &str) -> Result<i32, TiledImportError> {
    parse_i32_attr(node, name, required_attr(node, name)?)
}

fn optional_i32_attr(
    node: roxmltree::Node<'_, '_>,
    name: &str,
) -> Result<Option<i32>, TiledImportError> {
    node.attribute(name)
        .map(|value| parse_i32_attr(node, name, value))
        .transpose()
}

fn parse_i32_attr(
    node: roxmltree::Node<'_, '_>,
    name: &str,
    value: &str,
) -> Result<i32, TiledImportError> {
    value.parse::<i32>().map_err(|_| {
        TiledImportError::MalformedMap(format!(
            "<{}> attribute `{}` must be an integer",
            node.tag_name().name(),
            name
        ))
    })
}

fn optional_f32_attr(
    node: roxmltree::Node<'_, '_>,
    name: &str,
) -> Result<Option<f32>, TiledImportError> {
    node.attribute(name)
        .map(|value| {
            value.parse::<f32>().map_err(|_| {
                TiledImportError::MalformedMap(format!(
                    "<{}> attribute `{}` must be a number",
                    node.tag_name().name(),
                    name
                ))
            })
        })
        .transpose()
}

fn optional_bool_attr(
    node: roxmltree::Node<'_, '_>,
    name: &str,
) -> Result<Option<bool>, TiledImportError> {
    node.attribute(name)
        .map(|value| match value {
            "1" | "true" => Ok(true),
            "0" | "false" => Ok(false),
            _ => Err(TiledImportError::MalformedMap(format!(
                "<{}> attribute `{}` must be 0/1 or true/false",
                node.tag_name().name(),
                name
            ))),
        })
        .transpose()
}

fn parse_transparent_color(value: Option<&str>) -> Result<Option<[u8; 3]>, TiledImportError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value.trim().trim_start_matches('#');
    if value.len() != 6 {
        return Err(TiledImportError::MalformedMap(format!(
            "transparent color `{value}` must be a 6-digit RGB hex value"
        )));
    }
    let r = u8::from_str_radix(&value[0..2], 16).map_err(|_| {
        TiledImportError::MalformedMap(format!("transparent color `{value}` is invalid"))
    })?;
    let g = u8::from_str_radix(&value[2..4], 16).map_err(|_| {
        TiledImportError::MalformedMap(format!("transparent color `{value}` is invalid"))
    })?;
    let b = u8::from_str_radix(&value[4..6], 16).map_err(|_| {
        TiledImportError::MalformedMap(format!("transparent color `{value}` is invalid"))
    })?;
    Ok(Some([r, g, b]))
}

fn parse_orientation(value: &str) -> Result<TilemapOrientation, TiledImportError> {
    match value {
        "orthogonal" => Ok(TilemapOrientation::Orthogonal),
        "isometric" => Ok(TilemapOrientation::Isometric),
        "staggered" => Ok(TilemapOrientation::Staggered),
        "hexagonal" => Ok(TilemapOrientation::Hexagonal),
        other => Err(TiledImportError::UnsupportedOrientation(other.to_string())),
    }
}

fn parse_stagger_axis(value: &str) -> Result<TilemapStaggerAxis, TiledImportError> {
    match value {
        "x" => Ok(TilemapStaggerAxis::X),
        "y" => Ok(TilemapStaggerAxis::Y),
        _ => Err(TiledImportError::UnsupportedLayerData {
            layer: "map".to_string(),
            reason: "unsupported staggeraxis; expected x or y",
        }),
    }
}

fn parse_stagger_index(value: &str) -> Result<TilemapStaggerIndex, TiledImportError> {
    match value {
        "odd" => Ok(TilemapStaggerIndex::Odd),
        "even" => Ok(TilemapStaggerIndex::Even),
        _ => Err(TiledImportError::UnsupportedLayerData {
            layer: "map".to_string(),
            reason: "unsupported staggerindex; expected odd or even",
        }),
    }
}

fn parse_render_order(value: &str) -> Result<TilemapRenderOrder, TiledImportError> {
    match value {
        "right-down" => Ok(TilemapRenderOrder::RightDown),
        "right-up" => Ok(TilemapRenderOrder::RightUp),
        "left-down" => Ok(TilemapRenderOrder::LeftDown),
        "left-up" => Ok(TilemapRenderOrder::LeftUp),
        other => Err(TiledImportError::UnsupportedLayerData {
            layer: "map".to_string(),
            reason: match other {
                "" => "renderorder is empty",
                _ => {
                    "unsupported renderorder; expected right-down, right-up, left-down, or left-up"
                }
            },
        }),
    }
}

fn collect_tile_layers(
    raw_layers: &[TiledJsonLayer],
    context: LayerContext,
    source_order: &mut i32,
    out: &mut Vec<ParsedLayer>,
    object_layers: &mut Vec<ParsedObjectLayer>,
    base_dir: &Path,
    map_pixel_height: f32,
) -> Result<(), TiledImportError> {
    for raw in raw_layers {
        let child_context = LayerContext {
            visible: context.visible && raw.visible,
            opacity: (context.opacity * raw.opacity).clamp(0.0, 1.0),
            offset: [
                context.offset[0] + raw.offsetx,
                context.offset[1] + raw.offsety,
            ],
            parallax: [
                context.parallax[0] * raw.parallaxx,
                context.parallax[1] * raw.parallaxy,
            ],
        };
        match raw.layer_type.as_str() {
            "tilelayer" => {
                let order = *source_order;
                *source_order = source_order.saturating_add(1);
                out.push(ParsedLayer {
                    name: raw.name.clone(),
                    source_order: order,
                    visible: child_context.visible,
                    opacity: child_context.opacity,
                    offset: child_context.offset,
                    parallax: child_context.parallax,
                    properties: collect_json_properties(&raw.properties, base_dir)?,
                    cells: collect_layer_cells(raw)?,
                });
            }
            "objectgroup" => {
                let order = *source_order;
                *source_order = source_order.saturating_add(1);
                let objects = raw
                    .objects
                    .iter()
                    .map(|object| collect_json_object(object, map_pixel_height, base_dir))
                    .collect::<Result<Vec<_>, _>>()?;
                object_layers.push(ParsedObjectLayer {
                    name: raw.name.clone(),
                    source_order: order,
                    visible: child_context.visible,
                    opacity: child_context.opacity,
                    offset: child_context.offset,
                    parallax: child_context.parallax,
                    properties: collect_json_properties(&raw.properties, base_dir)?,
                    objects,
                });
            }
            "group" => {
                collect_tile_layers(
                    &raw.layers,
                    child_context,
                    source_order,
                    out,
                    object_layers,
                    base_dir,
                    map_pixel_height,
                )?;
            }
            _ => {
                *source_order = source_order.saturating_add(1);
            }
        }
    }
    Ok(())
}

fn collect_layer_cells(layer: &TiledJsonLayer) -> Result<Vec<RawCell>, TiledImportError> {
    if !layer.chunks.is_empty() {
        let mut cells = Vec::new();
        for chunk in &layer.chunks {
            let data = data_array(
                &chunk.data,
                &layer.name,
                chunk.encoding.as_deref().or(layer.encoding.as_deref()),
                chunk
                    .compression
                    .as_deref()
                    .or(layer.compression.as_deref()),
            )?;
            let expected = chunk.width as usize * chunk.height as usize;
            if data.len() != expected {
                return Err(TiledImportError::UnsupportedLayerData {
                    layer: layer.name.clone(),
                    reason: "chunk data length does not match width * height",
                });
            }
            for local_y in 0..chunk.height {
                for local_x in 0..chunk.width {
                    let index = (local_y * chunk.width + local_x) as usize;
                    cells.push(RawCell {
                        x: chunk.x + local_x as i32,
                        y: chunk.y + local_y as i32,
                        gid: data[index],
                    });
                }
            }
        }
        return Ok(cells);
    }

    let Some(data) = &layer.data else {
        return Ok(Vec::new());
    };
    let data = data_array(
        data,
        &layer.name,
        layer.encoding.as_deref(),
        layer.compression.as_deref(),
    )?;
    let width = layer
        .width
        .ok_or_else(|| TiledImportError::UnsupportedLayerData {
            layer: layer.name.clone(),
            reason: "finite tile layer is missing width",
        })?;
    let height = layer
        .height
        .ok_or_else(|| TiledImportError::UnsupportedLayerData {
            layer: layer.name.clone(),
            reason: "finite tile layer is missing height",
        })?;
    let expected = width as usize * height as usize;
    if data.len() != expected {
        return Err(TiledImportError::UnsupportedLayerData {
            layer: layer.name.clone(),
            reason: "tile data length does not match width * height",
        });
    }

    let mut cells = Vec::with_capacity(data.len());
    for local_y in 0..height {
        for local_x in 0..width {
            let index = (local_y * width + local_x) as usize;
            cells.push(RawCell {
                x: layer.x + local_x as i32,
                y: layer.y + local_y as i32,
                gid: data[index],
            });
        }
    }
    Ok(cells)
}

fn data_array<'a>(
    data: &'a TiledLayerData,
    layer_name: &str,
    encoding: Option<&str>,
    compression: Option<&str>,
) -> Result<Vec<u32>, TiledImportError> {
    match data {
        TiledLayerData::Array(values) => Ok(values.clone()),
        TiledLayerData::Encoded(encoded) => match encoding {
            Some("base64") => decode_base64_gids(encoded, compression, layer_name),
            Some("csv") => parse_csv_gids(encoded, layer_name),
            Some(_) | None => Err(TiledImportError::UnsupportedLayerData {
                layer: layer_name.to_string(),
                reason: "encoded JSON tile data requires encoding `csv` or `base64`",
            }),
        },
    }
}

fn resolve_tmx_tilesets(
    map: roxmltree::Node<'_, '_>,
    base_dir: &Path,
) -> Result<Vec<TiledTileset>, TiledImportError> {
    let mut tilesets = Vec::new();
    for node in map
        .children()
        .filter(|node| node.is_element() && node.has_tag_name("tileset"))
    {
        let first_gid = required_u32_attr(node, "firstgid")?;
        if let Some(source) = node.attribute("source") {
            let source_path = resolve_path(base_dir, source);
            let extension = source_path
                .extension()
                .and_then(|extension| extension.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase();
            if extension != "tsx" {
                return Err(TiledImportError::UnsupportedExternalTileset {
                    source: source_path,
                });
            }
            tilesets.push(load_tmx_tileset_file(first_gid, &source_path)?);
        } else {
            tilesets.push(build_tmx_tileset(first_gid, node, base_dir, None)?);
        }
    }
    Ok(tilesets)
}

fn load_tmx_tileset_file(first_gid: u32, path: &Path) -> Result<TiledTileset, TiledImportError> {
    let text = std::fs::read_to_string(path).map_err(|source| TiledImportError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let document = roxmltree::Document::parse(&text).map_err(TiledImportError::Xml)?;
    let root = document.root_element();
    if !root.has_tag_name("tileset") {
        return Err(TiledImportError::MalformedMap(format!(
            "external tileset {} must use a <tileset> root",
            path.display()
        )));
    }
    let base_dir = path.parent().unwrap_or_else(|| Path::new("."));
    build_tmx_tileset(first_gid, root, base_dir, Some(path.to_path_buf()))
}

fn build_tmx_tileset(
    first_gid: u32,
    tileset: roxmltree::Node<'_, '_>,
    base_dir: &Path,
    source: Option<PathBuf>,
) -> Result<TiledTileset, TiledImportError> {
    let Some(image) = tileset
        .children()
        .find(|node| node.is_element() && node.has_tag_name("image"))
    else {
        return build_tmx_image_collection_tileset(first_gid, tileset, base_dir, source);
    };

    build_tileset(
        first_gid,
        image.attribute("source"),
        optional_u32_attr(tileset, "tilewidth")?,
        optional_u32_attr(tileset, "tileheight")?,
        optional_u32_attr(tileset, "columns")?,
        optional_u32_attr(tileset, "tilecount")?,
        optional_u32_attr(image, "width")?,
        optional_u32_attr(image, "height")?,
        parse_transparent_color(image.attribute("trans"))?,
        parse_tmx_tile_offset(tileset)?,
        collect_tmx_tile_animations(tileset)?,
        collect_tmx_properties(tileset, base_dir)?,
        collect_tmx_tile_properties(tileset, base_dir)?,
        optional_u32_attr(tileset, "margin")?.unwrap_or_default(),
        optional_u32_attr(tileset, "spacing")?.unwrap_or_default(),
        base_dir,
        source,
    )
}

fn build_tmx_image_collection_tileset(
    first_gid: u32,
    tileset: roxmltree::Node<'_, '_>,
    base_dir: &Path,
    source: Option<PathBuf>,
) -> Result<TiledTileset, TiledImportError> {
    if first_gid == 0 {
        return Err(TiledImportError::UnsupportedTileset {
            source,
            reason: "firstgid must be greater than zero",
        });
    }

    let tile_width = optional_u32_attr(tileset, "tilewidth")?.ok_or_else(|| {
        TiledImportError::UnsupportedTileset {
            source: source.clone(),
            reason: "tileset is missing tilewidth",
        }
    })?;
    let tile_height = optional_u32_attr(tileset, "tileheight")?.ok_or_else(|| {
        TiledImportError::UnsupportedTileset {
            source: source.clone(),
            reason: "tileset is missing tileheight",
        }
    })?;

    let mut image_source: Option<PathBuf> = None;
    let mut image_size = [0, 0];
    let mut max_tile_id = 0u32;
    let mut rects: Vec<Option<TilesetTileRect>> = Vec::new();

    for tile in tileset
        .children()
        .filter(|node| node.is_element() && node.has_tag_name("tile"))
    {
        let Some(image) = tile
            .children()
            .find(|node| node.is_element() && node.has_tag_name("image"))
        else {
            continue;
        };
        let image_path = resolve_path(base_dir, required_attr(image, "source")?);
        match &image_source {
            Some(existing) if existing != &image_path => {
                return Err(TiledImportError::UnsupportedTileset {
                    source,
                    reason: "image collection tilesets using multiple images are not supported",
                });
            }
            Some(_) => {}
            None => image_source = Some(image_path),
        }

        let tile_id = required_u32_attr(tile, "id")?;
        let x = optional_u32_attr(tile, "x")?.unwrap_or_default();
        let y = optional_u32_attr(tile, "y")?.unwrap_or_default();
        let source_width = optional_u32_attr(image, "width")?;
        let source_height = optional_u32_attr(image, "height")?;
        let width = optional_u32_attr(tile, "width")?
            .or(source_width)
            .unwrap_or(tile_width)
            .max(1);
        let height = optional_u32_attr(tile, "height")?
            .or(source_height)
            .unwrap_or(tile_height)
            .max(1);
        image_size[0] = image_size[0].max(source_width.unwrap_or_default());
        image_size[1] = image_size[1].max(source_height.unwrap_or_default());
        max_tile_id = max_tile_id.max(tile_id);
        if rects.len() <= tile_id as usize {
            rects.resize(tile_id as usize + 1, None);
        }
        rects[tile_id as usize] = Some(TilesetTileRect::new(x, y, width, height));
    }

    let image_path = image_source.ok_or_else(|| TiledImportError::UnsupportedTileset {
        source: source.clone(),
        reason: "only single-image tilesets are supported",
    })?;
    if image_size[0] == 0 || image_size[1] == 0 {
        let (width, height) =
            image::image_dimensions(&image_path).map_err(|source| TiledImportError::Image {
                path: image_path.clone(),
                source,
            })?;
        image_size = [width, height];
    }

    let animations = collect_tmx_tile_animations(tileset)?;
    let tile_count = max_tile_id.saturating_add(1).max(rects.len() as u32).max(1);
    if animations.iter().any(|animation| {
        animation.tile_id.0 >= tile_count
            || animation
                .frames
                .iter()
                .any(|frame| frame.tile_id.0 >= tile_count)
    }) {
        return Err(TiledImportError::UnsupportedTileset {
            source,
            reason: "tileset animation references a tile outside tilecount",
        });
    }

    Ok(TiledTileset {
        first_gid,
        image: image_path,
        tile_size: [tile_width, tile_height],
        columns: tile_count,
        rows: 1,
        tile_count,
        image_size,
        margin: 0,
        spacing: 0,
        tile_rects: rects,
        tile_offset: parse_tmx_tile_offset(tileset)?,
        animations,
        properties: collect_tmx_properties(tileset, base_dir)?,
        tile_properties: collect_tmx_tile_properties(tileset, base_dir)?,
        transparent_color: None,
    })
}

fn collect_tmx_tile_animations(
    tileset: roxmltree::Node<'_, '_>,
) -> Result<Vec<TileAnimation>, TiledImportError> {
    let mut animations = Vec::new();
    for tile in tileset
        .children()
        .filter(|node| node.is_element() && node.has_tag_name("tile"))
    {
        let Some(animation_node) = tile
            .children()
            .find(|node| node.is_element() && node.has_tag_name("animation"))
        else {
            continue;
        };
        let tile_id = required_u32_attr(tile, "id")?;
        let mut frames = Vec::new();
        for frame in animation_node
            .children()
            .filter(|node| node.is_element() && node.has_tag_name("frame"))
        {
            frames.push(TileAnimationFrame::new(
                TileId(required_u32_attr(frame, "tileid")?),
                required_u32_attr(frame, "duration")?,
            ));
        }
        if !frames.is_empty() {
            animations.push(TileAnimation::new(TileId(tile_id), frames));
        }
    }
    Ok(animations)
}

fn parse_tmx_tile_offset(tileset: roxmltree::Node<'_, '_>) -> Result<[i32; 2], TiledImportError> {
    let Some(offset) = tileset
        .children()
        .find(|node| node.is_element() && node.has_tag_name("tileoffset"))
    else {
        return Ok([0, 0]);
    };
    Ok([
        optional_i32_attr(offset, "x")?.unwrap_or_default(),
        optional_i32_attr(offset, "y")?.unwrap_or_default(),
    ])
}

fn json_tile_offset(offset: Option<&TiledJsonTileOffset>) -> [i32; 2] {
    offset.map_or([0, 0], |offset| [offset.x, offset.y])
}

fn collect_tmx_properties(
    node: roxmltree::Node<'_, '_>,
    base_dir: &Path,
) -> Result<Vec<TiledProperty>, TiledImportError> {
    let Some(properties) = node
        .children()
        .find(|child| child.is_element() && child.has_tag_name("properties"))
    else {
        return Ok(Vec::new());
    };
    properties
        .children()
        .filter(|child| child.is_element() && child.has_tag_name("property"))
        .map(|property| {
            let name = required_attr(property, "name")?.to_string();
            let value_type = property.attribute("type").unwrap_or("string");
            let value = property
                .attribute("value")
                .map(str::to_string)
                .unwrap_or_else(|| property.text().unwrap_or_default().to_string());
            Ok(TiledProperty {
                name,
                value: parse_property_value(value_type, &value, base_dir)?,
            })
        })
        .collect()
}

fn collect_tmx_tile_properties(
    tileset: roxmltree::Node<'_, '_>,
    base_dir: &Path,
) -> Result<Vec<Vec<TiledProperty>>, TiledImportError> {
    let mut tile_properties = Vec::new();
    for tile in tileset
        .children()
        .filter(|node| node.is_element() && node.has_tag_name("tile"))
    {
        let tile_id = required_u32_attr(tile, "id")? as usize;
        let properties = collect_tmx_properties(tile, base_dir)?;
        if properties.is_empty() {
            continue;
        }
        if tile_properties.len() <= tile_id {
            tile_properties.resize(tile_id + 1, Vec::new());
        }
        tile_properties[tile_id] = properties;
    }
    Ok(tile_properties)
}

fn collect_json_properties(
    properties: &[TiledJsonProperty],
    base_dir: &Path,
) -> Result<Vec<TiledProperty>, TiledImportError> {
    properties
        .iter()
        .map(|property| {
            let value_type = property.value_type.as_deref().unwrap_or("string");
            Ok(TiledProperty {
                name: property.name.clone(),
                value: parse_json_property_value(value_type, &property.value, base_dir)?,
            })
        })
        .collect()
}

fn collect_json_tile_properties(
    tiles: &[TiledJsonTile],
    base_dir: &Path,
) -> Result<Vec<Vec<TiledProperty>>, TiledImportError> {
    let mut tile_properties = Vec::new();
    for tile in tiles {
        let properties = collect_json_properties(&tile.properties, base_dir)?;
        if properties.is_empty() {
            continue;
        }
        let tile_id = tile.id as usize;
        if tile_properties.len() <= tile_id {
            tile_properties.resize(tile_id + 1, Vec::new());
        }
        tile_properties[tile_id] = properties;
    }
    Ok(tile_properties)
}

fn parse_property_value(
    value_type: &str,
    value: &str,
    base_dir: &Path,
) -> Result<TiledPropertyValue, TiledImportError> {
    match value_type {
        "bool" => Ok(TiledPropertyValue::Bool(matches!(value, "true" | "1"))),
        "int" => value
            .parse::<i32>()
            .map(TiledPropertyValue::Int)
            .map_err(|_| malformed_property(value_type, value)),
        "float" => value
            .parse::<f32>()
            .map(TiledPropertyValue::Float)
            .map_err(|_| malformed_property(value_type, value)),
        "color" => parse_tiled_color(value).map(TiledPropertyValue::Color),
        "file" => Ok(TiledPropertyValue::File(resolve_path(base_dir, value))),
        "object" => value
            .parse::<u32>()
            .map(TiledPropertyValue::Object)
            .map_err(|_| malformed_property(value_type, value)),
        _ => Ok(TiledPropertyValue::String(value.to_string())),
    }
}

fn parse_json_property_value(
    value_type: &str,
    value: &serde_json::Value,
    base_dir: &Path,
) -> Result<TiledPropertyValue, TiledImportError> {
    match value_type {
        "bool" => Ok(TiledPropertyValue::Bool(value.as_bool().unwrap_or(false))),
        "int" => Ok(TiledPropertyValue::Int(
            value.as_i64().unwrap_or_default() as i32
        )),
        "float" => Ok(TiledPropertyValue::Float(
            value.as_f64().unwrap_or_default() as f32,
        )),
        "color" => {
            parse_tiled_color(value.as_str().unwrap_or_default()).map(TiledPropertyValue::Color)
        }
        "file" => Ok(TiledPropertyValue::File(resolve_path(
            base_dir,
            value.as_str().unwrap_or_default(),
        ))),
        "object" => Ok(TiledPropertyValue::Object(
            value.as_u64().unwrap_or_default() as u32,
        )),
        _ => Ok(TiledPropertyValue::String(match value {
            serde_json::Value::String(value) => value.clone(),
            other => other.to_string(),
        })),
    }
}

fn parse_tiled_color(value: &str) -> Result<Color, TiledImportError> {
    let hex = value.strip_prefix('#').unwrap_or(value);
    let (a, r, g, b) = match hex.len() {
        6 => (
            255,
            u8::from_str_radix(&hex[0..2], 16),
            u8::from_str_radix(&hex[2..4], 16),
            u8::from_str_radix(&hex[4..6], 16),
        ),
        8 => (
            u8::from_str_radix(&hex[0..2], 16).unwrap_or(255),
            u8::from_str_radix(&hex[2..4], 16),
            u8::from_str_radix(&hex[4..6], 16),
            u8::from_str_radix(&hex[6..8], 16),
        ),
        _ => return Err(malformed_property("color", value)),
    };
    let r = r.map_err(|_| malformed_property("color", value))?;
    let g = g.map_err(|_| malformed_property("color", value))?;
    let b = b.map_err(|_| malformed_property("color", value))?;
    Ok(Color::new(
        r as f32 / 255.0,
        g as f32 / 255.0,
        b as f32 / 255.0,
        a as f32 / 255.0,
    ))
}

fn malformed_property(value_type: &str, value: &str) -> TiledImportError {
    TiledImportError::MalformedMap(format!(
        "property value `{value}` is not a valid {value_type}"
    ))
}

fn collect_json_tile_animations(tiles: &[TiledJsonTile]) -> Vec<TileAnimation> {
    tiles
        .iter()
        .filter_map(|tile| {
            if tile.animation.is_empty() {
                return None;
            }
            let frames = tile
                .animation
                .iter()
                .map(|frame| TileAnimationFrame::new(TileId(frame.tileid), frame.duration))
                .collect::<Vec<_>>();
            Some(TileAnimation::new(TileId(tile.id), frames))
        })
        .collect()
}

fn collect_tmx_child_layers(
    parent: roxmltree::Node<'_, '_>,
    context: LayerContext,
    source_order: &mut i32,
    out: &mut Vec<ParsedLayer>,
    object_layers: &mut Vec<ParsedObjectLayer>,
    base_dir: &Path,
) -> Result<(), TiledImportError> {
    for node in parent.children().filter(|node| node.is_element()) {
        match node.tag_name().name() {
            "layer" => {
                let child_context = layer_context_from_tmx(node, context)?;
                let order = *source_order;
                *source_order = source_order.saturating_add(1);
                out.push(ParsedLayer {
                    name: node.attribute("name").unwrap_or("Tile Layer").to_string(),
                    source_order: order,
                    visible: child_context.visible,
                    opacity: child_context.opacity,
                    offset: child_context.offset,
                    parallax: child_context.parallax,
                    properties: collect_tmx_properties(node, base_dir)?,
                    cells: collect_tmx_layer_cells(node)?,
                });
            }
            "group" => {
                let child_context = layer_context_from_tmx(node, context)?;
                collect_tmx_child_layers(
                    node,
                    child_context,
                    source_order,
                    out,
                    object_layers,
                    base_dir,
                )?;
            }
            "objectgroup" => {
                let child_context = layer_context_from_tmx(node, context)?;
                let order = *source_order;
                *source_order = source_order.saturating_add(1);
                let objects = collect_tmx_objects(node, base_dir)?;
                object_layers.push(ParsedObjectLayer {
                    name: node.attribute("name").unwrap_or("Object Layer").to_string(),
                    source_order: order,
                    visible: child_context.visible,
                    opacity: child_context.opacity,
                    offset: child_context.offset,
                    parallax: child_context.parallax,
                    properties: collect_tmx_properties(node, base_dir)?,
                    objects,
                });
            }
            "imagelayer" => {
                *source_order = source_order.saturating_add(1);
            }
            _ => {}
        }
    }
    Ok(())
}

fn layer_context_from_tmx(
    node: roxmltree::Node<'_, '_>,
    context: LayerContext,
) -> Result<LayerContext, TiledImportError> {
    Ok(LayerContext {
        visible: context.visible && optional_bool_attr(node, "visible")?.unwrap_or(true),
        opacity: (context.opacity * optional_f32_attr(node, "opacity")?.unwrap_or(1.0))
            .clamp(0.0, 1.0),
        offset: [
            context.offset[0] + optional_f32_attr(node, "offsetx")?.unwrap_or(0.0),
            context.offset[1] + optional_f32_attr(node, "offsety")?.unwrap_or(0.0),
        ],
        parallax: [
            context.parallax[0] * optional_f32_attr(node, "parallaxx")?.unwrap_or(1.0),
            context.parallax[1] * optional_f32_attr(node, "parallaxy")?.unwrap_or(1.0),
        ],
    })
}

fn collect_tmx_objects(
    object_group: roxmltree::Node<'_, '_>,
    base_dir: &Path,
) -> Result<Vec<RawObject>, TiledImportError> {
    let mut objects = Vec::new();
    for object in object_group
        .children()
        .filter(|node| node.is_element() && node.has_tag_name("object"))
    {
        let gid = optional_u32_attr(object, "gid")?;
        let shape = raw_object_shape_from_tmx(object, gid.is_some())?;
        objects.push(RawObject {
            id: required_u32_attr(object, "id")?,
            name: object.attribute("name").unwrap_or_default().to_string(),
            class: object
                .attribute("class")
                .or_else(|| object.attribute("type"))
                .unwrap_or_default()
                .to_string(),
            x: optional_f32_attr(object, "x")?.unwrap_or_default(),
            y: optional_f32_attr(object, "y")?.unwrap_or_default(),
            width: optional_f32_attr(object, "width")?.unwrap_or_default(),
            height: optional_f32_attr(object, "height")?.unwrap_or_default(),
            gid,
            shape,
            properties: collect_tmx_properties(object, base_dir)?,
            template: object
                .attribute("template")
                .map(|template| resolve_path(base_dir, template)),
        });
    }
    Ok(objects)
}

fn collect_json_object(
    object: &TiledJsonObject,
    _map_pixel_height: f32,
    base_dir: &Path,
) -> Result<RawObject, TiledImportError> {
    Ok(RawObject {
        id: object.id,
        name: object.name.clone(),
        class: if object.class_name.is_empty() {
            object.object_type.clone()
        } else {
            object.class_name.clone()
        },
        x: object.x,
        y: object.y,
        width: object.width,
        height: object.height,
        gid: object.gid,
        shape: raw_object_shape_from_json(object),
        properties: collect_json_properties(&object.properties, base_dir)?,
        template: object
            .template
            .as_deref()
            .map(|template| resolve_path(base_dir, template)),
    })
}

fn raw_object_shape_from_tmx(
    object: roxmltree::Node<'_, '_>,
    tile_object: bool,
) -> Result<RawObjectShape, TiledImportError> {
    if tile_object {
        return Ok(RawObjectShape::Tile);
    }
    for child in object.children().filter(|node| node.is_element()) {
        match child.tag_name().name() {
            "point" => return Ok(RawObjectShape::Point),
            "ellipse" => return Ok(RawObjectShape::Ellipse),
            "polygon" => {
                return Ok(RawObjectShape::Polygon(parse_tmx_points(
                    child.attribute("points").unwrap_or_default(),
                )?));
            }
            "polyline" => {
                return Ok(RawObjectShape::Polyline(parse_tmx_points(
                    child.attribute("points").unwrap_or_default(),
                )?));
            }
            _ => {}
        }
    }
    Ok(RawObjectShape::Rectangle)
}

fn raw_object_shape_from_json(object: &TiledJsonObject) -> RawObjectShape {
    if object.gid.is_some() {
        RawObjectShape::Tile
    } else if object.point {
        RawObjectShape::Point
    } else if object.ellipse {
        RawObjectShape::Ellipse
    } else if !object.polygon.is_empty() {
        RawObjectShape::Polygon(
            object
                .polygon
                .iter()
                .map(|point| [point.x, -point.y])
                .collect(),
        )
    } else if !object.polyline.is_empty() {
        RawObjectShape::Polyline(
            object
                .polyline
                .iter()
                .map(|point| [point.x, -point.y])
                .collect(),
        )
    } else {
        RawObjectShape::Rectangle
    }
}

fn parse_tmx_points(points: &str) -> Result<Vec<[f32; 2]>, TiledImportError> {
    points
        .split_whitespace()
        .map(|point| {
            let (x, y) = point.split_once(',').ok_or_else(|| {
                TiledImportError::MalformedMap(format!("object point `{point}` is invalid"))
            })?;
            let x = x.parse::<f32>().map_err(|_| {
                TiledImportError::MalformedMap(format!("object point `{point}` has invalid x"))
            })?;
            let y = y.parse::<f32>().map_err(|_| {
                TiledImportError::MalformedMap(format!("object point `{point}` has invalid y"))
            })?;
            Ok([x, -y])
        })
        .collect()
}

fn collect_tmx_layer_cells(
    layer: roxmltree::Node<'_, '_>,
) -> Result<Vec<RawCell>, TiledImportError> {
    let layer_name = layer.attribute("name").unwrap_or("Tile Layer");
    let data = layer
        .children()
        .find(|node| node.is_element() && node.has_tag_name("data"))
        .ok_or_else(|| TiledImportError::UnsupportedLayerData {
            layer: layer_name.to_string(),
            reason: "tile layer is missing <data>",
        })?;
    let encoding = data.attribute("encoding");
    let compression = data.attribute("compression");

    let chunks: Vec<_> = data
        .children()
        .filter(|node| node.is_element() && node.has_tag_name("chunk"))
        .collect();
    if !chunks.is_empty() {
        let mut cells = Vec::new();
        for chunk in chunks {
            let chunk_x = required_i32_attr(chunk, "x")?;
            let chunk_y = required_i32_attr(chunk, "y")?;
            let width = required_u32_attr(chunk, "width")?;
            let height = required_u32_attr(chunk, "height")?;
            let gids = decode_tmx_gids(
                chunk,
                layer_name,
                width as usize * height as usize,
                encoding,
                compression,
            )?;
            for local_y in 0..height {
                for local_x in 0..width {
                    let index = (local_y * width + local_x) as usize;
                    cells.push(RawCell {
                        x: chunk_x + local_x as i32,
                        y: chunk_y + local_y as i32,
                        gid: gids[index],
                    });
                }
            }
        }
        return Ok(cells);
    }

    let width = required_u32_attr(layer, "width")?;
    let height = required_u32_attr(layer, "height")?;
    let layer_x = optional_i32_attr(layer, "x")?.unwrap_or_default();
    let layer_y = optional_i32_attr(layer, "y")?.unwrap_or_default();
    let gids = decode_tmx_gids(
        data,
        layer_name,
        width as usize * height as usize,
        encoding,
        compression,
    )?;
    let mut cells = Vec::with_capacity(gids.len());
    for local_y in 0..height {
        for local_x in 0..width {
            let index = (local_y * width + local_x) as usize;
            cells.push(RawCell {
                x: layer_x + local_x as i32,
                y: layer_y + local_y as i32,
                gid: gids[index],
            });
        }
    }
    Ok(cells)
}

fn decode_tmx_gids(
    node: roxmltree::Node<'_, '_>,
    layer_name: &str,
    expected_len: usize,
    encoding: Option<&str>,
    compression: Option<&str>,
) -> Result<Vec<u32>, TiledImportError> {
    let gids = match encoding {
        Some("csv") => parse_csv_gids(node.text().unwrap_or_default(), layer_name)?,
        Some("base64") => {
            decode_base64_gids(node.text().unwrap_or_default(), compression, layer_name)?
        }
        Some(_) => {
            return Err(TiledImportError::UnsupportedLayerData {
                layer: layer_name.to_string(),
                reason: "only CSV and base64 TMX tile data are supported",
            });
        }
        None => {
            if compression.is_some() {
                return Err(TiledImportError::UnsupportedLayerData {
                    layer: layer_name.to_string(),
                    reason: "compressed tile data requires base64 encoding",
                });
            }
            let mut values = Vec::new();
            for tile in node
                .children()
                .filter(|node| node.is_element() && node.has_tag_name("tile"))
            {
                values.push(required_u32_attr(tile, "gid")?);
            }
            values
        }
    };

    if gids.len() != expected_len {
        return Err(TiledImportError::UnsupportedLayerData {
            layer: layer_name.to_string(),
            reason: "tile data length does not match width * height",
        });
    }
    Ok(gids)
}

fn parse_csv_gids(text: &str, layer_name: &str) -> Result<Vec<u32>, TiledImportError> {
    let mut gids = Vec::new();
    for part in text.split(|ch: char| ch == ',' || ch.is_whitespace()) {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        gids.push(part.parse::<u32>().map_err(|_| {
            TiledImportError::MalformedMap(format!(
                "layer `{layer_name}` contains an invalid CSV gid `{part}`"
            ))
        })?);
    }
    Ok(gids)
}

fn decode_base64_gids(
    text: &str,
    compression: Option<&str>,
    layer_name: &str,
) -> Result<Vec<u32>, TiledImportError> {
    let compact: String = text.chars().filter(|ch| !ch.is_whitespace()).collect();
    let bytes = base64::decode(compact).map_err(|source| TiledImportError::DecodeLayerData {
        layer: layer_name.to_string(),
        source,
    })?;
    let bytes = match compression {
        None | Some("") => bytes,
        Some("zlib") => decompress_zlib(&bytes, layer_name)?,
        Some("gzip") => decompress_gzip(&bytes, layer_name)?,
        Some(_) => {
            return Err(TiledImportError::UnsupportedLayerData {
                layer: layer_name.to_string(),
                reason: "only zlib and gzip compressed tile data are supported",
            });
        }
    };
    decode_little_endian_gids(&bytes, layer_name)
}

fn decompress_zlib(bytes: &[u8], layer_name: &str) -> Result<Vec<u8>, TiledImportError> {
    let mut decoder = flate2::read::ZlibDecoder::new(bytes);
    let mut decoded = Vec::new();
    decoder
        .read_to_end(&mut decoded)
        .map_err(|source| TiledImportError::InflateLayerData {
            layer: layer_name.to_string(),
            source,
        })?;
    Ok(decoded)
}

fn decompress_gzip(bytes: &[u8], layer_name: &str) -> Result<Vec<u8>, TiledImportError> {
    let mut decoder = flate2::read::GzDecoder::new(bytes);
    let mut decoded = Vec::new();
    decoder
        .read_to_end(&mut decoded)
        .map_err(|source| TiledImportError::InflateLayerData {
            layer: layer_name.to_string(),
            source,
        })?;
    Ok(decoded)
}

fn decode_little_endian_gids(bytes: &[u8], layer_name: &str) -> Result<Vec<u32>, TiledImportError> {
    if bytes.len() % 4 != 0 {
        return Err(TiledImportError::UnsupportedLayerData {
            layer: layer_name.to_string(),
            reason: "base64 tile data byte length must be divisible by 4",
        });
    }
    Ok(bytes
        .chunks_exact(4)
        .map(|chunk| u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect())
}

fn resolve_tilesets(
    tilesets: &[TiledJsonTilesetRef],
    base_dir: &Path,
) -> Result<Vec<TiledTileset>, TiledImportError> {
    let mut resolved = Vec::with_capacity(tilesets.len());
    for tileset in tilesets {
        let source = tileset.source.as_ref().map(PathBuf::from);
        let resolved_tileset = match &source {
            Some(source) => {
                let source_path = resolve_path(base_dir, source);
                let extension = source_path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .unwrap_or_default()
                    .to_ascii_lowercase();
                match extension.as_str() {
                    "json" | "tsj" => {
                        let text = std::fs::read_to_string(&source_path).map_err(|source| {
                            TiledImportError::Io {
                                path: source_path.clone(),
                                source,
                            }
                        })?;
                        let file: TiledJsonTilesetFile =
                            serde_json::from_str(&text).map_err(TiledImportError::Json)?;
                        let source_base = source_path.parent().unwrap_or(base_dir);
                        build_tileset(
                            tileset.firstgid,
                            file.image.as_deref(),
                            file.tilewidth,
                            file.tileheight,
                            file.columns,
                            file.tilecount,
                            file.imagewidth,
                            file.imageheight,
                            parse_transparent_color(file.transparentcolor.as_deref())?,
                            json_tile_offset(file.tileoffset.as_ref()),
                            collect_json_tile_animations(&file.tiles),
                            collect_json_properties(&file.properties, source_base)?,
                            collect_json_tile_properties(&file.tiles, source_base)?,
                            file.margin,
                            file.spacing,
                            source_base,
                            Some(source_path.clone()),
                        )?
                    }
                    "tsx" => load_tmx_tileset_file(tileset.firstgid, &source_path)?,
                    _ => {
                        return Err(TiledImportError::UnsupportedExternalTileset {
                            source: source_path,
                        });
                    }
                }
            }
            None => build_tileset(
                tileset.firstgid,
                tileset.image.as_deref(),
                tileset.tilewidth,
                tileset.tileheight,
                tileset.columns,
                tileset.tilecount,
                tileset.imagewidth,
                tileset.imageheight,
                parse_transparent_color(tileset.transparentcolor.as_deref())?,
                json_tile_offset(tileset.tileoffset.as_ref()),
                collect_json_tile_animations(&tileset.tiles),
                collect_json_properties(&tileset.properties, base_dir)?,
                collect_json_tile_properties(&tileset.tiles, base_dir)?,
                tileset.margin,
                tileset.spacing,
                base_dir,
                None,
            )?,
        };
        resolved.push(resolved_tileset);
    }
    Ok(resolved)
}

#[allow(clippy::too_many_arguments)]
fn build_tileset(
    first_gid: u32,
    image: Option<&str>,
    tilewidth: Option<u32>,
    tileheight: Option<u32>,
    columns: Option<u32>,
    tilecount: Option<u32>,
    imagewidth: Option<u32>,
    imageheight: Option<u32>,
    transparent_color: Option<[u8; 3]>,
    tile_offset: [i32; 2],
    animations: Vec<TileAnimation>,
    properties: Vec<TiledProperty>,
    tile_properties: Vec<Vec<TiledProperty>>,
    margin: u32,
    spacing: u32,
    base_dir: &Path,
    source: Option<PathBuf>,
) -> Result<TiledTileset, TiledImportError> {
    if first_gid == 0 {
        return Err(TiledImportError::UnsupportedTileset {
            source,
            reason: "firstgid must be greater than zero",
        });
    }
    let image = image.ok_or_else(|| TiledImportError::UnsupportedTileset {
        source: source.clone(),
        reason: "only single-image tilesets are supported",
    })?;
    let image_path = resolve_path(base_dir, image);
    let tile_width = tilewidth.ok_or_else(|| TiledImportError::UnsupportedTileset {
        source: source.clone(),
        reason: "tileset is missing tilewidth",
    })?;
    let tile_height = tileheight.ok_or_else(|| TiledImportError::UnsupportedTileset {
        source: source.clone(),
        reason: "tileset is missing tileheight",
    })?;
    let inferred_image_size = if (columns.is_none() || tilecount.is_none())
        && (imagewidth.is_none() || imageheight.is_none())
    {
        Some(
            image::image_dimensions(&image_path).map_err(|source| TiledImportError::Image {
                path: image_path.clone(),
                source,
            })?,
        )
    } else {
        None
    };
    let imagewidth = imagewidth.or_else(|| inferred_image_size.map(|(width, _)| width));
    let imageheight = imageheight.or_else(|| inferred_image_size.map(|(_, height)| height));
    let columns = columns.or_else(|| {
        let image_width = imagewidth?;
        tiles_in_image_span(image_width, tile_width, margin, spacing)
    });
    let columns = columns.filter(|columns| *columns > 0).ok_or_else(|| {
        TiledImportError::UnsupportedTileset {
            source: source.clone(),
            reason: "tileset is missing columns",
        }
    })?;
    let tile_count = tilecount.or_else(|| {
        let image_width = imagewidth?;
        let image_height = imageheight?;
        let columns = tiles_in_image_span(image_width, tile_width, margin, spacing)?;
        let rows = tiles_in_image_span(image_height, tile_height, margin, spacing)?;
        Some(columns.saturating_mul(rows))
    });
    let tile_count = tile_count.filter(|count| *count > 0).ok_or_else(|| {
        TiledImportError::UnsupportedTileset {
            source: source.clone(),
            reason: "tileset is missing tilecount",
        }
    })?;
    if animations.iter().any(|animation| {
        animation.tile_id.0 >= tile_count
            || animation
                .frames
                .iter()
                .any(|frame| frame.tile_id.0 >= tile_count)
    }) {
        return Err(TiledImportError::UnsupportedTileset {
            source,
            reason: "tileset animation references a tile outside tilecount",
        });
    }
    let rows = tile_count.div_ceil(columns);
    let image_size = [
        imagewidth.unwrap_or_else(|| columns.saturating_mul(tile_width)),
        imageheight.unwrap_or_else(|| rows.saturating_mul(tile_height)),
    ];

    Ok(TiledTileset {
        first_gid,
        image: image_path,
        tile_size: [tile_width, tile_height],
        columns,
        rows,
        tile_count,
        image_size,
        margin,
        spacing,
        tile_rects: Vec::new(),
        tile_offset,
        animations,
        properties,
        tile_properties,
        transparent_color,
    })
}

fn tiles_in_image_span(image_span: u32, tile_span: u32, margin: u32, spacing: u32) -> Option<u32> {
    let tile_span = tile_span.max(1);
    let available = image_span.checked_sub(margin.saturating_mul(2))?;
    if available < tile_span {
        return None;
    }
    Some(available.saturating_add(spacing) / tile_span.saturating_add(spacing).max(1))
}

fn split_layers_by_tileset(
    layers: &[ParsedLayer],
    tilesets: &[TiledTileset],
) -> Result<Vec<LayerSplit>, TiledImportError> {
    let mut split_layers = Vec::new();
    for (source_layer, layer) in layers.iter().enumerate() {
        let mut cells_by_tileset: BTreeMap<usize, Vec<RawCell>> = BTreeMap::new();
        for &cell in &layer.cells {
            let gid = cell.gid & GID_MASK;
            if gid == 0 {
                continue;
            }
            let tileset_index = tileset_index_for_gid(gid, tilesets)?;
            cells_by_tileset
                .entry(tileset_index)
                .or_default()
                .push(cell);
        }
        for (split_order, (tileset_index, cells)) in cells_by_tileset.into_iter().enumerate() {
            split_layers.push(LayerSplit {
                source_layer,
                tileset_index,
                split_order,
                cells,
            });
        }
    }
    Ok(split_layers)
}

fn tileset_index_for_gid(gid: u32, tilesets: &[TiledTileset]) -> Result<usize, TiledImportError> {
    tilesets
        .iter()
        .position(|tileset| gid_in_tileset(gid, tileset))
        .ok_or(TiledImportError::TileGidOutOfRange { gid })
}

fn tiled_depth_sort_for_layers(
    orientation: TilemapOrientation,
    map_tile_size: [u32; 2],
    tilesets: &[TiledTileset],
    layer_count: usize,
) -> TilemapDepthSort {
    if orientation == TilemapOrientation::Orthogonal
        && layer_count > 1
        && tilesets.iter().any(|tileset| {
            tileset.tile_size[0] > map_tile_size[0]
                || tileset.tile_size[1] > map_tile_size[1]
                || tileset.tile_offset != [0, 0]
        })
    {
        TilemapDepthSort::YThenLayer
    } else {
        TilemapDepthSort::Layer
    }
}

fn tilemap_bounds(
    infinite: bool,
    width: u32,
    height: u32,
    layers: &[ParsedLayer],
) -> ((i32, i32), u32, u32) {
    if !infinite {
        return ((0, 0), width.max(1), height.max(1));
    }

    let mut min_x = i32::MAX;
    let mut min_y = i32::MAX;
    let mut max_x = i32::MIN;
    let mut max_y = i32::MIN;
    for layer in layers {
        for cell in &layer.cells {
            min_x = min_x.min(cell.x);
            min_y = min_y.min(cell.y);
            max_x = max_x.max(cell.x);
            max_y = max_y.max(cell.y);
        }
    }
    if min_x == i32::MAX {
        return ((0, 0), 1, 1);
    }
    let width = (max_x - min_x + 1).max(1) as u32;
    let height = (max_y - min_y + 1).max(1) as u32;
    ((min_x, min_y), width, height)
}

fn decode_cell(
    cell: RawCell,
    tileset: &TiledTileset,
    orientation: TilemapOrientation,
    origin: (i32, i32),
    tilemap_width: u32,
    tilemap_height: u32,
) -> Result<Option<(Tile, u32, u32)>, TiledImportError> {
    let gid = cell.gid & GID_MASK;
    if gid == 0 {
        return Ok(None);
    }
    if !gid_in_tileset(gid, tileset) {
        return Err(TiledImportError::TileGidOutOfRange { gid });
    }

    let flags = tile_flags_from_gid(cell.gid);

    let x = (cell.x - origin.0) as u32;
    let tiled_y = (cell.y - origin.1) as u32;
    let (x, y) = match orientation {
        TilemapOrientation::Isometric => (
            tilemap_height.saturating_sub(1).saturating_sub(tiled_y),
            tilemap_width.saturating_sub(1).saturating_sub(x),
        ),
        _ => (x, tilemap_height.saturating_sub(1).saturating_sub(tiled_y)),
    };
    let tile = Tile::new(TileId(gid - tileset.first_gid)).with_flags(flags);
    Ok(Some((tile, x, y)))
}

fn decode_object(
    object: RawObject,
    tilesets: &[TiledTileset],
    map_pixel_height: f32,
) -> Result<Option<TiledObject>, TiledImportError> {
    let tile_payload = match object.gid {
        Some(gid_with_flags) => {
            let gid = gid_with_flags & GID_MASK;
            if gid == 0 {
                return Ok(None);
            }
            let tileset_index = tileset_index_for_gid(gid, tilesets)?;
            let tileset = &tilesets[tileset_index];
            let tile_id = TileId(gid - tileset.first_gid);
            Some((tileset_index, tile_id, tile_flags_from_gid(gid_with_flags)))
        }
        None => None,
    };
    let fallback_size = tile_payload
        .map(|(tileset_index, tile_id, _)| tilesets[tileset_index].tile_draw_size(tile_id))
        .unwrap_or([0, 0]);
    let width = if object.width > f32::EPSILON {
        object.width
    } else {
        fallback_size[0] as f32
    };
    let height = if object.height > f32::EPSILON {
        object.height
    } else {
        fallback_size[1] as f32
    };
    let position = [object.x, map_pixel_height - object.y];
    let size = [width.max(0.0), height.max(0.0)];
    let shape = match (object.shape, tile_payload) {
        (_, Some((tileset_index, tile_id, flags))) => TiledObjectShape::Tile {
            tileset_index,
            tile_id,
            flags,
        },
        (RawObjectShape::Rectangle, None) => TiledObjectShape::Rectangle,
        (RawObjectShape::Point, None) => TiledObjectShape::Point,
        (RawObjectShape::Ellipse, None) => TiledObjectShape::Ellipse,
        (RawObjectShape::Polygon(points), None) => TiledObjectShape::Polygon(points),
        (RawObjectShape::Polyline(points), None) => TiledObjectShape::Polyline(points),
        (RawObjectShape::Tile, None) => TiledObjectShape::Rectangle,
    };

    Ok(Some(TiledObject {
        id: object.id,
        name: object.name,
        class: object.class,
        position,
        size,
        shape,
        properties: object.properties,
        template: object.template,
    }))
}

fn tile_flags_from_gid(gid: u32) -> TileFlags {
    let mut flags = TileFlags::empty();
    if gid & FLIPPED_HORIZONTALLY_FLAG != 0 {
        flags |= TileFlags::FLIP_X;
    }
    if gid & FLIPPED_VERTICALLY_FLAG != 0 {
        flags |= TileFlags::FLIP_Y;
    }
    if gid & FLIPPED_DIAGONALLY_FLAG != 0 {
        flags |= TileFlags::FLIP_DIAGONAL;
    }
    flags
}

#[inline]
fn gid_in_tileset(gid: u32, tileset: &TiledTileset) -> bool {
    gid >= tileset.first_gid && gid < tileset.first_gid.saturating_add(tileset.tile_count)
}

fn resolve_path(base_dir: &Path, path: impl AsRef<Path>) -> PathBuf {
    let path = path.as_ref();
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base_dir.join(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_json(data: &str) -> String {
        format!(
            r#"{{
                "orientation": "orthogonal",
                "width": 3,
                "height": 2,
                "tilewidth": 16,
                "tileheight": 16,
                "layers": [
                    {{
                        "name": "Ground",
                        "type": "tilelayer",
                        "width": 3,
                        "height": 2,
                        "data": {data}
                    }},
                    {{
                        "name": "Foreground",
                        "type": "tilelayer",
                        "width": 3,
                        "height": 2,
                        "opacity": 0.5,
                        "data": [0, 0, 0, 0, 3, 0]
                    }}
                ],
                "tilesets": [
                    {{
                        "firstgid": 1,
                        "image": "tiles.png",
                        "tilewidth": 16,
                        "tileheight": 16,
                        "columns": 2,
                        "tilecount": 4
                    }}
                ]
            }}"#
        )
    }

    #[test]
    fn imports_embedded_tiled_json_tile_layers() {
        let import =
            TiledImport::from_json_str(&sample_json("[1, 2, 0, 0, 0, 0]"), Path::new("assets"))
                .expect("Tiled JSON should import");

        assert_eq!(import.orientation, TilemapOrientation::Orthogonal);
        assert_eq!(import.render_order, TilemapRenderOrder::RightDown);
        assert_eq!(import.tile_size, [16, 16]);
        assert_eq!(
            import.tileset.image,
            PathBuf::from("assets").join("tiles.png")
        );
        assert_eq!(import.tileset.columns, 2);
        assert_eq!(import.layers.len(), 2);
        assert_eq!(import.layers[0].name, "Ground");
        assert_eq!(import.layers[0].sorting_layer, 0);
        assert_eq!(import.layers[1].sorting_layer, 256);
        assert_eq!(import.layers[1].opacity, 0.5);
        assert_eq!(import.map.tile(0, 0, 1).unwrap().id, TileId(0));
        assert_eq!(import.map.tile(0, 1, 1).unwrap().id, TileId(1));
        assert_eq!(import.map.tile(1, 1, 0).unwrap().id, TileId(2));
    }

    #[test]
    fn imports_tiled_flip_flags() {
        let flipped = FLIPPED_HORIZONTALLY_FLAG | FLIPPED_VERTICALLY_FLAG | 2;
        let import = TiledImport::from_json_str(
            &sample_json(&format!("[{flipped}, 0, 0, 0, 0, 0]")),
            Path::new("."),
        )
        .expect("Tiled JSON should import");

        let tile = import.map.tile(0, 0, 1).unwrap();
        assert_eq!(tile.id, TileId(1));
        assert!(tile.flags.contains(TileFlags::FLIP_X));
        assert!(tile.flags.contains(TileFlags::FLIP_Y));
    }

    #[test]
    fn imports_diagonal_tiled_flip() {
        let diagonal = FLIPPED_DIAGONALLY_FLAG | 1;
        let import = TiledImport::from_json_str(
            &sample_json(&format!("[{diagonal}, 0, 0, 0, 0, 0]")),
            Path::new("."),
        )
        .expect("diagonal flags should import");

        let tile = import.map.tile(0, 0, 1).unwrap();
        assert_eq!(tile.id, TileId(0));
        assert!(tile.flags.contains(TileFlags::FLIP_DIAGONAL));
    }

    #[test]
    fn imports_multiple_used_tilesets_as_split_layers() {
        let json = r#"{
            "orientation": "orthogonal",
            "width": 2,
            "height": 1,
            "tilewidth": 16,
            "tileheight": 16,
            "layers": [{
                "name": "Ground",
                "type": "tilelayer",
                "width": 2,
                "height": 1,
                "data": [1, 10]
            }],
            "tilesets": [
                {
                    "firstgid": 1,
                    "image": "a.png",
                    "tilewidth": 16,
                    "tileheight": 16,
                    "columns": 1,
                    "tilecount": 1
                },
                {
                    "firstgid": 10,
                    "image": "b.png",
                    "tilewidth": 16,
                    "tileheight": 16,
                    "columns": 1,
                    "tilecount": 1
                }
            ]
        }"#;
        let import =
            TiledImport::from_json_str(json, Path::new(".")).expect("multiple tilesets import");

        assert_eq!(import.tilesets.len(), 2);
        assert_eq!(import.layers.len(), 2);
        assert_eq!(import.layers[0].source_layer, 0);
        assert_eq!(import.layers[0].tileset_index, 0);
        assert_eq!(import.layers[0].storage_layer, 0);
        assert_eq!(import.layers[1].source_layer, 0);
        assert_eq!(import.layers[1].tileset_index, 1);
        assert_eq!(import.layers[1].storage_layer, 1);
        assert_eq!(import.map.tile(0, 0, 0).unwrap().id, TileId(0));
        assert_eq!(import.map.tile(1, 1, 0).unwrap().id, TileId(0));

        let texture_a = Handle::<TextureAsset>::new(crate::asset::AssetId::new());
        let texture_b = Handle::<TextureAsset>::new(crate::asset::AssetId::new());
        let renderer_a = import
            .renderer_for_layer(TilemapHandle::new(0, 0), &[texture_a, texture_b], 0)
            .expect("first split layer should render");
        let renderer_b = import
            .renderer_for_layer(TilemapHandle::new(0, 0), &[texture_a, texture_b], 1)
            .expect("second split layer should render");
        assert_eq!(renderer_a.tileset.texture, texture_a);
        assert_eq!(renderer_b.tileset.texture, texture_b);
    }

    #[test]
    fn imports_official_object_shapes_and_properties() {
        let import =
            TiledImport::from_file("examples/assets/tiled/tiled/examples/orthogonal-outside.tmx")
                .expect("official orthogonal map should import");

        assert!(import.properties.iter().any(|property| {
            property.name == "enemyTint"
                && matches!(property.value, TiledPropertyValue::Color(color)
                    if (color.r - 0.6392157).abs() < 0.001
                        && (color.a - 1.0).abs() < 0.001)
        }));

        let objects = &import
            .object_layers
            .iter()
            .find(|layer| layer.name == "Objects")
            .expect("Objects layer should import")
            .objects;
        assert!(objects.iter().any(|object| {
            object.name == "maggots"
                && object.class == "Location"
                && matches!(object.shape, TiledObjectShape::Rectangle)
                && object.properties.iter().any(|property| {
                    property.name == "spawncount"
                        && matches!(property.value, TiledPropertyValue::Int(5))
                })
        }));
        assert!(objects.iter().any(|object| {
            object.name == "discover chest"
                && object.class == "Trigger"
                && matches!(object.shape, TiledObjectShape::Ellipse)
                && object.properties.iter().any(|property| {
                    property.name == "script"
                        && matches!(property.value, TiledPropertyValue::File(ref path)
                            if path.ends_with("chest-discovered.lua"))
                })
        }));
        assert!(objects.iter().any(|object| {
            object.name == "unreachable"
                && object.class == "Fixture"
                && matches!(object.shape, TiledObjectShape::Polygon(ref points) if points.len() > 4)
        }));
        assert!(objects.iter().any(|object| {
            object.name == "guard"
                && object.class == "NPC"
                && matches!(object.shape, TiledObjectShape::Polyline(ref points) if points.len() > 2)
        }));
        assert!(objects.iter().any(|object| {
            object.name == "player-start" && matches!(object.shape, TiledObjectShape::Point)
        }));
        assert!(objects.iter().any(|object| {
            object.class == "Sign"
                && matches!(
                    object.shape,
                    TiledObjectShape::Tile {
                        tileset_index: 0,
                        ..
                    }
                )
        }));
    }

    #[test]
    fn imports_json_object_layers_and_properties() {
        let json = r##"{
            "orientation": "orthogonal",
            "width": 4,
            "height": 4,
            "tilewidth": 16,
            "tileheight": 16,
            "layers": [{
                "name": "Objects",
                "type": "objectgroup",
                "objects": [
                    {
                        "id": 1,
                        "name": "spawn",
                        "class": "Location",
                        "x": 16,
                        "y": 16,
                        "point": true,
                        "properties": [
                            { "name": "enabled", "type": "bool", "value": true },
                            { "name": "weight", "type": "float", "value": 1.5 }
                        ]
                    },
                    {
                        "id": 2,
                        "type": "Trigger",
                        "x": 32,
                        "y": 32,
                        "width": 16,
                        "height": 16,
                        "ellipse": true,
                        "properties": [
                            { "name": "target", "type": "object", "value": 1 },
                            { "name": "tint", "type": "color", "value": "#80ff0000" }
                        ]
                    }
                ]
            }],
            "tilesets": [{
                "firstgid": 1,
                "image": "tiles.png",
                "tilewidth": 16,
                "tileheight": 16,
                "columns": 1,
                "tilecount": 1
            }]
        }"##;
        let import =
            TiledImport::from_json_str(json, Path::new("assets")).expect("JSON objects import");

        let objects = &import.object_layers[0].objects;
        assert_eq!(objects[0].class, "Location");
        assert!(matches!(objects[0].shape, TiledObjectShape::Point));
        assert!(objects[0].properties.iter().any(|property| {
            property.name == "enabled" && matches!(property.value, TiledPropertyValue::Bool(true))
        }));
        assert_eq!(objects[1].class, "Trigger");
        assert!(matches!(objects[1].shape, TiledObjectShape::Ellipse));
        assert!(objects[1].properties.iter().any(|property| {
            property.name == "target" && matches!(property.value, TiledPropertyValue::Object(1))
        }));
    }

    #[test]
    fn imports_external_tsj_tileset() {
        let temp = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            temp.path().join("terrain.tsj"),
            r#"{
                "image": "terrain.png",
                "tilewidth": 8,
                "tileheight": 8,
                "columns": 1,
                "tilecount": 1
            }"#,
        )
        .expect("write tsj");
        let json = r#"{
            "orientation": "orthogonal",
            "width": 1,
            "height": 1,
            "tilewidth": 8,
            "tileheight": 8,
            "layers": [{
                "name": "Ground",
                "type": "tilelayer",
                "width": 1,
                "height": 1,
                "data": [1]
            }],
            "tilesets": [{ "firstgid": 1, "source": "terrain.tsj" }]
        }"#;

        let import =
            TiledImport::from_json_str(json, temp.path()).expect("external TSJ should import");

        assert_eq!(import.tileset.image, temp.path().join("terrain.png"));
        assert_eq!(import.map.tile(0, 0, 0).unwrap().id, TileId(0));
    }

    #[test]
    fn imports_external_tsx_tile_animations() {
        let temp = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            temp.path().join("water.tsx"),
            r#"<?xml version="1.0" encoding="UTF-8"?>
            <tileset version="1.8" tiledversion="1.8.2" name="water" tilewidth="8" tileheight="8" tilecount="4" columns="4">
                <image source="water.png" width="32" height="8"/>
                <tile id="1">
                    <animation>
                        <frame tileid="1" duration="100"/>
                        <frame tileid="2" duration="150"/>
                    </animation>
                </tile>
            </tileset>"#,
        )
        .expect("write tsx");
        let tmx = r#"<?xml version="1.0" encoding="UTF-8"?>
        <map version="1.8" tiledversion="1.8.2" orientation="orthogonal" width="1" height="1" tilewidth="8" tileheight="8">
            <tileset firstgid="1" source="water.tsx"/>
            <layer name="Ground" width="1" height="1">
                <data><tile gid="2"/></data>
            </layer>
        </map>"#;

        let import =
            TiledImport::from_tmx_str(tmx, temp.path()).expect("external TSX should import");

        assert_eq!(import.tileset.animations.len(), 1);
        assert_eq!(import.tileset.animations[0].tile_id, TileId(1));
        assert_eq!(import.tileset.animations[0].frame_at(0.12), TileId(2));
        assert_eq!(import.map.tile(0, 0, 0).unwrap().id, TileId(1));
    }

    #[test]
    fn imports_tsx_image_dimensions_and_tile_offset() {
        let temp = tempfile::tempdir().expect("tempdir");
        image::RgbaImage::new(128, 64)
            .save(temp.path().join("walls.png"))
            .expect("write png");
        std::fs::write(
            temp.path().join("walls.tsx"),
            r#"<?xml version="1.0" encoding="UTF-8"?>
            <tileset name="walls" tilewidth="64" tileheight="64">
                <tileoffset x="-32" y="4"/>
                <image source="walls.png"/>
            </tileset>"#,
        )
        .expect("write tsx");
        let tmx = r#"<?xml version="1.0" encoding="UTF-8"?>
        <map version="1.0" orientation="orthogonal" width="1" height="1" tilewidth="31" tileheight="31">
            <tileset firstgid="1" source="walls.tsx"/>
            <layer name="Walls" width="1" height="1">
                <data><tile gid="1"/></data>
            </layer>
        </map>"#;

        let import =
            TiledImport::from_tmx_str(tmx, temp.path()).expect("external TSX should import");
        let texture = Handle::<TextureAsset>::new(crate::asset::AssetId::new());
        let renderer = import
            .renderer_for_layer(TilemapHandle::new(0, 0), &[texture], 0)
            .expect("layer renderer");

        assert_eq!(import.tile_size, [31, 31]);
        assert_eq!(import.tileset.tile_size, [64, 64]);
        assert_eq!(import.tileset.image_size, [128, 64]);
        assert_eq!(import.tileset.columns, 2);
        assert_eq!(import.tileset.tile_count, 2);
        assert_eq!(import.tileset.tile_offset, [-32, 4]);
        assert_eq!(import.tileset.margin, 0);
        assert_eq!(import.tileset.spacing, 0);
        assert_eq!(renderer.tile_size, [31.0, 31.0]);
        assert_eq!(renderer.tile_draw_size, [64.0, 64.0]);
        assert_eq!(renderer.tile_offset, [-32.0, -4.0]);
    }

    #[test]
    fn imports_tmx_tileset_margin_and_spacing() {
        let import = TiledImport::from_tmx_file("examples/assets/tiled/tiled/examples/desert.tmx")
            .expect("Tiled spacing/margin sample should import");
        let texture = Handle::<TextureAsset>::new(crate::asset::AssetId::new());
        let grid = import.tileset_grid(texture);

        assert_eq!(import.tileset.margin, 1);
        assert_eq!(import.tileset.spacing, 1);
        assert_eq!(import.tileset.columns, 8);
        assert_eq!(import.tileset.rows, 6);
        assert_eq!(import.tileset.tile_count, 48);
        assert_eq!(grid.texture_size, [265, 199]);
        for (actual, expected) in grid.uv_rect(TileId(9)).unwrap().into_iter().zip([
            34.0 / 265.0,
            34.0 / 199.0,
            66.0 / 265.0,
            66.0 / 199.0,
        ]) {
            assert!((actual - expected).abs() <= 1e-6);
        }
    }

    #[test]
    fn imports_official_isometric_tmx_layer_with_tiled_direction() {
        let import = TiledImport::from_tmx_file(
            "examples/assets/tiled/tiled/examples/isometric_grass_and_water.tmx",
        )
        .expect("official Tiled isometric TMX sample should import");

        assert_eq!(import.orientation, TilemapOrientation::Isometric);
        assert_eq!(import.render_order, TilemapRenderOrder::RightDown);
        assert_eq!(import.tile_size, [64, 32]);
        assert_eq!(import.map.width(), 25);
        assert_eq!(import.map.height(), 25);
        assert_eq!(import.layers.len(), 1);
        assert_eq!(import.tileset.tile_size, [64, 64]);
        assert_eq!(import.tileset.tile_offset, [0, 16]);

        let texture = Handle::<TextureAsset>::new(crate::asset::AssetId::new());
        let renderer = import
            .renderer_for_layer(TilemapHandle::new(0, 0), &[texture], 0)
            .expect("layer renderer");
        assert_eq!(renderer.tile_offset, [0.0, -16.0]);

        assert_eq!(import.map.tile(0, 24, 24).unwrap().id, TileId(23));
        assert_eq!(import.map.tile(0, 24, 0).unwrap().id, TileId(0));
        assert_eq!(import.map.tile(0, 0, 24).unwrap().id, TileId(0));
        assert_eq!(import.map.tile(0, 0, 0).unwrap().id, TileId(0));
    }

    #[test]
    fn imports_official_forest_image_collection_and_tile_objects() {
        let import =
            TiledImport::from_tmx_file("examples/assets/tiled/tiled/examples/forest/forest.tmx")
                .expect("official Tiled forest TMX sample should import");

        assert_eq!(import.orientation, TilemapOrientation::Orthogonal);
        assert_eq!(import.tile_size, [16, 16]);
        assert_eq!(import.parallax_origin, [320.0, 128.0]);
        assert_eq!(import.tileset.tile_size, [160, 208]);
        assert_eq!(import.tileset.tile_count, 14);
        assert_eq!(
            import.tileset.tile_rects[0],
            Some(TilesetTileRect::new(1, 1, 16, 16))
        );
        assert_eq!(
            import.tileset.tile_rects[13],
            Some(TilesetTileRect::new(116, 824, 25, 25))
        );
        assert_eq!(import.layers.len(), 1);
        assert_eq!(import.layers[0].parallax, [1.0, 1.0]);
        assert_eq!(import.object_layers.len(), 4);
        assert_eq!(import.object_layers[0].objects.len(), 4);
        assert_eq!(import.object_layers[0].parallax, [0.12, 0.12]);
        assert_eq!(import.object_layers[1].parallax, [0.25, 0.25]);
        assert_eq!(import.object_layers[2].parallax, [0.5, 0.5]);
        assert_eq!(import.object_layers[3].parallax, [1.0, 1.0]);
        assert_eq!(
            import.object_layers[3].objects[0].tile_id(),
            Some(TileId(13))
        );

        let texture = Handle::<TextureAsset>::new(crate::asset::AssetId::new());
        let grid = import.tileset_grid(texture);
        let uv = grid.uv_rect(TileId(13)).expect("animated tile source uv");
        assert_eq!(grid.tile_draw_size(TileId(0)), Some([16, 16]));
        assert_eq!(grid.tile_draw_size(TileId(13)), Some([25, 25]));
        for (actual, expected) in uv.into_iter().zip([
            116.0 / 1024.0,
            824.0 / 1024.0,
            141.0 / 1024.0,
            849.0 / 1024.0,
        ]) {
            assert!((actual - expected).abs() <= 1e-6);
        }
    }

    #[test]
    fn imports_official_staggered_tmx_layer() {
        let import = TiledImport::from_tmx_file(
            "examples/assets/tiled/tiled/examples/isometric_staggered_grass_and_water.tmx",
        )
        .expect("official Tiled staggered TMX sample should import");

        assert_eq!(import.orientation, TilemapOrientation::Staggered);
        assert_eq!(import.stagger_axis, TilemapStaggerAxis::Y);
        assert_eq!(import.stagger_index, TilemapStaggerIndex::Odd);
        assert_eq!(import.render_order, TilemapRenderOrder::RightDown);
        assert_eq!(import.tile_size, [64, 32]);
        assert_eq!(import.map.width(), 32);
        assert_eq!(import.map.height(), 64);
        assert_eq!(import.layers.len(), 1);
        assert_eq!(import.tileset.tile_size, [64, 64]);
        assert_eq!(import.tileset.tile_offset, [0, 16]);

        let texture = Handle::<TextureAsset>::new(crate::asset::AssetId::new());
        let renderer = import
            .renderer_for_layer(TilemapHandle::new(0, 0), &[texture], 0)
            .expect("layer renderer");
        assert_eq!(renderer.stagger_axis, TilemapStaggerAxis::Y);
        assert_eq!(renderer.stagger_index, TilemapStaggerIndex::Even);
        assert_eq!(renderer.tile_offset, [0.0, -16.0]);
        assert_eq!(renderer.cell_to_local_origin([0, 0]), [32.0, 0.0]);
        assert_eq!(renderer.cell_to_local_origin([0, 63]), [0.0, 1008.0]);

        let mut non_empty = 0usize;
        for y in 0..import.map.height() {
            for x in 0..import.map.width() {
                if !import.map.tile(0, x, y).unwrap().is_empty() {
                    non_empty += 1;
                }
            }
        }
        assert!(non_empty > 0);
    }

    #[test]
    fn imports_official_hexagonal_tmx_layer() {
        let import =
            TiledImport::from_tmx_file("examples/assets/tiled/tiled/examples/hexagonal-mini.tmx")
                .expect("official Tiled hexagonal TMX sample should import");

        assert_eq!(import.orientation, TilemapOrientation::Hexagonal);
        assert_eq!(import.stagger_axis, TilemapStaggerAxis::Y);
        assert_eq!(import.stagger_index, TilemapStaggerIndex::Odd);
        assert_eq!(import.hex_side_length, 6);
        assert_eq!(import.tile_size, [14, 12]);
        assert_eq!(import.map.width(), 20);
        assert_eq!(import.map.height(), 20);
        assert_eq!(import.layers.len(), 1);
        assert_eq!(import.tileset.tile_size, [18, 18]);
        assert_eq!(import.tileset.tile_offset, [0, 1]);
        assert_eq!(import.tileset.columns, 5);
        assert_eq!(import.tileset.rows, 4);

        let texture = Handle::<TextureAsset>::new(crate::asset::AssetId::new());
        let renderer = import
            .renderer_for_layer(TilemapHandle::new(0, 0), &[texture], 0)
            .expect("layer renderer");
        assert_eq!(renderer.hex_side_length, 6.0);
        assert_eq!(renderer.stagger_index, TilemapStaggerIndex::Even);
        assert_eq!(renderer.cell_to_local_origin([0, 0]), [7.0, 0.0]);
        assert_eq!(renderer.cell_to_local_origin([0, 1]), [0.0, 9.0]);

        let mut non_empty = 0usize;
        for y in 0..import.map.height() {
            for x in 0..import.map.width() {
                if !import.map.tile(0, x, y).unwrap().is_empty() {
                    non_empty += 1;
                }
            }
        }
        assert!(non_empty > 0);
    }

    #[test]
    fn tmx_overhanging_multilayer_tiles_enable_y_then_layer_sort() {
        let temp = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            temp.path().join("walls.tsx"),
            r#"<?xml version="1.0" encoding="UTF-8"?>
            <tileset name="walls" tilewidth="64" tileheight="64" columns="1" tilecount="1">
                <tileoffset x="-32" y="0"/>
                <image source="walls.png" width="64" height="64"/>
            </tileset>"#,
        )
        .expect("write tsx");
        let tmx = r#"<?xml version="1.0" encoding="UTF-8"?>
        <map version="1.0" orientation="orthogonal" renderorder="right-down" width="2" height="2" tilewidth="31" tileheight="31">
            <tileset firstgid="1" source="walls.tsx"/>
            <layer name="Walls" width="2" height="2">
                <data><tile gid="1"/><tile gid="0"/><tile gid="0"/><tile gid="0"/></data>
            </layer>
            <layer name="Walls level 2" width="2" height="2">
                <data><tile gid="0"/><tile gid="0"/><tile gid="0"/><tile gid="1"/></data>
            </layer>
        </map>"#;

        let import =
            TiledImport::from_tmx_str(tmx, temp.path()).expect("overhanging layers should import");
        let texture = Handle::<TextureAsset>::new(crate::asset::AssetId::new());
        let renderer = import
            .renderer_for_layer(TilemapHandle::new(0, 0), &[texture], 0)
            .expect("layer renderer");

        assert_eq!(import.depth_sort, TilemapDepthSort::YThenLayer);
        assert_eq!(import.layers[0].sorting_layer, 0);
        assert_eq!(import.layers[1].sorting_layer, 0);
        assert_eq!(renderer.depth_sort, TilemapDepthSort::YThenLayer);
    }

    #[test]
    fn imports_official_tmx_base64_zlib_layers() {
        let import = TiledImport::from_tmx_file("examples/assets/tiled/sewers.tmx")
            .expect("official Tiled TMX sample should import");

        assert_eq!(import.orientation, TilemapOrientation::Orthogonal);
        assert_eq!(import.render_order, TilemapRenderOrder::RightDown);
        assert_eq!(import.tile_size, [24, 24]);
        assert_eq!(import.map.width(), 50);
        assert_eq!(import.map.height(), 50);
        assert_eq!(import.layers.len(), 2);
        assert_eq!(import.layers[0].name, "Bottom");
        assert_eq!(import.layers[1].name, "Top");
        assert_eq!(import.layers[1].opacity, 0.49);
        assert_eq!(import.tileset.image_size, [192, 217]);
        assert_eq!(import.tileset.transparent_color, Some([255, 0, 255]));

        let mut non_empty = 0usize;
        for layer in 0..import.map.layer_count() {
            for y in 0..import.map.height() {
                for x in 0..import.map.width() {
                    if !import.map.tile(layer, x, y).unwrap().is_empty() {
                        non_empty += 1;
                    }
                }
            }
        }
        assert!(non_empty > 0);
    }
}
