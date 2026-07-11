use std::path::{Path, PathBuf};

use super::super::{TileAnimation, TileAnimationFrame, TileId, TilesetTileRect};
use super::error::TiledImportError;
use super::json::{
    TiledJsonProperty, TiledJsonTile, TiledJsonTileOffset, TiledJsonTilesetFile,
    TiledJsonTilesetRef,
};
use super::properties::{collect_json_properties, collect_tmx_properties};
use super::types::{TiledProperty, TiledTileset, TiledTilesetImageSource};
use super::util::{
    optional_i32_attr, optional_u32_attr, required_attr, required_u32_attr, resolve_path,
};

pub(super) fn resolve_tmx_tilesets(
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

pub(super) fn resolve_json_tilesets(
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
                        if file.tiles.iter().any(|tile| tile.image.is_some()) {
                            build_json_image_collection_tileset(
                                tileset.firstgid,
                                file.tilewidth,
                                file.tileheight,
                                file.transparentcolor.as_deref(),
                                file.tileoffset.as_ref(),
                                &file.properties,
                                &file.tiles,
                                source_base,
                                Some(source_path.clone()),
                            )?
                        } else {
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
                    }
                    "tsx" => load_tmx_tileset_file(tileset.firstgid, &source_path)?,
                    _ => {
                        return Err(TiledImportError::UnsupportedExternalTileset {
                            source: source_path,
                        });
                    }
                }
            }
            None => {
                if tileset.tiles.iter().any(|tile| tile.image.is_some()) {
                    build_json_image_collection_tileset(
                        tileset.firstgid,
                        tileset.tilewidth,
                        tileset.tileheight,
                        tileset.transparentcolor.as_deref(),
                        tileset.tileoffset.as_ref(),
                        &tileset.properties,
                        &tileset.tiles,
                        base_dir,
                        None,
                    )?
                } else {
                    build_tileset(
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
                    )?
                }
            }
        };
        resolved.push(resolved_tileset);
    }
    Ok(resolved)
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

    let mut max_tile_id = 0u32;
    let mut tiles = Vec::new();
    let mut unique_images = Vec::<PathBuf>::new();
    let mut single_image_size = [0, 0];

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
        if !unique_images.iter().any(|existing| existing == &image_path) {
            unique_images.push(image_path.clone());
        }

        let tile_id = required_u32_attr(tile, "id")?;
        let x = optional_u32_attr(tile, "x")?.unwrap_or_default();
        let y = optional_u32_attr(tile, "y")?.unwrap_or_default();
        let source_width = optional_u32_attr(image, "width")?;
        let source_height = optional_u32_attr(image, "height")?;
        single_image_size[0] = single_image_size[0].max(source_width.unwrap_or_default());
        single_image_size[1] = single_image_size[1].max(source_height.unwrap_or_default());
        let width = optional_u32_attr(tile, "width")?
            .or(source_width)
            .unwrap_or(tile_width)
            .max(1);
        let height = optional_u32_attr(tile, "height")?
            .or(source_height)
            .unwrap_or(tile_height)
            .max(1);
        max_tile_id = max_tile_id.max(tile_id);
        tiles.push(ImageCollectionTile {
            tile_id,
            image: image_path,
            source_rect: TilesetTileRect::new(x, y, width, height),
        });
    }

    let image_path =
        unique_images
            .first()
            .cloned()
            .ok_or_else(|| TiledImportError::UnsupportedTileset {
                source: source.clone(),
                reason: "only single-image tilesets are supported",
            })?;
    let mut rects = vec![None; max_tile_id as usize + 1];
    let mut tile_images = Vec::new();
    let image_size = if unique_images.len() <= 1 {
        for tile in &tiles {
            rects[tile.tile_id as usize] = Some(tile.source_rect);
        }
        let mut image_size = single_image_size;
        if image_size[0] == 0 || image_size[1] == 0 {
            let (width, height) =
                image::image_dimensions(&image_path).map_err(|source| TiledImportError::Image {
                    path: image_path.clone(),
                    source,
                })?;
            image_size = [width, height];
        }
        image_size
    } else {
        tile_images = vec![None; rects.len()];
        pack_image_collection_atlas(&tiles, &mut rects, &mut tile_images)
    };

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
        tile_images,
        tile_offset: parse_tmx_tile_offset(tileset)?,
        animations,
        properties: collect_tmx_properties(tileset, base_dir)?,
        tile_properties: collect_tmx_tile_properties(tileset, base_dir)?,
        transparent_color: None,
    })
}

fn build_json_image_collection_tileset(
    first_gid: u32,
    tilewidth: Option<u32>,
    tileheight: Option<u32>,
    transparent_color: Option<&str>,
    tileoffset: Option<&TiledJsonTileOffset>,
    properties: &[TiledJsonProperty],
    tileset_tiles: &[TiledJsonTile],
    base_dir: &Path,
    source: Option<PathBuf>,
) -> Result<TiledTileset, TiledImportError> {
    if first_gid == 0 {
        return Err(TiledImportError::UnsupportedTileset {
            source,
            reason: "firstgid must be greater than zero",
        });
    }

    let tile_width = tilewidth.ok_or_else(|| TiledImportError::UnsupportedTileset {
        source: source.clone(),
        reason: "tileset is missing tilewidth",
    })?;
    let tile_height = tileheight.ok_or_else(|| TiledImportError::UnsupportedTileset {
        source: source.clone(),
        reason: "tileset is missing tileheight",
    })?;

    let mut max_tile_id = 0u32;
    let mut tiles = Vec::new();
    let mut unique_images = Vec::<PathBuf>::new();
    let mut single_image_size = [0, 0];

    for tile in tileset_tiles {
        let Some(image) = tile.image.as_deref() else {
            continue;
        };
        let image_path = resolve_path(base_dir, image);
        if !unique_images.iter().any(|existing| existing == &image_path) {
            unique_images.push(image_path.clone());
        }

        let tile_id = tile.id;
        let x = tile.x.unwrap_or_default();
        let y = tile.y.unwrap_or_default();
        let source_width = tile.imagewidth.or(tile.width);
        let source_height = tile.imageheight.or(tile.height);
        single_image_size[0] = single_image_size[0].max(source_width.unwrap_or_default());
        single_image_size[1] = single_image_size[1].max(source_height.unwrap_or_default());
        let width = tile.width.or(source_width).unwrap_or(tile_width).max(1);
        let height = tile.height.or(source_height).unwrap_or(tile_height).max(1);
        max_tile_id = max_tile_id.max(tile_id);
        tiles.push(ImageCollectionTile {
            tile_id,
            image: image_path,
            source_rect: TilesetTileRect::new(x, y, width, height),
        });
    }

    let image_path =
        unique_images
            .first()
            .cloned()
            .ok_or_else(|| TiledImportError::UnsupportedTileset {
                source: source.clone(),
                reason: "only single-image tilesets are supported",
            })?;
    let mut rects = vec![None; max_tile_id as usize + 1];
    let mut tile_images = Vec::new();
    let image_size = if unique_images.len() <= 1 {
        for tile in &tiles {
            rects[tile.tile_id as usize] = Some(tile.source_rect);
        }
        let mut image_size = single_image_size;
        if image_size[0] == 0 || image_size[1] == 0 {
            let (width, height) =
                image::image_dimensions(&image_path).map_err(|source| TiledImportError::Image {
                    path: image_path.clone(),
                    source,
                })?;
            image_size = [width, height];
        }
        image_size
    } else {
        tile_images = vec![None; rects.len()];
        pack_image_collection_atlas(&tiles, &mut rects, &mut tile_images)
    };

    let animations = collect_json_tile_animations(tileset_tiles);
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
        tile_images,
        tile_offset: json_tile_offset(tileoffset),
        animations,
        properties: collect_json_properties(properties, base_dir)?,
        tile_properties: collect_json_tile_properties(tileset_tiles, base_dir)?,
        transparent_color: parse_transparent_color(transparent_color)?,
    })
}

struct ImageCollectionTile {
    tile_id: u32,
    image: PathBuf,
    source_rect: TilesetTileRect,
}

fn pack_image_collection_atlas(
    tiles: &[ImageCollectionTile],
    rects: &mut [Option<TilesetTileRect>],
    tile_images: &mut [Option<TiledTilesetImageSource>],
) -> [u32; 2] {
    let mut ordered = tiles.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|tile| tile.tile_id);

    let mut cursor_x = 0u32;
    let mut atlas_height = 1u32;
    for (index, tile) in ordered.into_iter().enumerate() {
        let source_rect = tile.source_rect;
        let atlas_rect = TilesetTileRect::new(cursor_x, 0, source_rect.width, source_rect.height);
        rects[tile.tile_id as usize] = Some(atlas_rect);
        tile_images[tile.tile_id as usize] = Some(TiledTilesetImageSource {
            image: tile.image.clone(),
            source_rect,
        });
        cursor_x = cursor_x.saturating_add(source_rect.width);
        if index + 1 < tiles.len() {
            cursor_x = cursor_x.saturating_add(1);
        }
        atlas_height = atlas_height.max(source_rect.height);
    }
    [cursor_x.max(1), atlas_height.max(1)]
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
        tile_images: Vec::new(),
        tile_offset,
        animations,
        properties,
        tile_properties,
        transparent_color,
    })
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

fn tiles_in_image_span(image_span: u32, tile_span: u32, margin: u32, spacing: u32) -> Option<u32> {
    let tile_span = tile_span.max(1);
    let available = image_span.checked_sub(margin.saturating_mul(2))?;
    if available < tile_span {
        return None;
    }
    Some(available.saturating_add(spacing) / tile_span.saturating_add(spacing).max(1))
}
