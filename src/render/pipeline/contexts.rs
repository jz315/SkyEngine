use std::any::Any;

use crate::gpu::GpuContext;
use crate::render::execution::{
    CompletedViewState, FinalizeExecutionContext, FinalizePhaseState, PhaseState, PreparedFrame,
    PreparedView, ViewExecutionContext,
};
use crate::render::gpu::Texture;
use crate::render::graph::{CompiledPass, PhysicalResources, RenderGraph};
use crate::render::phase::DrawFunctionRegistry;
use crate::render::resources::material::MaterialRegistry;
use crate::render::resources::mesh::MeshRegistry;
use crate::render::view::SceneView;

pub struct ComputePassSetupContext<'graph, 'frame> {
    graph: &'graph mut RenderGraph,
    state: &'graph mut PhaseState,
    frame: &'frame PreparedFrame<'frame>,
    view: &'frame PreparedView<'frame>,
}

impl<'graph, 'frame> ComputePassSetupContext<'graph, 'frame> {
    #[inline]
    pub(crate) fn new(
        graph: &'graph mut RenderGraph,
        state: &'graph mut PhaseState,
        frame: &'frame PreparedFrame<'frame>,
        view: &'frame PreparedView<'frame>,
    ) -> Self {
        Self {
            graph,
            state,
            frame,
            view,
        }
    }

    #[inline]
    pub fn graph(&mut self) -> &mut RenderGraph {
        self.graph
    }

    #[inline]
    pub fn state(&mut self) -> &mut PhaseState {
        self.state
    }

    #[inline]
    pub fn frame(&self) -> &PreparedFrame<'frame> {
        self.frame
    }

    #[inline]
    pub fn view(&self) -> &PreparedView<'frame> {
        self.view
    }

    #[inline]
    pub fn frame_payload<T: Any>(&self) -> Option<&'frame T> {
        self.frame.payload::<T>()
    }

    #[inline]
    pub fn view_payload<T: Any>(&self) -> Option<&'frame T> {
        self.view.payload::<T>()
    }
}

pub struct ComputePassExecuteContext<'gpu, 'frame> {
    gpu: &'gpu mut GpuContext,
    pass: &'frame CompiledPass,
    resources: &'frame PhysicalResources<'frame>,
    execution: &'frame ViewExecutionContext<'frame>,
}

impl<'gpu, 'frame> ComputePassExecuteContext<'gpu, 'frame> {
    #[inline]
    pub(crate) fn new(
        gpu: &'gpu mut GpuContext,
        pass: &'frame CompiledPass,
        resources: &'frame PhysicalResources<'frame>,
        execution: &'frame ViewExecutionContext<'frame>,
    ) -> Self {
        Self {
            gpu,
            pass,
            resources,
            execution,
        }
    }

    #[inline]
    pub fn gpu(&mut self) -> &mut GpuContext {
        self.gpu
    }

    #[inline]
    pub fn pass(&self) -> &CompiledPass {
        self.pass
    }

    #[inline]
    pub fn resources(&self) -> &PhysicalResources<'frame> {
        self.resources
    }

    #[inline]
    pub fn frame(&self) -> &PreparedFrame<'frame> {
        self.execution.frame()
    }

    #[inline]
    pub fn view(&self) -> &PreparedView<'frame> {
        self.execution.view()
    }

    #[inline]
    pub fn view_index(&self) -> usize {
        self.execution.view_index()
    }

    #[inline]
    pub fn frame_payload<T: Any>(&self) -> Option<&'frame T> {
        self.execution.frame_payload::<T>()
    }

    #[inline]
    pub fn view_payload<T: Any>(&self) -> Option<&'frame T> {
        self.execution.view_payload::<T>()
    }

    #[inline]
    pub fn scene_view(&self) -> Option<&'frame SceneView> {
        self.execution.view_payload::<SceneView>()
    }

    #[inline]
    pub fn split(
        &mut self,
    ) -> (
        &mut GpuContext,
        &CompiledPass,
        &PhysicalResources<'frame>,
        &ViewExecutionContext<'frame>,
    ) {
        (self.gpu, self.pass, self.resources, self.execution)
    }
}

pub struct PostFxPassSetupContext<'graph, 'frame> {
    graph: &'graph mut RenderGraph,
    state: &'graph mut PhaseState,
    frame: &'frame PreparedFrame<'frame>,
    view: &'frame PreparedView<'frame>,
}

impl<'graph, 'frame> PostFxPassSetupContext<'graph, 'frame> {
    #[inline]
    pub(crate) fn new(
        graph: &'graph mut RenderGraph,
        state: &'graph mut PhaseState,
        frame: &'frame PreparedFrame<'frame>,
        view: &'frame PreparedView<'frame>,
    ) -> Self {
        Self {
            graph,
            state,
            frame,
            view,
        }
    }

    #[inline]
    pub fn graph(&mut self) -> &mut RenderGraph {
        self.graph
    }

    #[inline]
    pub fn state(&mut self) -> &mut PhaseState {
        self.state
    }

    #[inline]
    pub fn frame(&self) -> &PreparedFrame<'frame> {
        self.frame
    }

    #[inline]
    pub fn view(&self) -> &PreparedView<'frame> {
        self.view
    }

    #[inline]
    pub fn frame_payload<T: Any>(&self) -> Option<&'frame T> {
        self.frame.payload::<T>()
    }

    #[inline]
    pub fn view_payload<T: Any>(&self) -> Option<&'frame T> {
        self.view.payload::<T>()
    }
}

pub struct PostFxPassExecuteContext<'gpu, 'frame> {
    gpu: &'gpu mut GpuContext,
    pass: &'frame CompiledPass,
    resources: &'frame PhysicalResources<'frame>,
    execution: &'frame ViewExecutionContext<'frame>,
}

impl<'gpu, 'frame> PostFxPassExecuteContext<'gpu, 'frame> {
    #[inline]
    pub(crate) fn new(
        gpu: &'gpu mut GpuContext,
        pass: &'frame CompiledPass,
        resources: &'frame PhysicalResources<'frame>,
        execution: &'frame ViewExecutionContext<'frame>,
    ) -> Self {
        Self {
            gpu,
            pass,
            resources,
            execution,
        }
    }

    #[inline]
    pub fn gpu(&mut self) -> &mut GpuContext {
        self.gpu
    }

    #[inline]
    pub fn pass(&self) -> &CompiledPass {
        self.pass
    }

    #[inline]
    pub fn resources(&self) -> &PhysicalResources<'frame> {
        self.resources
    }

    #[inline]
    pub fn frame(&self) -> &PreparedFrame<'frame> {
        self.execution.frame()
    }

    #[inline]
    pub fn view(&self) -> &PreparedView<'frame> {
        self.execution.view()
    }

    #[inline]
    pub fn view_index(&self) -> usize {
        self.execution.view_index()
    }

    #[inline]
    pub fn frame_payload<T: Any>(&self) -> Option<&'frame T> {
        self.execution.frame_payload::<T>()
    }

    #[inline]
    pub fn view_payload<T: Any>(&self) -> Option<&'frame T> {
        self.execution.view_payload::<T>()
    }

    #[inline]
    pub fn scene_view(&self) -> Option<&'frame SceneView> {
        self.execution.view_payload::<SceneView>()
    }

    #[inline]
    pub fn split(
        &mut self,
    ) -> (
        &mut GpuContext,
        &CompiledPass,
        &PhysicalResources<'frame>,
        &ViewExecutionContext<'frame>,
    ) {
        (self.gpu, self.pass, self.resources, self.execution)
    }
}

pub struct RenderPassSetupContext<'graph, 'slots, 'frame> {
    graph: &'graph mut RenderGraph,
    state: &'graph mut FinalizePhaseState<'slots>,
    frame: &'frame PreparedFrame<'frame>,
}

impl<'graph, 'slots, 'frame> RenderPassSetupContext<'graph, 'slots, 'frame> {
    #[inline]
    pub(crate) fn new(
        graph: &'graph mut RenderGraph,
        state: &'graph mut FinalizePhaseState<'slots>,
        frame: &'frame PreparedFrame<'frame>,
    ) -> Self {
        Self {
            graph,
            state,
            frame,
        }
    }

    #[inline]
    pub fn graph(&mut self) -> &mut RenderGraph {
        self.graph
    }

    #[inline]
    pub fn state(&mut self) -> &mut FinalizePhaseState<'slots> {
        self.state
    }

    #[inline]
    pub fn frame(&self) -> &PreparedFrame<'frame> {
        self.frame
    }

    #[inline]
    pub fn payload<T: Any>(&self) -> Option<&'frame T> {
        self.frame.payload::<T>()
    }
}

pub struct RenderPassExecuteContext<'gpu, 'frame> {
    gpu: &'gpu mut GpuContext,
    pass: &'frame CompiledPass,
    resources: &'frame PhysicalResources<'frame>,
    execution: &'frame FinalizeExecutionContext<'frame>,
}

impl<'gpu, 'frame> RenderPassExecuteContext<'gpu, 'frame> {
    #[inline]
    pub(crate) fn new(
        gpu: &'gpu mut GpuContext,
        pass: &'frame CompiledPass,
        resources: &'frame PhysicalResources<'frame>,
        execution: &'frame FinalizeExecutionContext<'frame>,
    ) -> Self {
        Self {
            gpu,
            pass,
            resources,
            execution,
        }
    }

    #[inline]
    pub fn gpu(&mut self) -> &mut GpuContext {
        self.gpu
    }

    #[inline]
    pub fn pass(&self) -> &CompiledPass {
        self.pass
    }

    #[inline]
    pub fn resources(&self) -> &PhysicalResources<'frame> {
        self.resources
    }

    #[inline]
    pub fn frame(&self) -> &PreparedFrame<'frame> {
        self.execution.frame()
    }

    #[inline]
    pub fn completed_views(&self) -> &'frame [CompletedViewState] {
        self.execution.completed_views()
    }

    #[inline]
    pub fn payload<T: Any>(&self) -> Option<&'frame T> {
        self.execution.payload::<T>()
    }

    #[inline]
    pub fn split(
        &mut self,
    ) -> (
        &mut GpuContext,
        &CompiledPass,
        &PhysicalResources<'frame>,
        &FinalizeExecutionContext<'frame>,
    ) {
        (self.gpu, self.pass, self.resources, self.execution)
    }
}

pub struct RenderPhaseSetupContext<'graph, 'frame> {
    graph: &'graph mut RenderGraph,
    state: &'graph mut PhaseState,
    frame: &'frame PreparedFrame<'frame>,
    view: &'frame PreparedView<'frame>,
}

impl<'graph, 'frame> RenderPhaseSetupContext<'graph, 'frame> {
    #[inline]
    pub(crate) fn new(
        graph: &'graph mut RenderGraph,
        state: &'graph mut PhaseState,
        frame: &'frame PreparedFrame<'frame>,
        view: &'frame PreparedView<'frame>,
    ) -> Self {
        Self {
            graph,
            state,
            frame,
            view,
        }
    }

    #[inline]
    pub fn graph(&mut self) -> &mut RenderGraph {
        self.graph
    }

    #[inline]
    pub fn state(&mut self) -> &mut PhaseState {
        self.state
    }

    #[inline]
    pub(crate) fn graph_and_state(&mut self) -> (&mut RenderGraph, &mut PhaseState) {
        (self.graph, self.state)
    }

    #[inline]
    pub fn frame(&self) -> &PreparedFrame<'frame> {
        self.frame
    }

    #[inline]
    pub fn view(&self) -> &PreparedView<'frame> {
        self.view
    }

    #[inline]
    pub fn frame_payload<T: Any>(&self) -> Option<&'frame T> {
        self.frame.payload::<T>()
    }

    #[inline]
    pub fn view_payload<T: Any>(&self) -> Option<&'frame T> {
        self.view.payload::<T>()
    }
}

pub struct RenderPhaseExecuteContext<'gpu, 'frame, 'tex> {
    gpu: &'gpu mut GpuContext,
    pass: &'frame CompiledPass,
    resources: &'frame PhysicalResources<'frame>,
    execution: &'frame ViewExecutionContext<'frame>,
    draw_functions: &'gpu mut DrawFunctionRegistry,
    material_registry: &'gpu mut MaterialRegistry,
    mesh_registry: &'gpu MeshRegistry,
    fallback_texture: &'tex Texture,
}

impl<'gpu, 'frame, 'tex> RenderPhaseExecuteContext<'gpu, 'frame, 'tex> {
    #[inline]
    pub(crate) fn new(
        gpu: &'gpu mut GpuContext,
        pass: &'frame CompiledPass,
        resources: &'frame PhysicalResources<'frame>,
        execution: &'frame ViewExecutionContext<'frame>,
        draw_functions: &'gpu mut DrawFunctionRegistry,
        material_registry: &'gpu mut MaterialRegistry,
        mesh_registry: &'gpu MeshRegistry,
        fallback_texture: &'tex Texture,
    ) -> Self {
        Self {
            gpu,
            pass,
            resources,
            execution,
            draw_functions,
            material_registry,
            mesh_registry,
            fallback_texture,
        }
    }

    #[inline]
    pub fn gpu(&mut self) -> &mut GpuContext {
        self.gpu
    }

    #[inline]
    pub fn pass(&self) -> &CompiledPass {
        self.pass
    }

    #[inline]
    pub fn resources(&self) -> &PhysicalResources<'frame> {
        self.resources
    }

    #[inline]
    pub fn frame(&self) -> &PreparedFrame<'frame> {
        self.execution.frame()
    }

    #[inline]
    pub fn view(&self) -> &PreparedView<'frame> {
        self.execution.view()
    }

    #[inline]
    pub fn view_index(&self) -> usize {
        self.execution.view_index()
    }

    #[inline]
    pub fn frame_payload<T: Any>(&self) -> Option<&'frame T> {
        self.execution.frame_payload::<T>()
    }

    #[inline]
    pub fn view_payload<T: Any>(&self) -> Option<&'frame T> {
        self.execution.view_payload::<T>()
    }

    #[inline]
    pub fn scene_view(&self) -> Option<&'frame SceneView> {
        self.execution.view_payload::<SceneView>()
    }

    #[inline]
    pub fn draw_functions(&mut self) -> &mut DrawFunctionRegistry {
        self.draw_functions
    }

    #[inline]
    pub fn material_registry(&mut self) -> &mut MaterialRegistry {
        self.material_registry
    }

    #[inline]
    pub fn mesh_registry(&self) -> &MeshRegistry {
        self.mesh_registry
    }

    #[inline]
    pub fn fallback_texture(&self) -> &Texture {
        self.fallback_texture
    }

    #[inline]
    pub fn split(
        &mut self,
    ) -> (
        &mut GpuContext,
        &CompiledPass,
        &PhysicalResources<'frame>,
        &ViewExecutionContext<'frame>,
        &mut DrawFunctionRegistry,
        &mut MaterialRegistry,
        &MeshRegistry,
        &'tex Texture,
    ) {
        (
            self.gpu,
            self.pass,
            self.resources,
            self.execution,
            self.draw_functions,
            self.material_registry,
            self.mesh_registry,
            self.fallback_texture,
        )
    }
}
