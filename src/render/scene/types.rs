use std::borrow::Cow;
use std::fmt;

use crate::render::frame_pipeline::TextureFormat;
use crate::render::stats::RenderTimingStats;

#[derive(Debug, Clone, Copy, Default)]
pub struct RenderStats {
    pub producer_count: usize,
    pub view_count: usize,
    pub sprite_count: usize,
    pub light_count: usize,
    pub draw_calls: usize,
    pub passes: usize,
    pub dirty_sprite_slots: usize,
    pub dirty_light_slots: usize,
    pub visible_sprite_upload_count: usize,
    pub visible_light_upload_count: usize,
    pub timings: RenderTimingStats,
}

pub(crate) const SCENE_HDR_FORMAT: TextureFormat = wgpu::TextureFormat::Rgba16Float;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderOutputFormat {
    Preserve,
    Surface,
    Hdr,
    Fixed(TextureFormat),
}

impl RenderOutputFormat {
    #[inline]
    pub(crate) fn apply(self, input: TextureFormat, surface: TextureFormat) -> TextureFormat {
        match self {
            Self::Preserve => input,
            Self::Surface => surface,
            Self::Hdr => SCENE_HDR_FORMAT,
            Self::Fixed(format) => format,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RenderStageKey(Cow<'static, str>);

impl RenderStageKey {
    #[inline]
    pub fn new(name: impl Into<Cow<'static, str>>) -> Self {
        Self(name.into())
    }

    #[inline]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&'static str> for RenderStageKey {
    fn from(value: &'static str) -> Self {
        Self::new(value)
    }
}

impl From<String> for RenderStageKey {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl fmt::Display for RenderStageKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_str().fmt(f)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderQueueSort {
    OpaqueDepthFrontToBack,
    TransparentScene,
    OverlayStable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderQueueDesc {
    name: Cow<'static, str>,
    stage: RenderStageKey,
    sort_policy: RenderQueueSort,
}

impl RenderQueueDesc {
    pub fn new(
        name: impl Into<Cow<'static, str>>,
        stage: impl Into<RenderStageKey>,
        sort_policy: RenderQueueSort,
    ) -> Self {
        Self {
            name: name.into(),
            stage: stage.into(),
            sort_policy,
        }
    }

    #[inline]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[inline]
    pub fn stage(&self) -> &RenderStageKey {
        &self.stage
    }

    #[inline]
    pub fn sort_policy(&self) -> RenderQueueSort {
        self.sort_policy
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderInjectionPoint {
    BeforeStage(RenderStageKey),
    AfterStage(RenderStageKey),
    BeforeQueue(Cow<'static, str>),
    AfterQueue(Cow<'static, str>),
    BeforePresent,
}

impl RenderInjectionPoint {
    #[inline]
    pub fn before_stage(stage: impl Into<RenderStageKey>) -> Self {
        Self::BeforeStage(stage.into())
    }

    #[inline]
    pub fn after_stage(stage: impl Into<RenderStageKey>) -> Self {
        Self::AfterStage(stage.into())
    }

    #[inline]
    pub fn before_queue(queue: impl Into<Cow<'static, str>>) -> Self {
        Self::BeforeQueue(queue.into())
    }

    #[inline]
    pub fn after_queue(queue: impl Into<Cow<'static, str>>) -> Self {
        Self::AfterQueue(queue.into())
    }
}
