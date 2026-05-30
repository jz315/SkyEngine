use crate::ecs::World;
use crate::gpu::{GpuContext, GpuError, GpuInitError};
use crate::render::pipeline::RenderBackendKind;
use crate::render::resources::texture_cache::SharedRenderAssetCache;
use crate::render::runtime::{FrameSkipReason, RenderRuntime};
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

/// Token for an acquired renderer frame.
///
/// Normal app code receives this from [`SceneRenderer::begin_frame`], passes it
/// to render/overlay/screenshot operations, and must give it back to
/// [`SceneRenderer::end_frame`].
#[derive(Debug)]
#[must_use = "a SceneFrame must be finished through SceneRenderer::end_frame"]
pub struct SceneFrame {
    backend_kind: RenderBackendKind,
    render_outcome: SceneRenderOutcome,
    pre_present_notified: bool,
}

impl SceneFrame {
    #[inline]
    pub(crate) fn new(backend_kind: RenderBackendKind) -> Self {
        Self {
            backend_kind,
            render_outcome: SceneRenderOutcome::NotRendered,
            pre_present_notified: false,
        }
    }

    #[inline]
    pub fn backend_kind(&self) -> RenderBackendKind {
        self.backend_kind
    }

    #[inline]
    pub fn render_outcome(&self) -> SceneRenderOutcome {
        self.render_outcome
    }

    #[inline]
    pub fn is_presentable(&self) -> bool {
        self.render_outcome.is_presentable()
    }

    #[inline]
    pub(crate) fn set_render_outcome(&mut self, outcome: SceneRenderOutcome) {
        self.render_outcome = outcome;
    }

    #[inline]
    pub(crate) fn mark_pre_present_notified(&mut self) {
        self.pre_present_notified = true;
    }

    #[inline]
    pub fn pre_present_notified(&self) -> bool {
        self.pre_present_notified
    }
}

/// App-visible result of a backend render operation for the current frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SceneRenderOutcome {
    NotRendered,
    Rendered,
    Cleared(SceneFrameClearReason),
    Skipped(SceneFrameSkipReason),
}

impl SceneRenderOutcome {
    #[inline]
    pub fn is_presentable(self) -> bool {
        matches!(self, Self::Rendered | Self::Cleared(_))
    }
}

/// Why the app or backend cleared the acquired presentation frame instead of
/// leaving its contents undefined.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SceneFrameClearReason {
    NoRenderCall,
    MissingPipeline,
    RuntimeSkipped(FrameSkipReason),
    OverlayWithoutScene,
    ScreenshotWithoutScene,
}

/// Why a frame has no presentable contents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SceneFrameSkipReason {
    MissingPipeline,
    RuntimeSkipped(FrameSkipReason),
    ClearUnsupported(SceneFrameClearReason),
    BackendFrameUnavailable,
    BackendRenderFailed,
}

/// Backend-neutral renderer interface used by the app runner.
pub trait SceneRenderer {
    fn backend_kind(&self) -> RenderBackendKind;
    fn presents_during_render(&self) -> bool {
        false
    }
    fn begin_frame(&mut self) -> Result<SceneFrame, SceneRendererError>;
    fn end_frame(&mut self, frame: SceneFrame);
    fn render_world(&mut self, frame: &mut SceneFrame, world: &World) -> SceneRenderOutcome;
    fn clear_frame(
        &mut self,
        frame: &mut SceneFrame,
        _world: &World,
        reason: SceneFrameClearReason,
    ) -> SceneRenderOutcome {
        let outcome = SceneRenderOutcome::Skipped(SceneFrameSkipReason::ClearUnsupported(reason));
        frame.set_render_outcome(outcome);
        outcome
    }
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
