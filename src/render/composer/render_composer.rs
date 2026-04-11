use crate::ecs::World;
use crate::gpu::GpuContext;
use crate::render::ecs::RenderSettings;
use crate::render::frame_pipeline::FramePipeline;
use crate::render::pipeline::{
    CompiledRenderPipeline, DomainEntry, FeatureEntry, RenderPipelineAsset,
};
use crate::render::scene::{RenderStats, ResolvedSceneTransforms, SceneView};

use super::WorldViewCollector;

pub struct RenderComposer {
    pub(crate) compiled: CompiledRenderPipeline,
    pub(crate) domains: Vec<DomainEntry>,
    pub(crate) features: Vec<FeatureEntry>,
    pub(crate) pipeline: Option<FramePipeline>,
    pub(crate) last_stats: RenderStats,
    pub(crate) surface_size: [u32; 2],
    pub(crate) frame_settings: RenderSettings,
    pub(crate) view_collector: WorldViewCollector,
}

impl RenderComposer {
    pub fn from_asset(asset: RenderPipelineAsset) -> Self {
        Self {
            compiled: asset.compiled,
            domains: asset.domains,
            features: asset.features,
            pipeline: None,
            last_stats: RenderStats::default(),
            surface_size: [1, 1],
            frame_settings: RenderSettings::default(),
            view_collector: WorldViewCollector::default(),
        }
    }

    #[inline]
    pub fn stats(&self) -> RenderStats {
        self.last_stats
    }

    pub(crate) fn collect_world_views(
        &mut self,
        world: &World,
        transforms: &ResolvedSceneTransforms,
    ) -> Vec<SceneView> {
        self.view_collector
            .collect_world_views(world, transforms, self.surface_size)
    }

    pub(crate) fn resolve_scene_transforms(&mut self, world: &World) -> ResolvedSceneTransforms {
        self.view_collector.resolve_transforms(world)
    }

    pub fn resize(&mut self, gpu: &GpuContext, width: u32, height: u32) {
        self.surface_size = [width.max(1), height.max(1)];
        if let Some(pipeline) = self.pipeline.as_mut() {
            pipeline.resize(gpu, width, height);
        }
        for entry in &mut self.domains {
            entry.domain.resize(gpu, width, height);
        }
    }

    pub fn surface_lost(&mut self) {
        for entry in &mut self.domains {
            entry.domain.surface_lost();
        }
    }

    pub fn domain_mut<T: 'static>(&mut self) -> Option<&mut T> {
        for entry in &mut self.domains {
            let any = entry.domain.as_any_mut();
            if any.is::<T>() {
                return any.downcast_mut::<T>();
            }
        }
        None
    }
}
