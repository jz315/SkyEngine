use std::any::Any;

use crate::ecs::World;
use crate::gpu::GpuContext;
use crate::render::execution::{PreparedFrame, PreparedView};
use crate::render::phase::{OpaquePhase, TransparentPhase};
use crate::render::view::{ResolvedSceneTransforms, SceneView};

use super::RenderPipelineBuilder;

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
