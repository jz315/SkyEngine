use std::path::PathBuf;

use super::super::TileId;
use super::error::TiledImportError;
use super::json::TiledJsonObject;
use super::layer::{gid_without_flags, tile_flags_from_gid, tileset_index_for_gid};
use super::properties::{collect_json_properties, collect_tmx_properties};
use super::types::{TiledObject, TiledObjectShape, TiledProperty, TiledTileset};
use super::util::{optional_f32_attr, optional_u32_attr, required_u32_attr, resolve_path};

#[derive(Clone, Debug)]
pub(super) struct RawObject {
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

pub(super) fn collect_tmx_objects(
    object_group: roxmltree::Node<'_, '_>,
    base_dir: &std::path::Path,
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

pub(super) fn collect_json_object(
    object: &TiledJsonObject,
    base_dir: &std::path::Path,
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

pub(super) fn decode_object(
    object: RawObject,
    tilesets: &[TiledTileset],
    map_pixel_height: f32,
) -> Result<Option<TiledObject>, TiledImportError> {
    let tile_payload = match object.gid {
        Some(gid_with_flags) => {
            let gid = gid_without_flags(gid_with_flags);
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
