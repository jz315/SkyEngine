use std::fmt;
use std::path::PathBuf;

use super::super::io::tiled::{TiledExportError, TiledImportError};
use super::super::model::{CellCoord, LayerKind};

#[derive(Debug)]
pub enum TileError {
    NotFound(String),
    WrongLayerKind {
        name: String,
        expected: LayerKind,
        found: LayerKind,
    },
    OutOfBounds {
        cell: CellCoord,
        size: [u32; 2],
    },
    InvalidOperation(String),
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    Format(String),
    UnboundMap,
    UndoUnavailable,
}

impl fmt::Display for TileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(name) => write!(f, "tile item not found: {name}"),
            Self::WrongLayerKind {
                name,
                expected,
                found,
            } => write!(
                f,
                "tile layer `{name}` has kind {found:?}, expected {expected:?}"
            ),
            Self::OutOfBounds { cell, size } => write!(
                f,
                "tile cell [{}, {}] is outside map bounds {}x{}",
                cell.x, cell.y, size[0], size[1]
            ),
            Self::InvalidOperation(message) => write!(f, "invalid tile operation: {message}"),
            Self::Io { path, source } => {
                write!(f, "tile I/O error at {}: {source}", path.display())
            }
            Self::Format(message) => write!(f, "tile format error: {message}"),
            Self::UnboundMap => write!(
                f,
                "tile map is not bound to a save format; call save_as_tiled first"
            ),
            Self::UndoUnavailable => write!(f, "tile undo/redo operation is unavailable"),
        }
    }
}

impl std::error::Error for TileError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<TiledImportError> for TileError {
    fn from(value: TiledImportError) -> Self {
        Self::Format(value.to_string())
    }
}

impl From<TiledExportError> for TileError {
    fn from(value: TiledExportError) -> Self {
        match value {
            TiledExportError::Io { path, source } => Self::Io { path, source },
            other => Self::Format(other.to_string()),
        }
    }
}
