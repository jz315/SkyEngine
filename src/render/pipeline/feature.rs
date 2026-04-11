use std::borrow::Cow;
use std::sync::{Arc, Mutex};

use crate::gpu::GpuContext;
use crate::render::frame_pipeline::{
    FrameContextExecuteAccess, FrameContextSetupAccess, FrameViewNode, PhaseState, PreparedFrame,
    PreparedView, TextureFormat, TextureSlot, ViewExecutionContext,
};
use crate::render::graph::{CompiledPass, PhysicalResources, RenderGraph, RenderGraphError};
use crate::render::scene::{
    default_scene_view_from_prepared, RenderInjectionPoint, RenderOutputFormat, SceneView,
};
use crate::render::RenderSettings;

pub struct RenderFeatureSetupContext<'a, 'frame> {
    access: FrameContextSetupAccess<'a, 'frame>,
    injection_point: &'a RenderInjectionPoint,
    stage: Option<&'a str>,
    queue: Option<&'a str>,
}

impl<'a, 'frame> RenderFeatureSetupContext<'a, 'frame> {
    #[inline]
    pub fn frame(&self) -> &PreparedFrame<'frame> {
        self.access.frame()
    }

    #[inline]
    pub fn view(&self) -> &PreparedView<'frame> {
        self.access.view()
    }

    #[inline]
    pub fn scene_view(&self) -> SceneView {
        self.access
            .view_payload::<SceneView>()
            .copied()
            .unwrap_or_else(|| default_scene_view_from_prepared(self.view()))
    }

    #[inline]
    pub fn injection_point(&self) -> &RenderInjectionPoint {
        self.injection_point
    }

    #[inline]
    pub fn stage(&self) -> Option<&str> {
        self.stage
    }

    #[inline]
    pub fn queue(&self) -> Option<&str> {
        self.queue
    }

    #[inline]
    pub fn surface_format(&self) -> TextureFormat {
        self.access.surface_format()
    }

    #[inline]
    pub fn has_surface(&self) -> bool {
        self.access.has_surface()
    }

    #[inline]
    pub fn current_color(&self) -> Option<TextureSlot> {
        self.access.current_color()
    }

    #[inline]
    pub fn texture_slot(&self, name: &str) -> Option<TextureSlot> {
        self.access.texture_slot(name)
    }

    #[inline]
    pub fn set_current_color(
        &mut self,
        handle: crate::render::expert::TextureHandle,
        format: TextureFormat,
    ) {
        self.access.set_current_color(handle, format);
    }

    #[inline]
    pub fn set_texture_slot(
        &mut self,
        name: impl Into<Cow<'static, str>>,
        handle: crate::render::expert::TextureHandle,
        format: TextureFormat,
    ) -> Option<TextureSlot> {
        self.access.set_texture_slot(name, handle, format)
    }

    #[inline]
    pub fn graph_mut(&mut self) -> &mut RenderGraph {
        self.access.graph_mut()
    }

    #[inline]
    pub fn render_settings(&self) -> RenderSettings {
        self.access
            .frame_payload::<RenderSettings>()
            .copied()
            .unwrap_or_default()
    }
}

pub struct RenderFeatureExecuteContext<'a, 'frame> {
    access: FrameContextExecuteAccess<'a, 'frame>,
    injection_point: &'a RenderInjectionPoint,
    stage: Option<&'a str>,
    queue: Option<&'a str>,
}

impl<'a, 'frame> RenderFeatureExecuteContext<'a, 'frame> {
    #[inline]
    pub fn pass(&self) -> &CompiledPass {
        self.access.pass()
    }

    #[inline]
    pub fn gpu(&mut self) -> &mut GpuContext {
        self.access.gpu()
    }

    #[inline]
    pub fn resources(&self) -> &PhysicalResources<'frame> {
        self.access.resources()
    }

    #[inline]
    pub fn frame(&self) -> &PreparedFrame<'frame> {
        self.access.frame()
    }

    #[inline]
    pub fn view(&self) -> &PreparedView<'frame> {
        self.access.view()
    }

    #[inline]
    pub fn scene_view(&self) -> SceneView {
        self.access
            .view_payload::<SceneView>()
            .copied()
            .unwrap_or_else(|| default_scene_view_from_prepared(self.view()))
    }

    #[inline]
    pub fn injection_point(&self) -> &RenderInjectionPoint {
        self.injection_point
    }

    #[inline]
    pub fn stage(&self) -> Option<&str> {
        self.stage
    }

    #[inline]
    pub fn queue(&self) -> Option<&str> {
        self.queue
    }

    #[inline]
    pub fn render_settings(&self) -> RenderSettings {
        self.access
            .frame_payload::<RenderSettings>()
            .copied()
            .unwrap_or_default()
    }
}

pub trait RenderFeature: Send {
    fn name(&self) -> &'static str;

    fn is_enabled(&self, _frame: &PreparedFrame<'_>) -> bool {
        true
    }

    fn output_format_hint(&self) -> RenderOutputFormat {
        RenderOutputFormat::Preserve
    }

    fn setup(&mut self, ctx: &mut RenderFeatureSetupContext<'_, '_>);

    fn execute(
        &mut self,
        ctx: &mut RenderFeatureExecuteContext<'_, '_>,
    ) -> Result<(), RenderGraphError>;

    fn draw_calls(&self, _frame: &PreparedFrame<'_>, _view: &PreparedView<'_>) -> usize {
        0
    }

    fn resize(&mut self, _ctx: &GpuContext, _width: u32, _height: u32) {}
}

#[derive(Clone)]
pub(crate) struct SharedRenderFeature(Arc<Mutex<Box<dyn RenderFeature>>>);

impl SharedRenderFeature {
    #[inline]
    pub(crate) fn new(feature: Box<dyn RenderFeature>) -> Self {
        Self(Arc::new(Mutex::new(feature)))
    }

    fn with_feature<R>(&self, f: impl FnOnce(&mut dyn RenderFeature) -> R) -> R {
        let mut feature = self
            .0
            .lock()
            .expect("render feature lock should not be poisoned");
        f(feature.as_mut())
    }

    #[inline]
    pub(crate) fn name(&self) -> &'static str {
        self.with_feature(|feature| feature.name())
    }

    #[inline]
    pub(crate) fn is_enabled(&self, frame: &PreparedFrame<'_>) -> bool {
        self.with_feature(|feature| feature.is_enabled(frame))
    }

    #[inline]
    pub(crate) fn output_format_hint(&self) -> RenderOutputFormat {
        self.with_feature(|feature| feature.output_format_hint())
    }

    #[inline]
    pub(crate) fn setup(&self, ctx: &mut RenderFeatureSetupContext<'_, '_>) {
        self.with_feature(|feature| feature.setup(ctx));
    }

    #[inline]
    pub(crate) fn execute(
        &self,
        ctx: &mut RenderFeatureExecuteContext<'_, '_>,
    ) -> Result<(), RenderGraphError> {
        self.with_feature(|feature| feature.execute(ctx))
    }

    #[inline]
    pub(crate) fn draw_calls(&self, frame: &PreparedFrame<'_>, view: &PreparedView<'_>) -> usize {
        self.with_feature(|feature| feature.draw_calls(frame, view))
    }

    #[inline]
    pub(crate) fn resize(&self, ctx: &GpuContext, width: u32, height: u32) {
        self.with_feature(|feature| feature.resize(ctx, width, height));
    }
}

pub(crate) struct RenderFeatureNode {
    pub(crate) feature: SharedRenderFeature,
    pub(crate) injection_point: RenderInjectionPoint,
    pub(crate) stage: Option<Cow<'static, str>>,
    pub(crate) queue: Option<Cow<'static, str>>,
}

impl FrameViewNode for RenderFeatureNode {
    fn name(&self) -> &'static str {
        self.feature.name()
    }

    fn is_enabled(&self, frame: &PreparedFrame<'_>) -> bool {
        self.feature.is_enabled(frame)
    }

    fn setup(
        &mut self,
        graph: &mut RenderGraph,
        state: &mut PhaseState,
        frame: &PreparedFrame<'_>,
        view: &PreparedView<'_>,
    ) {
        let mut ctx = RenderFeatureSetupContext {
            access: FrameContextSetupAccess::new(graph, state, frame, view),
            injection_point: &self.injection_point,
            stage: self.stage.as_deref(),
            queue: self.queue.as_deref(),
        };
        self.feature.setup(&mut ctx);
    }

    fn execute(
        &mut self,
        pass: &CompiledPass,
        ctx: &mut GpuContext,
        resources: &PhysicalResources<'_>,
        execution: &ViewExecutionContext<'_>,
    ) -> Result<(), RenderGraphError> {
        let mut feature_ctx = RenderFeatureExecuteContext {
            access: FrameContextExecuteAccess::new(pass, ctx, resources, execution),
            injection_point: &self.injection_point,
            stage: self.stage.as_deref(),
            queue: self.queue.as_deref(),
        };
        self.feature.execute(&mut feature_ctx)
    }

    fn draw_calls(&self, execution: &ViewExecutionContext<'_>) -> usize {
        self.feature.draw_calls(execution.frame(), execution.view())
    }

    fn resize(&mut self, ctx: &GpuContext, width: u32, height: u32) {
        self.feature.resize(ctx, width, height);
    }
}
