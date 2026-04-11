use crate::render::resources::material::MaterialError;

/// Errors returned by [`super::MeshPass`] rendering APIs.
#[derive(Debug, Clone, PartialEq)]
pub enum MeshPassError {
    Material(MaterialError),
    MissingMaterial {
        pipeline: String,
    },
    TargetSampleCountMismatch {
        pipeline: String,
        pipeline_samples: u32,
        target_samples: u32,
    },
    SurfaceSampleCountMismatch {
        pipeline: String,
        pipeline_samples: u32,
    },
    MissingDepthTarget {
        pipeline: String,
    },
    UnexpectedDepthTarget {
        pipeline: String,
    },
    DepthFormatMismatch {
        pipeline: String,
        pipeline_format: wgpu::TextureFormat,
        target_format: wgpu::TextureFormat,
    },
    DepthSampleCountMismatch {
        color_samples: u32,
        depth_samples: u32,
    },
    AliasingTargets,
    VertexRangeOutOfBounds {
        mesh: String,
        start: u32,
        end: u32,
        vertex_count: u32,
    },
    IndexRangeOutOfBounds {
        mesh: String,
        start: u32,
        end: u32,
        index_count: u32,
    },
    MissingIndexBuffer {
        mesh: String,
    },
    InvalidInstanceRange {
        start: u32,
        end: u32,
    },
}

impl std::fmt::Display for MeshPassError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Material(err) => write!(f, "{err}"),
            Self::MissingMaterial { pipeline } => {
                write!(f, "Mesh pipeline \"{pipeline}\" requires a material instance")
            }
            Self::TargetSampleCountMismatch {
                pipeline,
                pipeline_samples,
                target_samples,
            } => write!(
                f,
                "Mesh pipeline \"{pipeline}\" uses sample_count={}, but the color target uses {}",
                pipeline_samples, target_samples
            ),
            Self::SurfaceSampleCountMismatch {
                pipeline,
                pipeline_samples,
            } => write!(
                f,
                "Mesh pipeline \"{pipeline}\" uses sample_count={}, but surface rendering only supports 1x",
                pipeline_samples
            ),
            Self::MissingDepthTarget { pipeline } => {
                write!(f, "Mesh pipeline \"{pipeline}\" requires a depth target")
            }
            Self::UnexpectedDepthTarget { pipeline } => write!(
                f,
                "Mesh pipeline \"{pipeline}\" does not declare depth-stencil state"
            ),
            Self::DepthFormatMismatch {
                pipeline,
                pipeline_format,
                target_format,
            } => write!(
                f,
                "Mesh pipeline \"{pipeline}\" expects depth format {pipeline_format:?}, got {target_format:?}"
            ),
            Self::DepthSampleCountMismatch {
                color_samples,
                depth_samples,
            } => write!(
                f,
                "Color/depth sample counts must match, got color={} depth={}",
                color_samples, depth_samples
            ),
            Self::AliasingTargets => {
                write!(f, "MeshPass requires distinct color and depth targets")
            }
            Self::VertexRangeOutOfBounds {
                mesh,
                start,
                end,
                vertex_count,
            } => write!(
                f,
                "Vertex range {start}..{end} is out of bounds for mesh \"{mesh}\" with {} vertices",
                vertex_count
            ),
            Self::IndexRangeOutOfBounds {
                mesh,
                start,
                end,
                index_count,
            } => write!(
                f,
                "Index range {start}..{end} is out of bounds for mesh \"{mesh}\" with {} indices",
                index_count
            ),
            Self::MissingIndexBuffer { mesh } => {
                write!(f, "Mesh \"{mesh}\" has no index buffer")
            }
            Self::InvalidInstanceRange { start, end } => {
                write!(f, "Invalid instance range {start}..{end}")
            }
        }
    }
}

impl std::error::Error for MeshPassError {}

impl From<MaterialError> for MeshPassError {
    fn from(value: MaterialError) -> Self {
        Self::Material(value)
    }
}
