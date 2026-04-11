use crate::gpu::GpuContext;

use super::gpu_scene::GpuScene2D;
use super::prepared::PreparedRenderWorld2D;
use super::scene_cache::SceneCache2D;

#[derive(Default)]
pub(crate) struct GpuSceneRuntime {
    scene: Option<GpuScene2D>,
}

impl GpuSceneRuntime {
    #[inline]
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn upload_scene_frame<'a>(
        &'a mut self,
        gpu: &GpuContext,
        scene_cache: &mut SceneCache2D,
        prepared: &PreparedRenderWorld2D,
    ) -> &'a GpuScene2D {
        let scene = self.scene.get_or_insert_with(|| GpuScene2D::new(gpu));
        scene.upload_scene_frame(gpu, scene_cache, prepared);
        scene
    }

    #[inline]
    pub(crate) fn ready(&self) -> Option<&GpuScene2D> {
        self.scene.as_ref()
    }
}
