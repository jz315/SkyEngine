use crate::gpu::GpuContext;
use crate::render::ecs::RenderSettings;
use crate::render::frame_pipeline::{
    FrameContextExecuteAccess, FrameContextSetupAccess, FrameViewNode, PhaseState, PreparedFrame,
    PreparedView, TextureFormat, TextureSlot, ViewExecutionContext,
};
use crate::render::graph::{
    CompiledPass, PhysicalResources, RenderGraph, RenderGraphError, TextureHandle,
};

use super::gpu_scene::GpuScene2D;
use super::prepared::PreparedView2D;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpriteDomainStage {
    BeforeSprites,
    AfterSprites,
    AfterLighting,
    AfterComposite,
}

pub struct SpriteDomainSetupContext<'a, 'frame> {
    access: FrameContextSetupAccess<'a, 'frame>,
}

impl<'a, 'frame> SpriteDomainSetupContext<'a, 'frame> {
    #[inline]
    pub fn frame(&self) -> &PreparedFrame<'frame> {
        self.access.frame()
    }

    #[inline]
    pub fn view(&self) -> &PreparedView<'frame> {
        self.access.view()
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
    pub fn set_current_color(&mut self, handle: TextureHandle, format: TextureFormat) {
        self.access.set_current_color(handle, format);
    }

    #[inline]
    pub fn set_texture_slot(
        &mut self,
        name: impl Into<std::borrow::Cow<'static, str>>,
        handle: TextureHandle,
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

    #[inline]
    pub fn gpu_scene(&self) -> Option<&GpuScene2D> {
        self.access.frame_payload::<GpuScene2D>()
    }

    #[inline]
    pub fn prepared_view(&self) -> Option<&PreparedView2D> {
        self.access.view_payload::<PreparedView2D>()
    }
}

pub struct SpriteDomainExecuteContext<'a, 'frame> {
    access: FrameContextExecuteAccess<'a, 'frame>,
}

impl<'a, 'frame> SpriteDomainExecuteContext<'a, 'frame> {
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
    pub fn view_index(&self) -> usize {
        self.access.view_index()
    }

    #[inline]
    pub fn render_settings(&self) -> RenderSettings {
        self.access
            .frame_payload::<RenderSettings>()
            .copied()
            .unwrap_or_default()
    }

    #[inline]
    pub fn gpu_scene(&self) -> Option<&GpuScene2D> {
        self.access.frame_payload::<GpuScene2D>()
    }

    #[inline]
    pub fn prepared_view(&self) -> Option<&PreparedView2D> {
        self.access.view_payload::<PreparedView2D>()
    }
}

pub trait SpriteDomainFeature: Send {
    fn name(&self) -> &'static str;

    fn stage(&self) -> SpriteDomainStage;

    fn is_enabled(&self, _frame: &PreparedFrame<'_>) -> bool {
        true
    }

    fn setup(&mut self, ctx: &mut SpriteDomainSetupContext<'_, '_>);

    fn execute(
        &mut self,
        ctx: &mut SpriteDomainExecuteContext<'_, '_>,
    ) -> Result<(), RenderGraphError>;

    fn draw_calls(&self, _frame: &PreparedFrame<'_>, _view: &PreparedView<'_>) -> usize {
        0
    }

    fn resize(&mut self, _ctx: &GpuContext, _width: u32, _height: u32) {}
}

pub(crate) struct SpriteDomainFeatureAdapter {
    feature: Box<dyn SpriteDomainFeature>,
}

impl SpriteDomainFeatureAdapter {
    pub(crate) fn new(feature: Box<dyn SpriteDomainFeature>) -> Self {
        Self { feature }
    }
}

impl FrameViewNode for SpriteDomainFeatureAdapter {
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
        let mut ctx = SpriteDomainSetupContext {
            access: FrameContextSetupAccess::new(graph, state, frame, view),
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
        let mut feature_ctx = SpriteDomainExecuteContext {
            access: FrameContextExecuteAccess::new(pass, ctx, resources, execution),
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
