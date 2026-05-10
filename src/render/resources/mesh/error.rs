/// Errors returned by fallible mesh APIs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MeshError {
    EmptyVertices,
    EmptyIndices,
    InvalidVertexLayout {
        stride: u32,
    },
    VertexDataSizeMismatch {
        bytes: usize,
        stride: u32,
        vertex_count: u32,
    },
    IndexedSubMeshOutOfBounds {
        end: u32,
        index_count: u32,
    },
    NonIndexedSubMeshHasIndices {
        index_offset: u32,
        index_count: u32,
    },
    GltfImport {
        path: String,
        message: String,
    },
    GltfMissingMesh {
        path: String,
    },
    GltfMissingPositions {
        mesh: String,
        primitive: usize,
    },
    GltfUnsupportedPrimitiveMode {
        mesh: String,
        primitive: usize,
        mode: String,
    },
    GltfTooManyVertices {
        count: usize,
    },
    GltfTooManyIndices {
        count: usize,
    },
}

impl std::fmt::Display for MeshError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyVertices => write!(f, "Mesh requires at least one vertex"),
            Self::EmptyIndices => write!(f, "Indexed mesh requires at least one index"),
            Self::InvalidVertexLayout { stride } => {
                write!(f, "Mesh vertex layout must use a non-zero stride (got {stride})")
            }
            Self::VertexDataSizeMismatch {
                bytes,
                stride,
                vertex_count,
            } => write!(
                f,
                "Mesh vertex payload size mismatch: {bytes} bytes cannot describe {vertex_count} vertices with stride {stride}"
            ),
            Self::IndexedSubMeshOutOfBounds { end, index_count } => write!(
                f,
                "Sub-mesh index range ends at {end}, beyond mesh index count {index_count}"
            ),
            Self::NonIndexedSubMeshHasIndices {
                index_offset,
                index_count,
            } => write!(
                f,
                "Non-indexed meshes cannot use sub-mesh index ranges (offset {index_offset}, count {index_count})"
            ),
            Self::GltfImport { path, message } => {
                write!(f, "Failed to import glTF mesh from `{path}`: {message}")
            }
            Self::GltfMissingMesh { path } => {
                write!(f, "glTF file `{path}` did not contain any triangle mesh primitives")
            }
            Self::GltfMissingPositions { mesh, primitive } => write!(
                f,
                "glTF mesh `{mesh}` primitive {primitive} is missing POSITION data"
            ),
            Self::GltfUnsupportedPrimitiveMode {
                mesh,
                primitive,
                mode,
            } => write!(
                f,
                "glTF mesh `{mesh}` primitive {primitive} uses unsupported mode `{mode}`"
            ),
            Self::GltfTooManyVertices { count } => {
                write!(f, "glTF mesh expands to {count} vertices, exceeding u32 indexing")
            }
            Self::GltfTooManyIndices { count } => {
                write!(f, "glTF mesh expands to {count} indices, exceeding u32 indexing")
            }
        }
    }
}

impl std::error::Error for MeshError {}
