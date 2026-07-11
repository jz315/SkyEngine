//! Crate-private extension points for frame work that belongs to a renderer
//! family rather than to the runtime kernel.
//!
//! The core runtime owns ordering and failure handling; concrete renderer
//! families own their GPU state, view augmentation, preparation, and payloads.

#[cfg(test)]
use std::any::Any;

use crate::ecs::World;
use crate::gpu::GpuContext;
use crate::render::execution::{PreparedFrame, PreparedView};
use crate::render::view::ResolvedSceneTransforms;
use crate::render::view::{RenderStats, SceneView};

use super::frame::{ExtractedFrame, SceneUploadFrame};
use super::state::{FrameRuntimeState, RenderResourceHub};

#[derive(Debug)]
pub(crate) struct FrameExtensionError {
    pub(crate) extension: &'static str,
    pub(crate) message: String,
}

pub(crate) struct FrameExtensionInitContext<'a> {
    pub(crate) gpu: &'a GpuContext,
}

pub(crate) struct FrameExtensionViewContext<'a> {
    pub(crate) world: &'a World,
    pub(crate) views: &'a mut Vec<SceneView>,
}

pub(crate) struct FrameExtensionPrepareContext<'a> {
    pub(crate) gpu: &'a mut GpuContext,
    pub(crate) resources: &'a RenderResourceHub,
    pub(crate) runtime: &'a FrameRuntimeState,
    pub(crate) extracted: &'a ExtractedFrame,
    pub(crate) uploads: &'a SceneUploadFrame,
}

pub(crate) struct FrameExtensionUploadContext<'a> {
    pub(crate) world: &'a World,
    pub(crate) transforms: &'a ResolvedSceneTransforms,
    pub(crate) upload: &'a mut SceneUploadFrame,
}

/// An erased feature-owned contribution to one prepared frame.
///
/// This remains crate-private deliberately: public renderer customization uses
/// `RenderFeature`, phases, passes, and extractors.  The adapter prevents the
/// core runtime from learning about a concrete feature's state or resources.
pub(crate) trait FrameExtension {
    #[cfg(test)]
    fn as_any(&self) -> &dyn Any;

    fn initialize(&mut self, _ctx: FrameExtensionInitContext<'_>) {}

    fn collect_views(&mut self, _ctx: FrameExtensionViewContext<'_>) {}

    fn upload_scene(&mut self, _ctx: FrameExtensionUploadContext<'_>) {}

    fn prepare(
        &mut self,
        _ctx: FrameExtensionPrepareContext<'_>,
    ) -> Result<(), FrameExtensionError> {
        Ok(())
    }

    fn insert_frame_payloads<'a>(&'a self, _frame: &mut PreparedFrame<'a>) {}

    fn insert_view_payloads<'a>(
        &'a self,
        _view_index: usize,
        _view: &SceneView,
        _prepared_view: &mut PreparedView<'a>,
    ) {
    }

    fn update_render_stats(&self, _stats: &mut RenderStats) {}

    fn invalidate(&mut self) {}
}
