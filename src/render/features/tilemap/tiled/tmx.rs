use std::path::Path;

use super::data::decode_tmx_gids;
use super::error::TiledImportError;
use super::layer::{LayerContext, ParsedLayer, RawCell};
use super::object::{collect_tmx_objects, RawObject};
use super::properties::collect_tmx_properties;
use super::util::{
    optional_bool_attr, optional_f32_attr, optional_i32_attr, required_i32_attr, required_u32_attr,
};

#[derive(Clone, Debug)]
pub(super) struct ParsedObjectLayer {
    pub(super) name: String,
    pub(super) source_order: i32,
    pub(super) visible: bool,
    pub(super) opacity: f32,
    pub(super) offset: [f32; 2],
    pub(super) parallax: [f32; 2],
    pub(super) properties: Vec<super::types::TiledProperty>,
    pub(super) objects: Vec<RawObject>,
}

pub(super) fn collect_tmx_child_layers(
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
