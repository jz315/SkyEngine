use crate::ecs::World;
use crate::gpu::{GpuContext, GpuError, GpuInitError};
use crate::render::pipeline::RenderBackendKind;
use crate::render::resources::texture_cache::SharedRenderAssetCache;
use crate::render::runtime::RenderRuntime;
use crate::render::view::RenderStats;

/// Error returned when a scene renderer cannot be created.
#[derive(Debug)]
pub enum SceneRendererInitError {
    Wgpu(GpuInitError),
    KajiyaUnavailable(String),
    RenderlingUnavailable(String),
    Other(String),
}

impl std::fmt::Display for SceneRendererInitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Wgpu(error) => write!(f, "{error}"),
            Self::KajiyaUnavailable(message) => write!(f, "{message}"),
            Self::RenderlingUnavailable(message) => write!(f, "{message}"),
            Self::Other(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for SceneRendererInitError {}

impl From<GpuInitError> for SceneRendererInitError {
    fn from(value: GpuInitError) -> Self {
        Self::Wgpu(value)
    }
}

/// Error returned by a renderer while acquiring or submitting a frame.
#[derive(Debug)]
pub enum SceneRendererError {
    Wgpu(GpuError),
    Other(String),
}

impl std::fmt::Display for SceneRendererError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Wgpu(error) => write!(f, "{error}"),
            Self::Other(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for SceneRendererError {}

impl From<GpuError> for SceneRendererError {
    fn from(value: GpuError) -> Self {
        Self::Wgpu(value)
    }
}

/// Backend-neutral renderer interface used by the app runner.
pub trait SceneRenderer {
    fn backend_kind(&self) -> RenderBackendKind;
    fn begin_frame(&mut self) -> Result<(), SceneRendererError>;
    fn end_frame(&mut self);
    fn render_world(&mut self, world: &World);
    fn resize(&mut self, width: u32, height: u32);
    fn surface_lost(&mut self);
    fn stats(&self) -> RenderStats;
    fn surface_size(&self) -> [u32; 2];
    fn adapter_name(&self) -> &str;
    fn backend_name(&self) -> &str;

    fn wgpu(&self) -> Option<&GpuContext> {
        None
    }

    fn wgpu_mut(&mut self) -> Option<&mut GpuContext> {
        None
    }

    fn wgpu_render_runtime_mut(&mut self) -> Option<&mut RenderRuntime> {
        None
    }

    fn wgpu_render_runtime_parts_mut(&mut self) -> Option<(&mut RenderRuntime, &mut GpuContext)> {
        None
    }

    fn wgpu_overlay_parts_mut(
        &mut self,
    ) -> Option<(&mut GpuContext, Option<&SharedRenderAssetCache>)> {
        None
    }
}
