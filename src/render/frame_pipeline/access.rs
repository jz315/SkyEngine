use std::any::Any;
use std::borrow::Cow;

use crate::gpu::GpuContext;
use crate::render::graph::{CompiledPass, PhysicalResources, RenderGraph, TextureHandle};

use super::nodes::ViewExecutionContext;
use super::payload::{PreparedFrame, PreparedView};
use super::slots::{PhaseState, TextureSlot};
use super::TextureFormat;

pub(crate) struct FrameContextSetupAccess<'a, 'frame> {
    graph: &'a mut RenderGraph,
    state: &'a mut PhaseState,
    frame: &'a PreparedFrame<'frame>,
    view: &'a PreparedView<'frame>,
}

impl<'a, 'frame> FrameContextSetupAccess<'a, 'frame> {
    #[inline]
    pub(crate) fn new(
        graph: &'a mut RenderGraph,
        state: &'a mut PhaseState,
        frame: &'a PreparedFrame<'frame>,
        view: &'a PreparedView<'frame>,
    ) -> Self {
        Self {
            graph,
            state,
            frame,
            view,
        }
    }

    #[inline]
    pub(crate) fn frame(&self) -> &PreparedFrame<'frame> {
        self.frame
    }

    #[inline]
    pub(crate) fn view(&self) -> &PreparedView<'frame> {
        self.view
    }

    #[inline]
    pub(crate) fn frame_payload<T: Any>(&self) -> Option<&'frame T> {
        self.frame.payload::<T>()
    }

    #[inline]
    pub(crate) fn view_payload<T: Any>(&self) -> Option<&'frame T> {
        self.view.payload::<T>()
    }

    #[inline]
    pub(crate) fn surface_format(&self) -> TextureFormat {
        self.state.surface_format()
    }

    #[inline]
    pub(crate) fn has_surface(&self) -> bool {
        self.state.has_surface()
    }

    #[inline]
    pub(crate) fn current_color(&self) -> Option<TextureSlot> {
        self.state.current_color()
    }

    #[inline]
    pub(crate) fn texture_slot(&self, name: &str) -> Option<TextureSlot> {
        self.state.texture_slot(name)
    }

    #[inline]
    pub(crate) fn set_current_color(&mut self, handle: TextureHandle, format: TextureFormat) {
        self.state.set_current_color(handle, format);
    }

    #[inline]
    pub(crate) fn set_texture_slot(
        &mut self,
        name: impl Into<Cow<'static, str>>,
        handle: TextureHandle,
        format: TextureFormat,
    ) -> Option<TextureSlot> {
        self.state.set_texture_slot(name, handle, format)
    }

    #[inline]
    pub(crate) fn graph_mut(&mut self) -> &mut RenderGraph {
        self.graph
    }
}

pub(crate) struct FrameContextExecuteAccess<'a, 'frame> {
    pass: &'a CompiledPass,
    gpu: &'a mut GpuContext,
    resources: &'a PhysicalResources<'frame>,
    execution: &'a ViewExecutionContext<'frame>,
}

impl<'a, 'frame> FrameContextExecuteAccess<'a, 'frame> {
    #[inline]
    pub(crate) fn new(
        pass: &'a CompiledPass,
        gpu: &'a mut GpuContext,
        resources: &'a PhysicalResources<'frame>,
        execution: &'a ViewExecutionContext<'frame>,
    ) -> Self {
        Self {
            pass,
            gpu,
            resources,
            execution,
        }
    }

    #[inline]
    pub(crate) fn pass(&self) -> &CompiledPass {
        self.pass
    }

    #[inline]
    pub(crate) fn gpu(&mut self) -> &mut GpuContext {
        self.gpu
    }

    #[inline]
    pub(crate) fn resources(&self) -> &PhysicalResources<'frame> {
        self.resources
    }

    #[inline]
    pub(crate) fn frame(&self) -> &PreparedFrame<'frame> {
        self.execution.frame()
    }

    #[inline]
    pub(crate) fn view(&self) -> &PreparedView<'frame> {
        self.execution.view()
    }

    #[inline]
    pub(crate) fn view_index(&self) -> usize {
        self.execution.view_index()
    }

    #[inline]
    pub(crate) fn frame_payload<T: Any>(&self) -> Option<&'frame T> {
        self.execution.frame_payload::<T>()
    }

    #[inline]
    pub(crate) fn view_payload<T: Any>(&self) -> Option<&'frame T> {
        self.execution.view_payload::<T>()
    }
}
