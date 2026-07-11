use std::path::{Path, PathBuf};

use super::error::TiledImportError;

pub(super) fn required_attr<'a, 'input>(
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

pub(super) fn required_u32_attr(
    node: roxmltree::Node<'_, '_>,
    name: &str,
) -> Result<u32, TiledImportError> {
    parse_u32_attr(node, name, required_attr(node, name)?)
}

pub(super) fn optional_u32_attr(
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

pub(super) fn required_i32_attr(
    node: roxmltree::Node<'_, '_>,
    name: &str,
) -> Result<i32, TiledImportError> {
    parse_i32_attr(node, name, required_attr(node, name)?)
}

pub(super) fn optional_i32_attr(
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

pub(super) fn optional_f32_attr(
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

pub(super) fn optional_bool_attr(
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

pub(super) fn resolve_path(base_dir: &Path, path: impl AsRef<Path>) -> PathBuf {
    let path = path.as_ref();
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base_dir.join(path)
    }
}
