use crate::ecs::World;
use crate::gpu::GpuContext;
use crate::render::frame_pipeline::{FrameViewNode, PreparedFrame, PreparedView};
use crate::render::scene::{RenderQueueSort, RenderStats, ResolvedSceneTransforms, SceneView};
use crate::render::stats::{elapsed_ms, timing_start};

use super::composite_node::SpriteCompositeNode;
use super::config::{Ecs2DRenderPath, SpriteBackendStats};
use super::extractor::SceneExtractor;
use super::feature::{SpriteDomainFeature, SpriteDomainFeatureAdapter, SpriteDomainStage};
use super::light_node::SpriteLightNode;
use super::prepared::PreparedRenderWorld2D;
use super::runtime::GpuSceneRuntime;
use super::scene_cache::SceneCache2D;
use super::sprite_pass::SpriteSceneNode;

pub struct SpriteBackendBuilder {
    config: Ecs2DRenderPath,
    features: Vec<Box<dyn SpriteDomainFeature>>,
}

impl SpriteBackendBuilder {
    pub fn new() -> Self {
        Self {
            config: Ecs2DRenderPath::lit_hdr(),
            features: Vec::new(),
        }
    }

    #[inline]
    pub fn unlit(mut self) -> Self {
        self.config = Ecs2DRenderPath::unlit();
        self
    }

    #[inline]
    pub fn lit_hdr(mut self) -> Self {
        self.config = Ecs2DRenderPath::lit_hdr();
        self
    }

    pub fn build(self) -> SpriteBackend {
        SpriteBackend {
            config: self.config,
            extractor: SceneExtractor::new(),
            prepared_world: PreparedRenderWorld2D::new(),
            gpu_scene: GpuSceneRuntime::new(),
            scene_cache: SceneCache2D::new(),
            surface_size: [1, 1],
            sort_policy: RenderQueueSort::TransparentScene,
            features: self.features,
            stats: SpriteBackendStats::default(),
        }
    }
}

impl Default for SpriteBackendBuilder {
    fn default() -> Self {
        Self::new()
    }
}

pub struct SpriteBackend {
    config: Ecs2DRenderPath,
    extractor: SceneExtractor,
    prepared_world: PreparedRenderWorld2D,
    gpu_scene: GpuSceneRuntime,
    scene_cache: SceneCache2D,
    surface_size: [u32; 2],
    sort_policy: RenderQueueSort,
    features: Vec<Box<dyn SpriteDomainFeature>>,
    stats: SpriteBackendStats,
}

impl SpriteBackend {
    #[inline]
    pub fn builder() -> SpriteBackendBuilder {
        SpriteBackendBuilder::new()
    }

    pub(crate) fn set_sort_policy(&mut self, sort_policy: RenderQueueSort) {
        self.sort_policy = sort_policy;
        self.scene_cache.set_sort_policy(sort_policy);
    }

    fn build_nodes(&mut self, ctx: &GpuContext) -> Vec<Box<dyn FrameViewNode>> {
        let mut before_sprites = Vec::new();
        let mut after_sprites = Vec::new();
        let mut after_lighting = Vec::new();
        let mut after_composite = Vec::new();

        for feature in self.features.drain(..) {
            let stage = feature.stage();
            let adapter = Box::new(SpriteDomainFeatureAdapter::new(feature));
            match stage {
                SpriteDomainStage::BeforeSprites => {
                    before_sprites.push(adapter as Box<dyn FrameViewNode>)
                }
                SpriteDomainStage::AfterSprites => {
                    after_sprites.push(adapter as Box<dyn FrameViewNode>)
                }
                SpriteDomainStage::AfterLighting => {
                    after_lighting.push(adapter as Box<dyn FrameViewNode>)
                }
                SpriteDomainStage::AfterComposite => {
                    after_composite.push(adapter as Box<dyn FrameViewNode>)
                }
            }
        }

        let mut nodes: Vec<Box<dyn FrameViewNode>> = Vec::new();
        nodes.extend(before_sprites);
        if self.config.uses_hdr() {
            nodes.push(Box::new(SpriteSceneNode::hdr(ctx)));
        } else {
            nodes.push(Box::new(SpriteSceneNode::surface(ctx)));
        }
        nodes.extend(after_sprites);
        if self.config.uses_hdr() {
            nodes.push(Box::new(SpriteLightNode::new(ctx)));
            nodes.extend(after_lighting);
            nodes.push(Box::new(SpriteCompositeNode::new(ctx)));
        } else {
            nodes.extend(after_lighting);
        }
        nodes.extend(after_composite);
        nodes
    }
}

impl SpriteBackend {
    pub(crate) fn extract(
        &mut self,
        world: &World,
        transforms: &ResolvedSceneTransforms,
        surface_size: [u32; 2],
    ) {
        let extract_start = timing_start();
        self.extractor.sync_incremental(
            world,
            transforms,
            &mut self.scene_cache,
            surface_size,
            self.config.default_settings(),
        );
        self.stats.timings.extract_ms = elapsed_ms(extract_start);
    }

    pub(crate) fn collect_views(&self, _views: &mut Vec<SceneView>) {}

    pub(crate) fn prepare(&mut self, gpu: &mut GpuContext, _world: &World, views: &[SceneView]) {
        let [width, height] = gpu.surface_size();
        let resize_start = timing_start();
        self.stats.timings.resize_ms = if self.surface_size != [width, height] {
            self.surface_size = [width, height];
            elapsed_ms(resize_start)
        } else {
            0.0
        };

        let prepare_start = timing_start();
        self.prepared_world
            .prepare_scene(&mut self.scene_cache, views, [width, height]);
        self.stats.view_count = views.len();
        self.stats.timings.prepare_ms = elapsed_ms(prepare_start);

        let upload_start = timing_start();
        let gpu_scene =
            self.gpu_scene
                .upload_scene_frame(gpu, &mut self.scene_cache, &self.prepared_world);
        self.stats.timings.upload_ms = elapsed_ms(upload_start);
        self.stats.sprite_count = gpu_scene.sprite_count();
        self.stats.light_count = gpu_scene.light_count();
        self.stats.dirty_sprite_slots = gpu_scene.dirty_sprite_slot_uploads();
        self.stats.dirty_light_slots = gpu_scene.dirty_light_slot_uploads();
        self.stats.visible_sprite_upload_count = gpu_scene.visible_sprite_upload_count();
        self.stats.visible_light_upload_count = gpu_scene.visible_light_upload_count();
    }

    pub(crate) fn insert_frame_payloads<'a>(&'a self, frame: &mut PreparedFrame<'a>) {
        let _ = frame.insert_payload(&self.scene_cache.settings);
        if let Some(gpu_scene) = self.gpu_scene.ready() {
            let _ = frame.insert_payload(gpu_scene);
        }
    }

    pub(crate) fn insert_view_payloads<'a>(
        &'a self,
        view_index: usize,
        _view: &SceneView,
        prepared_view: &mut PreparedView<'a>,
    ) {
        if let Some(view) = self
            .gpu_scene
            .ready()
            .and_then(|gpu_scene| gpu_scene.views().get(view_index))
        {
            let _ = prepared_view.insert_payload(view);
        }
    }

    pub(crate) fn create_view_nodes(&mut self, ctx: &GpuContext) -> Vec<Box<dyn FrameViewNode>> {
        self.build_nodes(ctx)
    }

    pub(crate) fn wants_hdr_output(&self) -> bool {
        self.config.uses_hdr()
    }

    pub(crate) fn resize(&mut self, _ctx: &GpuContext, width: u32, height: u32) {
        self.surface_size = [width, height];
    }

    pub(crate) fn surface_lost(&mut self) {}

    pub(crate) fn populate_render_stats(&self, stats: &mut RenderStats) {
        let Some(gpu_scene) = self.gpu_scene.ready() else {
            return;
        };
        stats.view_count = stats.view_count.max(self.stats.view_count);
        stats.sprite_count = gpu_scene.sprite_count();
        stats.light_count = gpu_scene.light_count();
        stats.dirty_sprite_slots = gpu_scene.dirty_sprite_slot_uploads();
        stats.dirty_light_slots = gpu_scene.dirty_light_slot_uploads();
        stats.visible_sprite_upload_count = gpu_scene.visible_sprite_upload_count();
        stats.visible_light_upload_count = gpu_scene.visible_light_upload_count();
        stats.timings.extract_ms = self.stats.timings.extract_ms;
        stats.timings.resize_ms = self.stats.timings.resize_ms;
        stats.timings.prepare_ms = self.stats.timings.prepare_ms;
        stats.timings.upload_ms = self.stats.timings.upload_ms;
    }
}
