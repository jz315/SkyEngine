use std::path::PathBuf;

#[derive(Debug)]
pub(crate) enum KajiyaBackendError {
    WindowHandle(String),
    UnsupportedWindowHandle(String),
    CreateRenderer {
        component: &'static str,
        source: String,
    },
    PrepareFrame {
        frame: &'static str,
        source: String,
    },
    Asset(String),
    CacheIo {
        action: &'static str,
        path: PathBuf,
        source: String,
    },
    ImageBuild(String),
    MeshUpload {
        mesh: String,
        source: String,
    },
}

impl KajiyaBackendError {
    pub(crate) fn create_renderer(component: &'static str, source: impl std::fmt::Debug) -> Self {
        Self::CreateRenderer {
            component,
            source: format!("{source:?}"),
        }
    }

    pub(crate) fn prepare_frame(frame: &'static str, source: impl std::fmt::Debug) -> Self {
        Self::PrepareFrame {
            frame,
            source: format!("{source:?}"),
        }
    }

    pub(crate) fn asset(message: impl Into<String>) -> Self {
        Self::Asset(message.into())
    }

    pub(crate) fn cache_io(
        action: &'static str,
        path: impl Into<PathBuf>,
        source: impl std::fmt::Display,
    ) -> Self {
        Self::CacheIo {
            action,
            path: path.into(),
            source: source.to_string(),
        }
    }

    pub(crate) fn image_build(source: impl std::fmt::Debug) -> Self {
        Self::ImageBuild(format!("{source:?}"))
    }

    pub(crate) fn mesh_upload(mesh: impl Into<String>, source: impl std::fmt::Debug) -> Self {
        Self::MeshUpload {
            mesh: mesh.into(),
            source: format!("{source:?}"),
        }
    }
}

impl std::fmt::Display for KajiyaBackendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WindowHandle(message) => write!(f, "failed to query window handle: {message}"),
            Self::UnsupportedWindowHandle(message) => write!(f, "{message}"),
            Self::CreateRenderer { component, source } => {
                write!(f, "failed to create Kajiya {component}: {source}")
            }
            Self::PrepareFrame { frame, source } => {
                write!(f, "failed to prepare Kajiya {frame} frame: {source}")
            }
            Self::Asset(message) => write!(f, "{message}"),
            Self::CacheIo {
                action,
                path,
                source,
            } => {
                write!(f, "failed to {action} `{}`: {source}", path.display())
            }
            Self::ImageBuild(source) => write!(f, "failed to build Kajiya image asset: {source}"),
            Self::MeshUpload { mesh, source } => {
                write!(f, "failed to upload Kajiya mesh `{mesh}`: {source}")
            }
        }
    }
}

impl std::error::Error for KajiyaBackendError {}
