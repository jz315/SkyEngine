use std::io::Read;

use super::error::TiledImportError;
use super::json::TiledLayerData;

pub(super) fn decode_json_gids(
    data: &TiledLayerData,
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

pub(super) fn decode_tmx_gids(
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
                values.push(required_tile_gid(tile)?);
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

fn required_tile_gid(tile: roxmltree::Node<'_, '_>) -> Result<u32, TiledImportError> {
    let value = tile.attribute("gid").ok_or_else(|| {
        TiledImportError::MalformedMap("<tile> is missing required `gid` attribute".to_string())
    })?;
    value.parse::<u32>().map_err(|_| {
        TiledImportError::MalformedMap(format!("<tile> has invalid `gid` attribute `{value}`"))
    })
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
    if !bytes.len().is_multiple_of(4) {
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
