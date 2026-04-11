#![allow(unused_imports)]

use std::any::Any;

use crate::ecs::World;
use crate::gpu::GpuContext;
use crate::render::frame_pipeline::{FrameViewNode, PreparedFrame, PreparedView, TextureFormat};
use crate::render::scene::{
    RenderQueueSort, RenderStats, ResolvedSceneTransforms, SceneView, SCENE_HDR_FORMAT,
};

pub mod sprite;

#[cfg(feature = "live2d")]
pub mod live2d;

pub trait RenderDomain: Any {
    fn name(&self) -> &'static str;

    fn configure_queue_sort(&mut self, _sort_policy: RenderQueueSort) {}

    fn configure_target_format(&mut self, _target_format: TextureFormat) {}

    fn output_format_hint(
        &self,
        input_format: TextureFormat,
        _surface_format: TextureFormat,
    ) -> TextureFormat {
        if self.wants_hdr_output() {
            SCENE_HDR_FORMAT
        } else {
            input_format
        }
    }

    fn extract(
        &mut self,
        _world: &World,
        _transforms: &ResolvedSceneTransforms,
        _surface_size: [u32; 2],
    ) {
    }

    fn collect_views(&self, _views: &mut Vec<SceneView>) {}

    fn prepare(&mut self, _gpu: &mut GpuContext, _world: &World, _views: &[SceneView]) {}

    fn insert_frame_payloads<'a>(&'a self, _frame: &mut PreparedFrame<'a>) {}

    fn insert_view_payloads<'a>(
        &'a self,
        _view_index: usize,
        _view: &SceneView,
        _prepared_view: &mut PreparedView<'a>,
    ) {
    }

    fn create_view_nodes(&mut self, _ctx: &GpuContext) -> Vec<Box<dyn FrameViewNode>> {
        Vec::new()
    }

    fn wants_hdr_output(&self) -> bool {
        false
    }

    fn resize(&mut self, _ctx: &GpuContext, _width: u32, _height: u32) {}

    fn surface_lost(&mut self) {}

    fn populate_stats(&self, _stats: &mut RenderStats) {}

    fn as_any_mut(&mut self) -> &mut dyn Any;
}

pub use sprite::SpriteDomain;
pub use sprite::{
    GpuScene2D, PreparedView2D, SpriteBackend, SpriteBackendBuilder, SpriteDomainExecuteContext,
    SpriteDomainFeature, SpriteDomainSetupContext, SpriteDomainStage,
};
pub(crate) use sprite::{
    PreparedRenderWorld2D, SceneCache2D, SceneExtractor, SceneLightItem, SceneSpriteItem,
};

#[cfg(feature = "live2d")]
pub use live2d::{Live2DBackend, Live2DDomain};
