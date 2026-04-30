use std::any::Any;

use crate::gpu::GpuContext;
use crate::render::graph::{CompiledPass, PhysicalResources, RenderGraph, RenderGraphError};

use super::payload::{PreparedFrame, PreparedView};
use super::slots::{CompletedViewState, FinalizePhaseState, PhaseState};
use super::TextureFormat;

pub struct SetupExecutionContext<'a> {
    pub(crate) frame: &'a PreparedFrame<'a>,
}

impl<'a> SetupExecutionContext<'a> {
    #[inline]
    pub fn frame(&self) -> &PreparedFrame<'a> {
        self.frame
    }

    #[inline]
    pub fn payload<T: Any>(&self) -> Option<&'a T> {
        self.frame.payload::<T>()
    }

    #[inline]
    pub fn surface_format(&self) -> TextureFormat {
        self.frame.surface_format()
    }

    #[inline]
    pub fn has_surface(&self) -> bool {
        self.frame.has_surface()
    }
}

pub struct ViewExecutionContext<'a> {
    pub(crate) frame: &'a PreparedFrame<'a>,
    pub(crate) view: &'a PreparedView<'a>,
    pub(crate) view_state: &'a CompletedViewState,
    pub(crate) view_index: usize,
}

impl<'a> ViewExecutionContext<'a> {
    #[inline]
    pub fn frame(&self) -> &PreparedFrame<'a> {
        self.frame
    }

    #[inline]
    pub fn view(&self) -> &PreparedView<'a> {
        self.view
    }

    #[inline]
    pub fn view_state(&self) -> &'a CompletedViewState {
        self.view_state
    }

    #[inline]
    pub fn view_index(&self) -> usize {
        self.view_index
    }

    #[inline]
    pub fn frame_payload<T: Any>(&self) -> Option<&'a T> {
        self.frame.payload::<T>()
    }

    #[inline]
    pub fn view_payload<T: Any>(&self) -> Option<&'a T> {
        self.view.payload::<T>()
    }

    #[inline]
    pub fn surface_format(&self) -> TextureFormat {
        self.frame.surface_format()
    }

    #[inline]
    pub fn has_surface(&self) -> bool {
        self.frame.has_surface()
    }
}

pub struct FinalizeExecutionContext<'a> {
    pub(crate) frame: &'a PreparedFrame<'a>,
    pub(crate) completed_views: &'a [CompletedViewState],
}

impl<'a> FinalizeExecutionContext<'a> {
    #[inline]
    pub fn frame(&self) -> &PreparedFrame<'a> {
        self.frame
    }

    #[inline]
    pub fn payload<T: Any>(&self) -> Option<&'a T> {
        self.frame.payload::<T>()
    }

    #[inline]
    pub fn completed_views(&self) -> &'a [CompletedViewState] {
        self.completed_views
    }

    #[inline]
    pub fn surface_format(&self) -> TextureFormat {
        self.frame.surface_format()
    }

    #[inline]
    pub fn has_surface(&self) -> bool {
        self.frame.has_surface()
    }
}

pub trait FrameSetupNode: Send {
    fn name(&self) -> &'static str;

    fn is_enabled(&self, _frame: &PreparedFrame<'_>) -> bool {
        true
    }

    fn setup(&mut self, graph: &mut RenderGraph, state: &mut PhaseState, frame: &PreparedFrame<'_>);

    fn execute(
        &mut self,
        pass: &CompiledPass,
        ctx: &mut GpuContext,
        resources: &PhysicalResources<'_>,
        execution: &SetupExecutionContext<'_>,
    ) -> Result<(), RenderGraphError>;

    fn draw_calls(&self, _execution: &SetupExecutionContext<'_>) -> usize {
        0
    }

    fn resize(&mut self, _ctx: &GpuContext, _width: u32, _height: u32) {}
}

pub trait FrameViewNode: Send {
    fn name(&self) -> &'static str;

    fn is_enabled(&self, _frame: &PreparedFrame<'_>) -> bool {
        true
    }

    fn is_view_enabled(&self, _frame: &PreparedFrame<'_>, _view: &PreparedView<'_>) -> bool {
        true
    }

    fn setup(
        &mut self,
        graph: &mut RenderGraph,
        state: &mut PhaseState,
        frame: &PreparedFrame<'_>,
        view: &PreparedView<'_>,
    );

    fn execute(
        &mut self,
        pass: &CompiledPass,
        ctx: &mut GpuContext,
        resources: &PhysicalResources<'_>,
        execution: &ViewExecutionContext<'_>,
    ) -> Result<(), RenderGraphError>;

    fn draw_calls(&self, _execution: &ViewExecutionContext<'_>) -> usize {
        0
    }

    fn resize(&mut self, _ctx: &GpuContext, _width: u32, _height: u32) {}

    #[cfg(test)]
    fn debug_last_light_ambient(&self) -> Option<[f32; 4]> {
        None
    }
}

pub trait FrameFinalizeNode: Send {
    fn name(&self) -> &'static str;

    fn is_enabled(&self, _frame: &PreparedFrame<'_>) -> bool {
        true
    }

    fn setup(
        &mut self,
        graph: &mut RenderGraph,
        state: &mut FinalizePhaseState<'_>,
        frame: &PreparedFrame<'_>,
    );

    fn execute(
        &mut self,
        pass: &CompiledPass,
        ctx: &mut GpuContext,
        resources: &PhysicalResources<'_>,
        execution: &FinalizeExecutionContext<'_>,
    ) -> Result<(), RenderGraphError>;

    fn draw_calls(&self, _execution: &FinalizeExecutionContext<'_>) -> usize {
        0
    }

    fn resize(&mut self, _ctx: &GpuContext, _width: u32, _height: u32) {}
}
