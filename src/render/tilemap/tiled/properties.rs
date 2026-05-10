use std::path::Path;

use crate::render::Color;

use super::error::TiledImportError;
use super::json::TiledJsonProperty;
use super::types::{TiledProperty, TiledPropertyValue};
use super::util::{required_attr, resolve_path};

pub(super) fn collect_tmx_properties(
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

pub(super) fn collect_json_properties(
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
