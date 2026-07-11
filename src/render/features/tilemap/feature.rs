use std::sync::{Arc, Mutex};

use crate::ecs::World;
use crate::gpu::GpuContext;
use crate::render::execution::{PreparedFrame, PreparedView};
use crate::render::features::tilemap::{
    DrawTilemap, ExtractTilemaps, TilemapCacheConfig, TilemapFrameCache,
};
use crate::render::phase::{OpaquePhase, TransparentPhase};
use crate::render::pipeline::{RenderFeature, RenderPipelineBuilder};
use crate::render::view::{ResolvedSceneTransforms, SceneView};

/// Render feature for chunked 2D tilemaps.
pub struct TilemapFeature {
    cache_config: TilemapCacheConfig,
}

impl TilemapFeature {
    #[inline]
    pub fn new() -> Self {
        Self::unlit()
    }

    #[inline]
    pub fn unlit() -> Self {
        Self {
            cache_config: TilemapCacheConfig::default(),
        }
    }

    #[inline]
    pub fn with_cache_config(mut self, cache_config: TilemapCacheConfig) -> Self {
        self.cache_config = cache_config;
        self
    }
}

impl Default for TilemapFeature {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderFeature for TilemapFeature {
    fn name(&self) -> &'static str {
        "tilemap"
    }

    fn register(&mut self, builder: &mut RenderPipelineBuilder) {
        let cache = Arc::new(Mutex::new(TilemapFrameCache::with_config(
            self.cache_config,
        )));
        let draw_function_id = builder.register_draw_function(DrawTilemap::new(cache.clone()));
        let current = std::mem::take(builder);
        *builder = current.add_extractor(ExtractTilemaps::new(draw_function_id, cache));
    }

    fn extract(
        &mut self,
        _world: &World,
        _transforms: &ResolvedSceneTransforms,
        _surface_size: [u32; 2],
    ) {
    }

    fn collect_views(&self, _views: &mut Vec<SceneView>) {}

    fn prepare(&mut self, _gpu: &mut GpuContext, _views: &[SceneView]) {}

    fn append_phase_items(
        &self,
        _view_index: usize,
        _opaque_phase: &mut OpaquePhase,
        _transparent_phase: &mut TransparentPhase,
    ) {
    }

    fn insert_frame_payloads<'a>(&'a self, _frame: &mut PreparedFrame<'a>) {}

    fn insert_view_payloads<'a>(
        &'a self,
        _view_index: usize,
        _view: &SceneView,
        _prepared_view: &mut PreparedView<'a>,
    ) {
    }
}
