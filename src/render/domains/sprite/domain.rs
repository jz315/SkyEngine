use crate::ecs::World;
use crate::gpu::GpuContext;
use crate::render::domains::RenderDomain;
use crate::render::frame_pipeline::{FrameViewNode, PreparedFrame, PreparedView};
use crate::render::scene::{RenderQueueSort, RenderStats, ResolvedSceneTransforms, SceneView};

use super::backend::SpriteBackend;

pub struct SpriteDomain {
    backend: SpriteBackend,
    sort_policy: RenderQueueSort,
}

impl SpriteDomain {
    #[inline]
    pub fn new() -> Self {
        Self::lit_hdr()
    }

    #[inline]
    pub fn lit_hdr() -> Self {
        Self {
            backend: SpriteBackend::builder().lit_hdr().build(),
            sort_policy: RenderQueueSort::TransparentScene,
        }
    }

    #[inline]
    pub fn unlit() -> Self {
        Self {
            backend: SpriteBackend::builder().unlit().build(),
            sort_policy: RenderQueueSort::TransparentScene,
        }
    }
}

impl Default for SpriteDomain {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderDomain for SpriteDomain {
    fn name(&self) -> &'static str {
        "sprites"
    }

    fn configure_queue_sort(&mut self, sort_policy: RenderQueueSort) {
        self.sort_policy = sort_policy;
    }

    fn extract(
        &mut self,
        world: &World,
        transforms: &ResolvedSceneTransforms,
        surface_size: [u32; 2],
    ) {
        self.backend.extract(world, transforms, surface_size);
    }

    fn collect_views(&self, views: &mut Vec<SceneView>) {
        self.backend.collect_views(views);
    }

    fn prepare(&mut self, gpu: &mut GpuContext, world: &World, views: &[SceneView]) {
        self.backend.set_sort_policy(self.sort_policy);
        self.backend.prepare(gpu, world, views);
    }

    fn insert_frame_payloads<'a>(&'a self, frame: &mut PreparedFrame<'a>) {
        self.backend.insert_frame_payloads(frame);
    }

    fn insert_view_payloads<'a>(
        &'a self,
        view_index: usize,
        view: &SceneView,
        prepared_view: &mut PreparedView<'a>,
    ) {
        self.backend
            .insert_view_payloads(view_index, view, prepared_view);
    }

    fn create_view_nodes(&mut self, ctx: &GpuContext) -> Vec<Box<dyn FrameViewNode>> {
        self.backend.create_view_nodes(ctx)
    }

    fn wants_hdr_output(&self) -> bool {
        self.backend.wants_hdr_output()
    }

    fn resize(&mut self, ctx: &GpuContext, width: u32, height: u32) {
        self.backend.resize(ctx, width, height);
    }

    fn surface_lost(&mut self) {
        self.backend.surface_lost();
    }

    fn populate_stats(&self, stats: &mut RenderStats) {
        self.backend.populate_render_stats(stats);
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
