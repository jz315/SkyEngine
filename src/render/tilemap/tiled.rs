use std::path::Path;
#[cfg(test)]
use std::path::PathBuf;

use crate::asset::{Handle, TextureAsset, TextureColorSpace};
use crate::render::component::{
    TilemapDepthSort, TilemapOrientation, TilemapRenderOrder, TilemapRenderer, TilemapStaggerAxis,
    TilemapStaggerIndex, TilesetGrid, TilesetTileRect,
};
use crate::render::Color;

#[cfg(test)]
use super::TileFlags;
#[cfg(test)]
use super::TileId;
use super::{Tilemap, TilemapDescriptor, TilemapHandle};

#[path = "tiled/data.rs"]
mod data;
#[path = "tiled/error.rs"]
mod error;
#[path = "tiled/json.rs"]
mod json;
#[path = "tiled/layer.rs"]
mod layer;
#[path = "tiled/object.rs"]
mod object;
#[path = "tiled/properties.rs"]
mod properties;
#[path = "tiled/tileset.rs"]
mod tileset;
#[path = "tiled/tmx.rs"]
mod tmx;
#[path = "tiled/types.rs"]
mod types;
#[path = "tiled/util.rs"]
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
    if !tileset.tile_images.is_empty() {
        return load_tiled_image_collection_atlas(tileset);
    }

    let image = load_tiled_rgba_image(&tileset.image, tileset.transparent_color)?;
    let (width, height) = image.dimensions();
    Ok(TextureAsset::new(
        width,
        height,
        TextureColorSpace::Srgb,
        image.into_raw(),
    ))
}

fn load_tiled_rgba_image(
    path: &Path,
    transparent_color: Option<[u8; 3]>,
) -> Result<image::RgbaImage, TiledImportError> {
    let image = image::open(path).map_err(|source| TiledImportError::Image {
        path: path.to_path_buf(),
        source,
    })?;
    let mut image = image.to_rgba8();
    if let Some([r, g, b]) = transparent_color {
        for pixel in image.as_flat_samples_mut().samples.chunks_exact_mut(4) {
            if pixel[0] == r && pixel[1] == g && pixel[2] == b {
                pixel[3] = 0;
            }
        }
    }
    Ok(image)
}

fn load_tiled_image_collection_atlas(
    tileset: &TiledTileset,
) -> Result<TextureAsset, TiledImportError> {
    let [atlas_width, atlas_height] = [tileset.image_size[0].max(1), tileset.image_size[1].max(1)];
    let mut atlas_pixels = vec![0u8; (atlas_width * atlas_height * 4) as usize];
    for (tile_id, source) in tileset.tile_images.iter().enumerate() {
        let Some(source) = source else {
            continue;
        };
        let atlas_rect = tileset
            .tile_rects
            .get(tile_id)
            .and_then(|rect| *rect)
            .ok_or_else(|| TiledImportError::UnsupportedTileset {
                source: Some(source.image.clone()),
                reason: "image collection atlas tile is missing a packed rectangle",
            })?;
        let image = load_tiled_rgba_image(&source.image, tileset.transparent_color)?;
        blit_tiled_image_rect(
            &image,
            source.source_rect,
            &mut atlas_pixels,
            [atlas_width, atlas_height],
            atlas_rect,
            &source.image,
        )?;
    }

    Ok(TextureAsset::new(
        atlas_width,
        atlas_height,
        TextureColorSpace::Srgb,
        atlas_pixels,
    ))
}

fn blit_tiled_image_rect(
    image: &image::RgbaImage,
    source_rect: TilesetTileRect,
    atlas_pixels: &mut [u8],
    atlas_size: [u32; 2],
    atlas_rect: TilesetTileRect,
    image_path: &Path,
) -> Result<(), TiledImportError> {
    if source_rect.x.saturating_add(source_rect.width) > image.width()
        || source_rect.y.saturating_add(source_rect.height) > image.height()
    {
        return Err(TiledImportError::UnsupportedTileset {
            source: Some(image_path.to_path_buf()),
            reason: "image collection tile source rectangle is outside the source image",
        });
    }
    if atlas_rect.x.saturating_add(atlas_rect.width) > atlas_size[0]
        || atlas_rect.y.saturating_add(atlas_rect.height) > atlas_size[1]
        || atlas_rect.width != source_rect.width
        || atlas_rect.height != source_rect.height
    {
        return Err(TiledImportError::UnsupportedTileset {
            source: Some(image_path.to_path_buf()),
            reason: "image collection atlas rectangle is invalid",
        });
    }

    let source_row_stride = image.width() as usize * 4;
    let atlas_row_stride = atlas_size[0] as usize * 4;
    let pixels = image.as_raw();
    let copy_len = source_rect.width as usize * 4;
    for row in 0..source_rect.height {
        let source_start =
            ((source_rect.y + row) as usize * source_row_stride) + source_rect.x as usize * 4;
        let source_end = source_start + copy_len;
        let atlas_start =
            ((atlas_rect.y + row) as usize * atlas_row_stride) + atlas_rect.x as usize * 4;
        let atlas_end = atlas_start + copy_len;
        atlas_pixels[atlas_start..atlas_end].copy_from_slice(&pixels[source_start..source_end]);
    }
    Ok(())
}

impl TiledImport {
    fn from_raw(raw: TiledJsonMap, base_dir: &Path) -> Result<Self, TiledImportError> {
        let orientation = parse_orientation(&raw.orientation)?;
        let stagger_axis = parse_stagger_axis(&raw.staggeraxis)?;
        let stagger_index = parse_stagger_index(&raw.staggerindex)?;
        let render_order = parse_render_order(&raw.renderorder)?;
        let parsed_tilesets = resolve_json_tilesets(&raw.tilesets, base_dir)?;

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
                    .map(|object| collect_json_object(object, base_dir))
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
            let data = decode_json_gids(
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
    let data = decode_json_gids(
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

    fn write_solid_png(path: &Path, width: u32, height: u32, color: [u8; 4]) {
        let mut image = image::RgbaImage::new(width, height);
        for pixel in image.pixels_mut() {
            *pixel = image::Rgba(color);
        }
        image.save(path).expect("write fixture png");
    }

    fn write_transparent_png_with_rect(
        path: &Path,
        width: u32,
        height: u32,
        rect: TilesetTileRect,
        color: [u8; 4],
    ) {
        let mut image = image::RgbaImage::new(width, height);
        for y in rect.y..rect.y + rect.height {
            for x in rect.x..rect.x + rect.width {
                image.put_pixel(x, y, image::Rgba(color));
            }
        }
        image.save(path).expect("write fixture png");
    }

    fn texture_pixel(texture: &TextureAsset, x: u32, y: u32) -> [u8; 4] {
        let index = ((y * texture.width() + x) * 4) as usize;
        texture.pixels()[index..index + 4]
            .try_into()
            .expect("pixel slice has four channels")
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
    fn packs_tmx_image_collection_tileset_with_multiple_source_images() {
        let temp = tempfile::tempdir().expect("tempdir");
        write_solid_png(&temp.path().join("red.png"), 2, 2, [255, 0, 0, 255]);
        write_solid_png(&temp.path().join("green.png"), 3, 1, [0, 255, 0, 255]);
        std::fs::write(
            temp.path().join("collection.tsx"),
            r#"<?xml version="1.0" encoding="UTF-8"?>
            <tileset name="collection" tilewidth="3" tileheight="2" tilecount="2" columns="0">
                <tile id="0" width="2" height="2">
                    <image width="2" height="2" source="red.png"/>
                </tile>
                <tile id="1" width="3" height="1">
                    <image width="3" height="1" source="green.png"/>
                </tile>
            </tileset>"#,
        )
        .expect("write tsx");
        let tmx = r#"<?xml version="1.0" encoding="UTF-8"?>
        <map version="1.10" orientation="orthogonal" renderorder="right-down" width="2" height="1" tilewidth="3" tileheight="2">
            <tileset firstgid="1" source="collection.tsx"/>
            <layer name="Ground" width="2" height="1">
                <data><tile gid="1"/><tile gid="2"/></data>
            </layer>
        </map>"#;

        let import = TiledImport::from_tmx_str(tmx, temp.path())
            .expect("multi-image image collection should import");

        assert_eq!(import.tileset.image_size, [6, 2]);
        assert_eq!(import.tileset.tile_rects.len(), 2);
        assert_eq!(
            import.tileset.tile_rects[0],
            Some(TilesetTileRect::new(0, 0, 2, 2))
        );
        assert_eq!(
            import.tileset.tile_rects[1],
            Some(TilesetTileRect::new(3, 0, 3, 1))
        );
        assert_eq!(import.tileset.tile_images.len(), 2);

        let texture = import
            .load_tileset_texture()
            .expect("multi-image collection atlas should load");
        assert_eq!(texture.size(), [6, 2]);
        assert_eq!(texture_pixel(&texture, 0, 0), [255, 0, 0, 255]);
        assert_eq!(texture_pixel(&texture, 2, 0), [0, 0, 0, 0]);
        assert_eq!(texture_pixel(&texture, 3, 0), [0, 255, 0, 255]);

        let grid = import.tileset_grid(Handle::<TextureAsset>::new(crate::asset::AssetId::new()));
        assert_eq!(grid.tile_draw_size(TileId(0)), Some([2, 2]));
        assert_eq!(grid.tile_draw_size(TileId(1)), Some([3, 1]));
        for (actual, expected) in grid
            .uv_rect(TileId(1))
            .expect("second tile uv")
            .into_iter()
            .zip([0.5, 0.0, 1.0, 0.5])
        {
            assert!((actual - expected).abs() <= 1e-6);
        }
    }

    #[test]
    fn imports_inline_json_image_collection_tileset_with_multiple_source_images() {
        let temp = tempfile::tempdir().expect("tempdir");
        write_solid_png(&temp.path().join("red.png"), 2, 2, [255, 0, 0, 255]);
        write_solid_png(&temp.path().join("green.png"), 3, 1, [0, 255, 0, 255]);
        let json = r#"{
            "orientation": "orthogonal",
            "renderorder": "right-down",
            "width": 2,
            "height": 1,
            "tilewidth": 3,
            "tileheight": 2,
            "layers": [{
                "name": "Ground",
                "type": "tilelayer",
                "width": 2,
                "height": 1,
                "data": [1, 2]
            }],
            "tilesets": [{
                "firstgid": 1,
                "name": "collection",
                "tilewidth": 3,
                "tileheight": 2,
                "columns": 2,
                "tilecount": 2,
                "tiles": [{
                    "id": 0,
                    "image": "red.png",
                    "imagewidth": 2,
                    "imageheight": 2,
                    "width": 2,
                    "height": 2
                }, {
                    "id": 1,
                    "image": "green.png",
                    "imagewidth": 3,
                    "imageheight": 1,
                    "width": 3,
                    "height": 1
                }]
            }]
        }"#;

        let import =
            TiledImport::from_json_str(json, temp.path()).expect("inline image collection import");

        assert_eq!(import.map.tile(0, 0, 0).unwrap().id, TileId(0));
        assert_eq!(import.map.tile(0, 1, 0).unwrap().id, TileId(1));
        assert_eq!(import.tileset.image_size, [6, 2]);
        assert_eq!(
            import.tileset.tile_rects[0],
            Some(TilesetTileRect::new(0, 0, 2, 2))
        );
        assert_eq!(
            import.tileset.tile_rects[1],
            Some(TilesetTileRect::new(3, 0, 3, 1))
        );
        assert_eq!(import.tileset.tile_images.len(), 2);

        let texture = import
            .load_tileset_texture()
            .expect("inline image collection atlas should load");
        assert_eq!(texture.size(), [6, 2]);
        assert_eq!(texture_pixel(&texture, 0, 0), [255, 0, 0, 255]);
        assert_eq!(texture_pixel(&texture, 2, 0), [0, 0, 0, 0]);
        assert_eq!(texture_pixel(&texture, 3, 0), [0, 255, 0, 255]);
    }

    #[test]
    fn imports_isometric_large_tile_over_small_cell_without_trimming() {
        let temp = tempfile::tempdir().expect("tempdir");
        write_transparent_png_with_rect(
            &temp.path().join("tower.png"),
            256,
            512,
            TilesetTileRect::new(96, 320, 64, 128),
            [80, 180, 255, 255],
        );
        std::fs::write(
            temp.path().join("large.tsx"),
            r#"<?xml version="1.0" encoding="UTF-8"?>
            <tileset name="large" tilewidth="256" tileheight="512" tilecount="1" columns="0">
                <tile id="0" width="256" height="512">
                    <image width="256" height="512" source="tower.png"/>
                </tile>
            </tileset>"#,
        )
        .expect("write tsx");
        let tmx = r#"<?xml version="1.0" encoding="UTF-8"?>
        <map version="1.10" orientation="isometric" renderorder="right-down" width="1" height="1" tilewidth="256" tileheight="128">
            <tileset firstgid="1" source="large.tsx"/>
            <layer name="Ground" width="1" height="1">
                <data><tile gid="1"/></data>
            </layer>
        </map>"#;

        let import = TiledImport::from_tmx_str(tmx, temp.path())
            .expect("large isometric tile fixture should import");
        assert_eq!(import.tile_size, [256, 128]);
        assert_eq!(import.tileset.tile_size, [256, 512]);
        assert_eq!(import.tileset.tile_draw_size(TileId(0)), [256, 512]);

        let texture = import
            .load_tileset_texture()
            .expect("large transparent tile texture should load");
        assert_eq!(texture.size(), [256, 512]);
        assert_eq!(texture_pixel(&texture, 0, 0), [0, 0, 0, 0]);
        assert_eq!(texture_pixel(&texture, 96, 320), [80, 180, 255, 255]);

        let renderer = import
            .renderer_for_layer(
                TilemapHandle::new(0, 0),
                &[Handle::<TextureAsset>::new(crate::asset::AssetId::new())],
                0,
            )
            .expect("layer renderer");
        assert_eq!(renderer.tile_size, [256.0, 128.0]);
        assert_eq!(renderer.tile_draw_size, [256.0, 512.0]);
        assert_eq!(renderer.cell_to_local_origin([0, 0]), [-128.0, -64.0]);
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
