use std::any::Any;

use crate::ecs::World;
use crate::gpu::GpuContext;
use crate::render::execution::{PreparedFrame, PreparedView};
use crate::render::phase::{OpaquePhase, TransparentPhase};
use crate::render::resources::material::{StandardMaterial, UnlitMaterial};
use crate::render::view::{ResolvedSceneTransforms, SceneView};

use super::RenderPipelineBuilder;

pub struct SpriteFeature;

impl SpriteFeature {
    #[inline]
    pub fn new() -> Self {
        Self::lit_hdr()
    }

    #[inline]
    pub fn lit_hdr() -> Self {
        Self
    }

    #[inline]
    pub fn unlit() -> Self {
        Self
    }
}

impl Default for SpriteFeature {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "live2d")]
pub use crate::render::live2d::Live2DFeature;

pub(crate) trait AnyRenderFeature {
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn extract(
        &mut self,
        world: &World,
        transforms: &ResolvedSceneTransforms,
        surface_size: [u32; 2],
    );
    fn collect_views(&self, views: &mut Vec<SceneView>);
    fn prepare(&mut self, gpu: &mut GpuContext, views: &[SceneView]);
    fn append_phase_items(
        &self,
        view_index: usize,
        opaque_phase: &mut OpaquePhase,
        transparent_phase: &mut TransparentPhase,
    );
    fn insert_frame_payloads<'a>(&'a self, frame: &mut PreparedFrame<'a>);
    fn insert_view_payloads<'a>(
        &'a self,
        view_index: usize,
        view: &SceneView,
        prepared_view: &mut PreparedView<'a>,
    );
}

impl<T> AnyRenderFeature for T
where
    T: RenderFeature,
{
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn extract(
        &mut self,
        world: &World,
        transforms: &ResolvedSceneTransforms,
        surface_size: [u32; 2],
    ) {
        RenderFeature::extract(self, world, transforms, surface_size);
    }

    fn collect_views(&self, views: &mut Vec<SceneView>) {
        RenderFeature::collect_views(self, views);
    }

    fn prepare(&mut self, gpu: &mut GpuContext, views: &[SceneView]) {
        RenderFeature::prepare(self, gpu, views);
    }

    fn append_phase_items(
        &self,
        view_index: usize,
        opaque_phase: &mut OpaquePhase,
        transparent_phase: &mut TransparentPhase,
    ) {
        RenderFeature::append_phase_items(self, view_index, opaque_phase, transparent_phase);
    }

    fn insert_frame_payloads<'a>(&'a self, frame: &mut PreparedFrame<'a>) {
        RenderFeature::insert_frame_payloads(self, frame);
    }

    fn insert_view_payloads<'a>(
        &'a self,
        view_index: usize,
        view: &SceneView,
        prepared_view: &mut PreparedView<'a>,
    ) {
        RenderFeature::insert_view_payloads(self, view_index, view, prepared_view);
    }
}

pub trait RenderFeature: Any + 'static {
    fn name(&self) -> &'static str;
    fn register(&mut self, builder: &mut RenderPipelineBuilder);

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

impl RenderFeature for SpriteFeature {
    fn name(&self) -> &'static str {
        "sprite"
    }

    fn register(&mut self, builder: &mut RenderPipelineBuilder) {
        let current = std::mem::take(builder);
        *builder = current
            .register_material::<crate::render::SpriteMaterial>()
            .register_material::<UnlitMaterial>()
            .register_material::<StandardMaterial>();
    }
}

#[cfg(feature = "live2d")]
impl RenderFeature for Live2DFeature {
    fn name(&self) -> &'static str {
        "live2d"
    }

    fn register(&mut self, builder: &mut RenderPipelineBuilder) {
        self.set_draw_function_id(
            builder.register_draw_function(crate::render::live2d::DrawLive2D::new()),
        );
    }

    fn extract(
        &mut self,
        world: &World,
        transforms: &ResolvedSceneTransforms,
        surface_size: [u32; 2],
    ) {
        Live2DFeature::extract(self, world, transforms, surface_size);
    }

    fn collect_views(&self, views: &mut Vec<SceneView>) {
        Live2DFeature::collect_views(self, views);
    }

    fn prepare(&mut self, gpu: &mut GpuContext, views: &[SceneView]) {
        self.set_target_format(gpu.surface_format());
        Live2DFeature::prepare(self, gpu, views);
    }

    fn append_phase_items(
        &self,
        view_index: usize,
        _opaque_phase: &mut OpaquePhase,
        transparent_phase: &mut TransparentPhase,
    ) {
        if let Some(phase_view) = self.phase_view(view_index) {
            for item in phase_view.transparent_phase.items() {
                transparent_phase.add_item(item.clone());
            }
        }
    }

    fn insert_frame_payloads<'a>(&'a self, frame: &mut PreparedFrame<'a>) {
        Live2DFeature::insert_frame_payloads(self, frame);
    }

    fn insert_view_payloads<'a>(
        &'a self,
        view_index: usize,
        view: &SceneView,
        prepared_view: &mut PreparedView<'a>,
    ) {
        Live2DFeature::insert_view_payloads(self, view_index, view, prepared_view);
    }
}
