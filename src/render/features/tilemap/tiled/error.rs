use std::fmt;
use std::path::PathBuf;

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
