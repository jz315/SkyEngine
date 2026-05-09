use std::any::Any;

use crate::gpu::GpuContext;
use crate::render::component::RenderSettings;
use crate::render::execution::{
    CompletedViewState, FinalizeExecutionContext, FinalizePhaseState, PhaseState, PreparedFrame,
    PreparedView, SceneTexture, TextureFormat, TextureSlot, ViewExecutionContext,
};
use crate::render::gpu::{GpuScene, Texture};
use crate::render::graph::{
    CompiledPass, PhysicalResources, RenderGraph, ResourceRef, TargetSize, TextureHandle,
    TextureSubresource,
};
use crate::render::lighting::shadow::{
    SceneShadowResources, ShadowSceneBindingLayout, ShadowViewBinding,
};
use crate::render::lighting::{LightTable, SceneLightingResources};
use crate::render::phase::DrawFunctionRegistry;
use crate::render::resources::blackboard::Blackboard;
use crate::render::resources::material::MaterialRegistry;
use crate::render::resources::mesh::MeshRegistry;
use crate::render::runtime::{HistoryTextureRequest, HistoryTextureStore};
use crate::render::view::SceneView;

use super::resource_spec::TextureSpec;

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

    #[inline]
    pub fn scene_lighting(&self) -> Option<SceneLightingResources<'frame>> {
        scene_lighting_for_view(self.frame, self.view)
    }

    #[inline]
    pub fn optional_scene_shadows(&self) -> Option<SceneShadowResources> {
        self.state
            .scene_shadows()
            .cloned()
            .or_else(|| scene_shadows_for_view(self.frame, self.view))
    }

    #[inline]
    pub fn require_scene_shadows(&self) -> SceneShadowResources {
        require_scene_shadows(self.optional_scene_shadows())
    }

    #[inline]
    pub fn publish_scene_shadows(
        &mut self,
        resources: SceneShadowResources,
    ) -> Option<SceneShadowResources> {
        self.state.set_scene_shadows(resources)
    }

    pub fn blackboard(&mut self) -> &mut Blackboard {
        self.graph.blackboard()
    }

    #[inline]
    pub fn blackboard_ref(&self) -> &Blackboard {
        self.graph.blackboard_ref()
    }

    #[inline]
    pub fn blackboard_set<T: Any>(
        &mut self,
        name: impl Into<std::borrow::Cow<'static, str>>,
        value: T,
    ) {
        self.graph.blackboard().set(name, value);
    }

    #[inline]
    pub fn blackboard_get<T: Any>(&self, name: &str) -> Option<&T> {
        self.graph.blackboard_ref().get::<T>(name)
    }

    #[inline]
    pub fn blackboard_get_mut<T: Any>(&mut self, name: &str) -> Option<&mut T> {
        self.graph.blackboard().get_mut::<T>(name)
    }

    #[inline]
    pub fn optional_scene_texture(&self, texture: SceneTexture) -> Option<TextureSlot> {
        self.state.scene_texture(texture)
    }

    #[inline]
    pub fn require_scene_texture(&self, texture: SceneTexture) -> TextureSlot {
        require_scene_texture_slot(self.optional_scene_texture(texture), texture)
    }

    #[inline]
    pub fn set_scene_texture(
        &mut self,
        texture: SceneTexture,
        handle: TextureHandle,
        format: TextureFormat,
    ) -> TextureSlot {
        set_phase_scene_texture(self.state, texture, handle, format)
    }

    #[inline]
    pub fn ensure_scene_texture(
        &mut self,
        texture: SceneTexture,
        format: TextureFormat,
    ) -> TextureSlot {
        ensure_phase_scene_texture(
            self.graph,
            self.state,
            self.view.target_size(),
            texture,
            format,
        )
    }

    #[inline]
    pub fn create_texture(&mut self, spec: TextureSpec) -> TextureSlot {
        create_texture_from_spec(self.graph, spec)
    }

    #[inline]
    pub fn history_texture(
        &mut self,
        name: impl Into<std::borrow::Cow<'static, str>>,
    ) -> HistoryTextureRequest<'_> {
        let history = self
            .frame
            .payload::<HistoryTextureStore>()
            .expect("history_texture requires the renderer history store frame payload");
        history.request(
            self.graph,
            self.view.history_key(),
            self.view.target_size(),
            name,
        )
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
    pub fn read_texture(&self, index: usize) -> TextureHandle {
        nth_texture(&self.pass.reads, index, "read")
    }

    #[inline]
    pub fn write_texture(&self, index: usize) -> TextureHandle {
        nth_texture(&self.pass.writes, index, "write")
    }

    #[inline]
    pub fn read_subresource(&self, index: usize) -> TextureSubresource {
        nth_subresource(&self.pass.reads, index, "read")
    }

    #[inline]
    pub fn write_subresource(&self, index: usize) -> TextureSubresource {
        nth_subresource(&self.pass.writes, index, "write")
    }

    #[inline]
    pub fn dispatch_2d(
        &self,
        pass: &mut wgpu::ComputePass<'_>,
        width: u32,
        height: u32,
        block_size: u32,
    ) {
        if width == 0 || height == 0 {
            return;
        }
        let block_size = block_size.max(1);
        pass.dispatch_workgroups(width.div_ceil(block_size), height.div_ceil(block_size), 1);
    }

    #[inline]
    pub fn dispatch_3d(
        &self,
        pass: &mut wgpu::ComputePass<'_>,
        width: u32,
        height: u32,
        depth: u32,
        block_size: [u32; 3],
    ) {
        if width == 0 || height == 0 || depth == 0 {
            return;
        }
        let [block_x, block_y, block_z] = block_size;
        pass.dispatch_workgroups(
            width.div_ceil(block_x.max(1)),
            height.div_ceil(block_y.max(1)),
            depth.div_ceil(block_z.max(1)),
        );
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
    pub fn scene_lighting(&self) -> Option<SceneLightingResources<'frame>> {
        scene_lighting_for_view(self.execution.frame(), self.execution.view())
    }

    #[inline]
    pub fn optional_scene_shadows(&self) -> Option<SceneShadowResources> {
        self.execution
            .view_state()
            .scene_shadows()
            .cloned()
            .or_else(|| scene_shadows_for_view(self.execution.frame(), self.execution.view()))
    }

    #[inline]
    pub fn require_scene_shadows(&self) -> SceneShadowResources {
        require_scene_shadows(self.optional_scene_shadows())
    }

    #[inline]
    pub fn blackboard(&self) -> &Blackboard {
        self.resources.blackboard()
    }

    #[inline]
    pub fn blackboard_get<T: Any>(&self, name: &str) -> Option<&T> {
        self.resources.blackboard_get::<T>(name)
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

    #[inline]
    pub fn execution(&self) -> &'frame ViewExecutionContext<'frame> {
        self.execution
    }
}

pub struct GraphPassSetupContext<'graph, 'frame> {
    graph: &'graph mut RenderGraph,
    state: &'graph mut PhaseState,
    frame: &'frame PreparedFrame<'frame>,
    view: &'frame PreparedView<'frame>,
}

impl<'graph, 'frame> GraphPassSetupContext<'graph, 'frame> {
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

    #[inline]
    pub fn scene_lighting(&self) -> Option<SceneLightingResources<'frame>> {
        scene_lighting_for_view(self.frame, self.view)
    }

    #[inline]
    pub fn optional_scene_shadows(&self) -> Option<SceneShadowResources> {
        self.state
            .scene_shadows()
            .cloned()
            .or_else(|| scene_shadows_for_view(self.frame, self.view))
    }

    #[inline]
    pub fn require_scene_shadows(&self) -> SceneShadowResources {
        require_scene_shadows(self.optional_scene_shadows())
    }

    #[inline]
    pub fn publish_scene_shadows(
        &mut self,
        resources: SceneShadowResources,
    ) -> Option<SceneShadowResources> {
        self.state.set_scene_shadows(resources)
    }

    pub fn blackboard(&mut self) -> &mut Blackboard {
        self.graph.blackboard()
    }

    #[inline]
    pub fn blackboard_ref(&self) -> &Blackboard {
        self.graph.blackboard_ref()
    }

    #[inline]
    pub fn blackboard_set<T: Any>(
        &mut self,
        name: impl Into<std::borrow::Cow<'static, str>>,
        value: T,
    ) {
        self.graph.blackboard().set(name, value);
    }

    #[inline]
    pub fn blackboard_get<T: Any>(&self, name: &str) -> Option<&T> {
        self.graph.blackboard_ref().get::<T>(name)
    }

    #[inline]
    pub fn blackboard_get_mut<T: Any>(&mut self, name: &str) -> Option<&mut T> {
        self.graph.blackboard().get_mut::<T>(name)
    }

    #[inline]
    pub fn optional_scene_texture(&self, texture: SceneTexture) -> Option<TextureSlot> {
        self.state.scene_texture(texture)
    }

    #[inline]
    pub fn require_scene_texture(&self, texture: SceneTexture) -> TextureSlot {
        require_scene_texture_slot(self.optional_scene_texture(texture), texture)
    }

    #[inline]
    pub fn set_scene_texture(
        &mut self,
        texture: SceneTexture,
        handle: TextureHandle,
        format: TextureFormat,
    ) -> TextureSlot {
        set_phase_scene_texture(self.state, texture, handle, format)
    }

    #[inline]
    pub fn ensure_scene_texture(
        &mut self,
        texture: SceneTexture,
        format: TextureFormat,
    ) -> TextureSlot {
        ensure_phase_scene_texture(
            self.graph,
            self.state,
            self.view.target_size(),
            texture,
            format,
        )
    }

    #[inline]
    pub fn create_texture(&mut self, spec: TextureSpec) -> TextureSlot {
        create_texture_from_spec(self.graph, spec)
    }

    #[inline]
    pub fn history_texture(
        &mut self,
        name: impl Into<std::borrow::Cow<'static, str>>,
    ) -> HistoryTextureRequest<'_> {
        let history = self
            .frame
            .payload::<HistoryTextureStore>()
            .expect("history_texture requires the renderer history store frame payload");
        history.request(
            self.graph,
            self.view.history_key(),
            self.view.target_size(),
            name,
        )
    }
}

pub struct GraphPassExecuteContext<'gpu, 'frame> {
    gpu: &'gpu mut GpuContext,
    pass: &'frame CompiledPass,
    resources: &'frame PhysicalResources<'frame>,
    execution: &'frame ViewExecutionContext<'frame>,
}

impl<'gpu, 'frame> GraphPassExecuteContext<'gpu, 'frame> {
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
    pub fn read_texture(&self, index: usize) -> TextureHandle {
        nth_texture(&self.pass.reads, index, "read")
    }

    #[inline]
    pub fn write_texture(&self, index: usize) -> TextureHandle {
        nth_texture(&self.pass.writes, index, "write")
    }

    #[inline]
    pub fn read_subresource(&self, index: usize) -> TextureSubresource {
        nth_subresource(&self.pass.reads, index, "read")
    }

    #[inline]
    pub fn write_subresource(&self, index: usize) -> TextureSubresource {
        nth_subresource(&self.pass.writes, index, "write")
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
    pub fn scene_lighting(&self) -> Option<SceneLightingResources<'frame>> {
        scene_lighting_for_view(self.execution.frame(), self.execution.view())
    }

    #[inline]
    pub fn optional_scene_shadows(&self) -> Option<SceneShadowResources> {
        self.execution
            .view_state()
            .scene_shadows()
            .cloned()
            .or_else(|| scene_shadows_for_view(self.execution.frame(), self.execution.view()))
    }

    #[inline]
    pub fn require_scene_shadows(&self) -> SceneShadowResources {
        require_scene_shadows(self.optional_scene_shadows())
    }

    #[inline]
    pub fn blackboard(&self) -> &Blackboard {
        self.resources.blackboard()
    }

    #[inline]
    pub fn blackboard_get<T: Any>(&self, name: &str) -> Option<&T> {
        self.resources.blackboard_get::<T>(name)
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

    #[inline]
    pub fn execution(&self) -> &'frame ViewExecutionContext<'frame> {
        self.execution
    }
}

fn nth_texture(resources: &[ResourceRef], index: usize, access: &str) -> TextureHandle {
    resources
        .iter()
        .filter_map(|resource| match resource {
            ResourceRef::Texture(handle) => Some(*handle),
            _ => None,
        })
        .nth(index)
        .unwrap_or_else(|| panic!("compute pass should have {access} texture at index {index}"))
}

fn nth_subresource(resources: &[ResourceRef], index: usize, access: &str) -> TextureSubresource {
    resources
        .iter()
        .filter_map(|resource| match resource {
            ResourceRef::TextureSubresource(subresource) => Some(*subresource),
            _ => None,
        })
        .nth(index)
        .unwrap_or_else(|| panic!("compute pass should have {access} subresource at index {index}"))
}

#[inline]
fn require_scene_texture_slot(slot: Option<TextureSlot>, texture: SceneTexture) -> TextureSlot {
    slot.unwrap_or_else(|| panic!("{} texture is required", texture.label()))
}

#[inline]
fn create_scene_texture(
    graph: &mut RenderGraph,
    target_size: [u32; 2],
    texture: SceneTexture,
    format: TextureFormat,
) -> TextureHandle {
    graph.create_texture(|builder| {
        builder
            .name(texture.debug_name())
            .size(TargetSize::Exact(target_size[0], target_size[1]))
            .format(format);
    })
}

#[inline]
fn set_phase_scene_texture(
    state: &mut PhaseState,
    texture: SceneTexture,
    handle: TextureHandle,
    format: TextureFormat,
) -> TextureSlot {
    let _ = state.set_scene_texture(texture, handle, format);
    TextureSlot::new(handle, format)
}

#[inline]
fn set_finalize_scene_texture(
    state: &mut FinalizePhaseState<'_>,
    texture: SceneTexture,
    handle: TextureHandle,
    format: TextureFormat,
) -> TextureSlot {
    let _ = state.set_scene_texture(texture, handle, format);
    TextureSlot::new(handle, format)
}

#[inline]
fn ensure_phase_scene_texture(
    graph: &mut RenderGraph,
    state: &mut PhaseState,
    target_size: [u32; 2],
    texture: SceneTexture,
    format: TextureFormat,
) -> TextureSlot {
    state.scene_texture(texture).unwrap_or_else(|| {
        let handle = create_scene_texture(graph, target_size, texture, format);
        set_phase_scene_texture(state, texture, handle, format)
    })
}

#[inline]
fn ensure_finalize_scene_texture(
    graph: &mut RenderGraph,
    state: &mut FinalizePhaseState<'_>,
    target_size: [u32; 2],
    texture: SceneTexture,
    format: TextureFormat,
) -> TextureSlot {
    state.scene_texture(texture).unwrap_or_else(|| {
        let handle = create_scene_texture(graph, target_size, texture, format);
        set_finalize_scene_texture(state, texture, handle, format)
    })
}

#[inline]
fn create_texture_from_spec(graph: &mut RenderGraph, spec: TextureSpec) -> TextureSlot {
    spec.create_slot(graph)
}

fn scene_lighting_for_view<'frame>(
    frame: &'frame PreparedFrame<'frame>,
    view: &'frame PreparedView<'frame>,
) -> Option<SceneLightingResources<'frame>> {
    let gpu_scene = frame.payload::<GpuScene>()?;
    let settings = frame
        .payload::<RenderSettings>()
        .cloned()
        .unwrap_or_default();
    let mut lighting = SceneLightingResources::new(
        gpu_scene.table::<LightTable>(),
        settings.ambient_color.to_array(),
    );
    if let (Some(shadow_view), Some(scene_layout)) = (
        view.payload::<ShadowViewBinding>(),
        frame.payload::<ShadowSceneBindingLayout>(),
    ) {
        lighting = lighting.with_scene_bind_group(
            shadow_view.bind_group(),
            scene_layout.bind_group_layout(),
            shadow_view.enabled(),
        );
    }
    Some(lighting)
}

#[inline]
fn scene_shadows_for_view<'frame>(
    frame: &'frame PreparedFrame<'frame>,
    view: &'frame PreparedView<'frame>,
) -> Option<SceneShadowResources> {
    let shadow_view = view.payload::<ShadowViewBinding>()?;
    let scene_layout = frame.payload::<ShadowSceneBindingLayout>()?;
    Some(SceneShadowResources::from_directional_shadow(
        scene_layout,
        shadow_view,
    ))
}

#[inline]
fn require_scene_shadows(shadows: Option<SceneShadowResources>) -> SceneShadowResources {
    shadows.expect("scene shadow resources are required")
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

    #[inline]
    pub fn scene_lighting(&self) -> Option<SceneLightingResources<'frame>> {
        scene_lighting_for_view(self.frame, self.view)
    }

    #[inline]
    pub fn optional_scene_shadows(&self) -> Option<SceneShadowResources> {
        self.state
            .scene_shadows()
            .cloned()
            .or_else(|| scene_shadows_for_view(self.frame, self.view))
    }

    #[inline]
    pub fn require_scene_shadows(&self) -> SceneShadowResources {
        require_scene_shadows(self.optional_scene_shadows())
    }

    #[inline]
    pub fn publish_scene_shadows(
        &mut self,
        resources: SceneShadowResources,
    ) -> Option<SceneShadowResources> {
        self.state.set_scene_shadows(resources)
    }

    pub fn blackboard(&mut self) -> &mut Blackboard {
        self.graph.blackboard()
    }

    #[inline]
    pub fn blackboard_ref(&self) -> &Blackboard {
        self.graph.blackboard_ref()
    }

    #[inline]
    pub fn blackboard_set<T: Any>(
        &mut self,
        name: impl Into<std::borrow::Cow<'static, str>>,
        value: T,
    ) {
        self.graph.blackboard().set(name, value);
    }

    #[inline]
    pub fn blackboard_get<T: Any>(&self, name: &str) -> Option<&T> {
        self.graph.blackboard_ref().get::<T>(name)
    }

    #[inline]
    pub fn blackboard_get_mut<T: Any>(&mut self, name: &str) -> Option<&mut T> {
        self.graph.blackboard().get_mut::<T>(name)
    }

    #[inline]
    pub fn optional_scene_texture(&self, texture: SceneTexture) -> Option<TextureSlot> {
        self.state.scene_texture(texture)
    }

    #[inline]
    pub fn require_scene_texture(&self, texture: SceneTexture) -> TextureSlot {
        require_scene_texture_slot(self.optional_scene_texture(texture), texture)
    }

    #[inline]
    pub fn set_scene_texture(
        &mut self,
        texture: SceneTexture,
        handle: TextureHandle,
        format: TextureFormat,
    ) -> TextureSlot {
        set_phase_scene_texture(self.state, texture, handle, format)
    }

    #[inline]
    pub fn ensure_scene_texture(
        &mut self,
        texture: SceneTexture,
        format: TextureFormat,
    ) -> TextureSlot {
        ensure_phase_scene_texture(
            self.graph,
            self.state,
            self.view.target_size(),
            texture,
            format,
        )
    }

    #[inline]
    pub fn create_texture(&mut self, spec: TextureSpec) -> TextureSlot {
        create_texture_from_spec(self.graph, spec)
    }

    #[inline]
    pub fn history_texture(
        &mut self,
        name: impl Into<std::borrow::Cow<'static, str>>,
    ) -> HistoryTextureRequest<'_> {
        let history = self
            .frame
            .payload::<HistoryTextureStore>()
            .expect("history_texture requires the renderer history store frame payload");
        history.request(
            self.graph,
            self.view.history_key(),
            self.view.target_size(),
            name,
        )
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
    pub fn scene_lighting(&self) -> Option<SceneLightingResources<'frame>> {
        scene_lighting_for_view(self.execution.frame(), self.execution.view())
    }

    #[inline]
    pub fn optional_scene_shadows(&self) -> Option<SceneShadowResources> {
        self.execution
            .view_state()
            .scene_shadows()
            .cloned()
            .or_else(|| scene_shadows_for_view(self.execution.frame(), self.execution.view()))
    }

    #[inline]
    pub fn require_scene_shadows(&self) -> SceneShadowResources {
        require_scene_shadows(self.optional_scene_shadows())
    }

    #[inline]
    pub fn blackboard(&self) -> &Blackboard {
        self.resources.blackboard()
    }

    #[inline]
    pub fn blackboard_get<T: Any>(&self, name: &str) -> Option<&T> {
        self.resources.blackboard_get::<T>(name)
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

    #[inline]
    pub fn execution(&self) -> &'frame ViewExecutionContext<'frame> {
        self.execution
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

    #[inline]
    pub fn blackboard(&mut self) -> &mut Blackboard {
        self.graph.blackboard()
    }

    #[inline]
    pub fn blackboard_ref(&self) -> &Blackboard {
        self.graph.blackboard_ref()
    }

    #[inline]
    pub fn blackboard_set<T: Any>(
        &mut self,
        name: impl Into<std::borrow::Cow<'static, str>>,
        value: T,
    ) {
        self.graph.blackboard().set(name, value);
    }

    #[inline]
    pub fn blackboard_get<T: Any>(&self, name: &str) -> Option<&T> {
        self.graph.blackboard_ref().get::<T>(name)
    }

    #[inline]
    pub fn blackboard_get_mut<T: Any>(&mut self, name: &str) -> Option<&mut T> {
        self.graph.blackboard().get_mut::<T>(name)
    }

    #[inline]
    pub fn optional_scene_texture(&self, texture: SceneTexture) -> Option<TextureSlot> {
        self.state.scene_texture(texture)
    }

    #[inline]
    pub fn require_scene_texture(&self, texture: SceneTexture) -> TextureSlot {
        require_scene_texture_slot(self.optional_scene_texture(texture), texture)
    }

    #[inline]
    pub fn set_scene_texture(
        &mut self,
        texture: SceneTexture,
        handle: TextureHandle,
        format: TextureFormat,
    ) -> TextureSlot {
        set_finalize_scene_texture(self.state, texture, handle, format)
    }

    #[inline]
    pub fn ensure_scene_texture(
        &mut self,
        texture: SceneTexture,
        target_size: [u32; 2],
        format: TextureFormat,
    ) -> TextureSlot {
        ensure_finalize_scene_texture(self.graph, self.state, target_size, texture, format)
    }

    #[inline]
    pub fn create_texture(&mut self, spec: TextureSpec) -> TextureSlot {
        create_texture_from_spec(self.graph, spec)
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
    pub fn blackboard(&self) -> &Blackboard {
        self.resources.blackboard()
    }

    #[inline]
    pub fn blackboard_get<T: Any>(&self, name: &str) -> Option<&T> {
        self.resources.blackboard_get::<T>(name)
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

    #[inline]
    pub fn scene_lighting(&self) -> Option<SceneLightingResources<'frame>> {
        scene_lighting_for_view(self.frame, self.view)
    }

    #[inline]
    pub fn optional_scene_shadows(&self) -> Option<SceneShadowResources> {
        self.state
            .scene_shadows()
            .cloned()
            .or_else(|| scene_shadows_for_view(self.frame, self.view))
    }

    #[inline]
    pub fn require_scene_shadows(&self) -> SceneShadowResources {
        require_scene_shadows(self.optional_scene_shadows())
    }

    #[inline]
    pub fn publish_scene_shadows(
        &mut self,
        resources: SceneShadowResources,
    ) -> Option<SceneShadowResources> {
        self.state.set_scene_shadows(resources)
    }

    #[inline]
    pub fn blackboard(&mut self) -> &mut Blackboard {
        self.graph.blackboard()
    }

    #[inline]
    pub fn blackboard_ref(&self) -> &Blackboard {
        self.graph.blackboard_ref()
    }

    #[inline]
    pub fn blackboard_set<T: Any>(
        &mut self,
        name: impl Into<std::borrow::Cow<'static, str>>,
        value: T,
    ) {
        self.graph.blackboard().set(name, value);
    }

    #[inline]
    pub fn blackboard_get<T: Any>(&self, name: &str) -> Option<&T> {
        self.graph.blackboard_ref().get::<T>(name)
    }

    #[inline]
    pub fn blackboard_get_mut<T: Any>(&mut self, name: &str) -> Option<&mut T> {
        self.graph.blackboard().get_mut::<T>(name)
    }

    #[inline]
    pub fn optional_scene_texture(&self, texture: SceneTexture) -> Option<TextureSlot> {
        self.state.scene_texture(texture)
    }

    #[inline]
    pub fn require_scene_texture(&self, texture: SceneTexture) -> TextureSlot {
        require_scene_texture_slot(self.optional_scene_texture(texture), texture)
    }

    #[inline]
    pub fn set_scene_texture(
        &mut self,
        texture: SceneTexture,
        handle: TextureHandle,
        format: TextureFormat,
    ) -> TextureSlot {
        set_phase_scene_texture(self.state, texture, handle, format)
    }

    #[inline]
    pub fn ensure_scene_texture(
        &mut self,
        texture: SceneTexture,
        format: TextureFormat,
    ) -> TextureSlot {
        ensure_phase_scene_texture(
            self.graph,
            self.state,
            self.view.target_size(),
            texture,
            format,
        )
    }

    #[inline]
    pub fn create_texture(&mut self, spec: TextureSpec) -> TextureSlot {
        create_texture_from_spec(self.graph, spec)
    }

    #[inline]
    pub fn history_texture(
        &mut self,
        name: impl Into<std::borrow::Cow<'static, str>>,
    ) -> HistoryTextureRequest<'_> {
        let history = self
            .frame
            .payload::<HistoryTextureStore>()
            .expect("history_texture requires the renderer history store frame payload");
        history.request(
            self.graph,
            self.view.history_key(),
            self.view.target_size(),
            name,
        )
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
    pub fn scene_lighting(&self) -> Option<SceneLightingResources<'frame>> {
        scene_lighting_for_view(self.execution.frame(), self.execution.view())
    }

    #[inline]
    pub fn optional_scene_shadows(&self) -> Option<SceneShadowResources> {
        self.execution
            .view_state()
            .scene_shadows()
            .cloned()
            .or_else(|| scene_shadows_for_view(self.execution.frame(), self.execution.view()))
    }

    #[inline]
    pub fn require_scene_shadows(&self) -> SceneShadowResources {
        require_scene_shadows(self.optional_scene_shadows())
    }

    #[inline]
    pub fn blackboard(&self) -> &Blackboard {
        self.resources.blackboard()
    }

    #[inline]
    pub fn blackboard_get<T: Any>(&self, name: &str) -> Option<&T> {
        self.resources.blackboard_get::<T>(name)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::execution::TextureFormat;
    use crate::render::graph::{PassFlags, PassHandle, PassType, ResourceRef};
    use crate::render::pipeline::GraphPass;
    use rustc_hash::FxHashMap;
    use std::borrow::Cow;

    fn create_test_device() -> (wgpu::Device, wgpu::Queue) {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .expect("No suitable GPU adapter found for render tests");

        pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("compute_context_test_device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
            },
            None,
        ))
        .expect("Failed to create test GPU device")
    }

    fn completed_view_state(
        view: &PreparedView<'_>,
        scene_shadows: Option<SceneShadowResources>,
    ) -> CompletedViewState {
        CompletedViewState::new(
            0,
            view,
            Default::default(),
            Default::default(),
            scene_shadows,
        )
    }

    #[test]
    fn compute_context_resource_helpers_resolve_declared_resources() {
        let (device, queue) = create_test_device();
        let mut gpu =
            GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [4, 4]);
        let read_texture = TextureHandle(0, 7);
        let write_texture = TextureHandle(1, 7);
        let read_subresource = TextureSubresource::new(TextureHandle(2, 7), 1, 1, 3, 1);
        let write_subresource = TextureSubresource::new(TextureHandle(3, 7), 2, 1, 4, 1);
        let pass = CompiledPass {
            handle: PassHandle(0, 7),
            index: 0,
            name: Cow::Borrowed("compute"),
            pass_type: PassType::Compute,
            reads: vec![
                ResourceRef::Texture(read_texture),
                ResourceRef::TextureSubresource(read_subresource),
            ],
            writes: vec![
                ResourceRef::Texture(write_texture),
                ResourceRef::TextureSubresource(write_subresource),
            ],
            color_outputs: Vec::new(),
            depth_stencil: None,
            copy_ops: Vec::new(),
            flags: PassFlags::empty(),
            dep_level: 0,
        };
        let frame = PreparedFrame::new(TextureFormat::Bgra8Unorm, false);
        let view = PreparedView::new(0, Default::default(), [4, 4], false);
        let view_state = completed_view_state(&view, None);
        let execution = ViewExecutionContext {
            frame: &frame,
            view: &view,
            view_state: &view_state,
            view_index: 0,
        };
        let textures = [];
        let buffers = [];
        let texture_descs = [];
        let buffer_descs = [];
        let alias_redirects = FxHashMap::default();
        let blackboard = Blackboard::new();
        let resources = PhysicalResources {
            handle_token: 7,
            textures: &textures,
            buffers: &buffers,
            texture_descs: &texture_descs,
            buffer_descs: &buffer_descs,
            alias_redirects: &alias_redirects,
            blackboard: &blackboard,
        };
        let ctx = ComputePassExecuteContext::new(&mut gpu, &pass, &resources, &execution);

        assert_eq!(ctx.read_texture(0), read_texture);
        assert_eq!(ctx.write_texture(0), write_texture);
        assert_eq!(ctx.read_subresource(0), read_subresource);
        assert_eq!(ctx.write_subresource(0), write_subresource);
    }

    #[test]
    fn custom_pass_can_require_scene_depth() {
        let mut graph = RenderGraph::new();
        let mut state = PhaseState::new(TextureFormat::Bgra8Unorm, false);
        let frame = PreparedFrame::new(TextureFormat::Bgra8Unorm, false);
        let view = PreparedView::new(0, Default::default(), [8, 8], false);
        let depth = graph.create_texture(|builder| {
            builder
                .name("test_scene_depth")
                .size(crate::render::graph::TargetSize::Exact(8, 8))
                .format(TextureFormat::Depth32Float);
        });
        let _ = state.set_scene_texture(SceneTexture::Depth, depth, TextureFormat::Depth32Float);

        let ctx = RenderPhaseSetupContext::new(&mut graph, &mut state, &frame, &view);

        assert_eq!(
            ctx.require_scene_texture(SceneTexture::Depth).handle(),
            depth
        );
        assert_eq!(ctx.optional_scene_texture(SceneTexture::Normal), None);
    }

    #[test]
    fn custom_pass_can_publish_indirect_diffuse() {
        let mut graph = RenderGraph::new();
        let mut state = PhaseState::new(TextureFormat::Bgra8Unorm, false);
        let frame = PreparedFrame::new(TextureFormat::Bgra8Unorm, false);
        let view = PreparedView::new(0, Default::default(), [16, 12], false);
        let mut ctx = ComputePassSetupContext::new(&mut graph, &mut state, &frame, &view);

        let indirect =
            ctx.ensure_scene_texture(SceneTexture::IndirectDiffuse, TextureFormat::Rgba16Float);
        let reused =
            ctx.ensure_scene_texture(SceneTexture::IndirectDiffuse, TextureFormat::Rgba16Float);

        assert_eq!(indirect, reused);
        assert_eq!(
            ctx.require_scene_texture(SceneTexture::IndirectDiffuse),
            indirect
        );
        assert_eq!(
            ctx.state()
                .texture_slot(SceneTexture::IndirectDiffuse.debug_name()),
            None
        );
    }

    #[test]
    fn custom_pass_can_publish_disabled_scene_shadows() {
        let mut graph = RenderGraph::new();
        let mut state = PhaseState::new(TextureFormat::Bgra8Unorm, false);
        let frame = PreparedFrame::new(TextureFormat::Bgra8Unorm, false);
        let view = PreparedView::new(0, Default::default(), [8, 8], false);
        let mut ctx = ComputePassSetupContext::new(&mut graph, &mut state, &frame, &view);

        assert!(ctx.optional_scene_shadows().is_none());
        let previous = ctx.publish_scene_shadows(SceneShadowResources::disabled(
            crate::render::lighting::ShadowResourceKind::DirectionalCascades,
        ));

        assert!(previous.is_none());
        let shadows = ctx.require_scene_shadows();
        assert_eq!(
            shadows.kind(),
            crate::render::lighting::ShadowResourceKind::DirectionalCascades
        );
        assert!(!shadows.enabled());
        assert!(shadows.bind_group().is_none());
        assert!(shadows.bind_group_layout().is_none());
        assert!(state.scene_shadows().is_some());
    }

    #[test]
    fn custom_shadow_resources_flow_to_execute_context() {
        let (device, queue) = create_test_device();
        let mut gpu =
            GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [8, 8]);
        let layout = gpu
            .device()
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("custom_shadow_bgl"),
                entries: &[],
            });
        let bind_group = gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("custom_shadow_bg"),
            layout: &layout,
            entries: &[],
        });
        let mut graph = RenderGraph::new();
        let mut state = PhaseState::new(TextureFormat::Bgra8Unorm, false);
        let frame = PreparedFrame::new(TextureFormat::Bgra8Unorm, false);
        let view = PreparedView::new(0, Default::default(), [8, 8], false);
        {
            let mut setup = GraphPassSetupContext::new(&mut graph, &mut state, &frame, &view);
            let previous = setup.publish_scene_shadows(SceneShadowResources::from_bind_group(
                crate::render::lighting::ShadowResourceKind::DirectionalCascades,
                &layout,
                &bind_group,
            ));
            assert!(previous.is_none());
        }
        let view_state = completed_view_state(&view, state.scene_shadows().cloned());
        let pass = CompiledPass {
            handle: PassHandle(0, 7),
            index: 0,
            name: Cow::Borrowed("custom_shadow_consumer"),
            pass_type: PassType::Render,
            reads: Vec::new(),
            writes: Vec::new(),
            color_outputs: Vec::new(),
            depth_stencil: None,
            copy_ops: Vec::new(),
            flags: PassFlags::empty(),
            dep_level: 0,
        };
        let execution = ViewExecutionContext {
            frame: &frame,
            view: &view,
            view_state: &view_state,
            view_index: 0,
        };
        let textures = [];
        let buffers = [];
        let texture_descs = [];
        let buffer_descs = [];
        let alias_redirects = FxHashMap::default();
        let resources = PhysicalResources {
            handle_token: 7,
            textures: &textures,
            buffers: &buffers,
            texture_descs: &texture_descs,
            buffer_descs: &buffer_descs,
            alias_redirects: &alias_redirects,
            blackboard: graph.blackboard_ref(),
        };
        let consumer = GraphPassExecuteContext::new(&mut gpu, &pass, &resources, &execution);

        let shadows = consumer.require_scene_shadows();
        assert_eq!(
            shadows.kind(),
            crate::render::lighting::ShadowResourceKind::DirectionalCascades
        );
        assert!(shadows.enabled());
        assert_eq!(shadows.bind_group_layout(), Some(&layout));
        assert_eq!(shadows.bind_group(), Some(&bind_group));
    }

    #[test]
    fn scene_texture_requests_do_not_use_generic_slot_map() {
        let mut graph = RenderGraph::new();
        let mut state = PhaseState::new(TextureFormat::Bgra8Unorm, false);
        let frame = PreparedFrame::new(TextureFormat::Bgra8Unorm, false);
        let view = PreparedView::new(0, Default::default(), [4, 4], false);
        let light = graph.create_texture(|builder| {
            builder
                .name("test_scene_light")
                .size(crate::render::graph::TargetSize::Exact(4, 4))
                .format(TextureFormat::Rgba16Float);
        });
        let mut ctx = PostFxPassSetupContext::new(&mut graph, &mut state, &frame, &view);

        let slot = ctx.set_scene_texture(SceneTexture::Light, light, TextureFormat::Rgba16Float);

        assert_eq!(ctx.optional_scene_texture(SceneTexture::Light), Some(slot));
        assert_eq!(
            ctx.state().texture_slot(SceneTexture::Light.debug_name()),
            None
        );
    }

    #[test]
    fn setup_context_can_request_history_texture() {
        let (device, queue) = create_test_device();
        let gpu =
            GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [32, 32]);
        let history_store = HistoryTextureStore::new();
        history_store.begin_frame(&gpu);
        let mut graph = RenderGraph::new();
        let mut state = PhaseState::new(TextureFormat::Bgra8Unorm, false);
        let mut frame = PreparedFrame::new(TextureFormat::Bgra8Unorm, false);
        let _ = frame.insert_payload(&history_store);
        let view = PreparedView::new(0, Default::default(), [32, 24], false).with_history_key(5);
        let mut ctx = RenderPhaseSetupContext::new(&mut graph, &mut state, &frame, &view);

        let history = ctx
            .history_texture("taa_color")
            .format(TextureFormat::Rgba16Float)
            .half_res()
            .ping_pong()
            .get();

        assert_eq!(history.size(), [16, 12]);
        assert_eq!(history.format(), TextureFormat::Rgba16Float);
        assert!(history.reset());
        assert!(history.read().is_none());
        assert!(graph.get_texture("history_5_taa_color_write").is_some());
    }

    struct DepthPublishingGraphPass;

    impl GraphPass for DepthPublishingGraphPass {
        fn name(&self) -> &'static str {
            "depth_publishing_graph_pass"
        }

        fn setup(&mut self, ctx: &mut GraphPassSetupContext<'_, '_>) {
            let depth = ctx.require_scene_texture(SceneTexture::Depth);
            let target_size = ctx.view().target_size();
            let target = ctx.graph().create_texture(|builder| {
                builder
                    .name("custom_indirect_diffuse")
                    .size(TargetSize::Exact(target_size[0], target_size[1]))
                    .format(TextureFormat::Rgba16Float)
                    .persistent();
            });
            ctx.graph().add_render_pass(self.name(), |setup| {
                setup.read(depth.handle());
                setup.write_color(0, target);
            });
            let _ = ctx.set_scene_texture(
                SceneTexture::IndirectDiffuse,
                target,
                TextureFormat::Rgba16Float,
            );
        }
    }

    #[test]
    fn custom_graph_pass_can_read_scene_depth_and_write_texture() {
        let mut graph = RenderGraph::new();
        let mut state = PhaseState::new(TextureFormat::Bgra8Unorm, false);
        let frame = PreparedFrame::new(TextureFormat::Bgra8Unorm, false);
        let view = PreparedView::new(0, Default::default(), [8, 8], false);
        let depth = graph.create_texture(|builder| {
            builder
                .name("scene_depth_for_graph_pass")
                .size(TargetSize::Exact(8, 8))
                .format(TextureFormat::Depth32Float);
        });
        graph.add_render_pass("scene_depth_seed_for_graph_pass", |setup| {
            setup.set_depth_stencil(depth);
        });
        let _ = state.set_scene_texture(SceneTexture::Depth, depth, TextureFormat::Depth32Float);

        let mut pass = DepthPublishingGraphPass;
        let mut ctx = GraphPassSetupContext::new(&mut graph, &mut state, &frame, &view);
        pass.setup(&mut ctx);

        let indirect = state
            .scene_texture(SceneTexture::IndirectDiffuse)
            .expect("graph pass should publish indirect diffuse");
        assert_eq!(indirect.format(), TextureFormat::Rgba16Float);
        let compiled = graph.compile().expect("graph pass setup should compile");
        let compiled_pass = compiled
            .iter()
            .find(|compiled_pass| compiled_pass.name == pass.name())
            .expect("graph pass should declare a render-graph pass");
        assert!(compiled_pass.reads.contains(&ResourceRef::Texture(depth)));
        assert!(compiled_pass
            .writes
            .contains(&ResourceRef::Texture(indirect.handle())));
    }

    #[test]
    fn graph_pass_setup_can_create_texture_from_spec() {
        let mut graph = RenderGraph::new();
        let mut state = PhaseState::new(TextureFormat::Bgra8Unorm, false);
        let frame = PreparedFrame::new(TextureFormat::Bgra8Unorm, false);
        let view = PreparedView::new(0, Default::default(), [64, 32], false);
        let mut ctx = GraphPassSetupContext::new(&mut graph, &mut state, &frame, &view);

        let slot = ctx.create_texture(
            TextureSpec::rgba16f("graph_pass_spec_target")
                .half_res()
                .storage()
                .mips(2),
        );

        assert_eq!(slot.format(), TextureFormat::Rgba16Float);
        assert_eq!(
            ctx.graph().get_texture("graph_pass_spec_target"),
            Some(slot.handle())
        );
    }

    #[test]
    fn graph_pass_contexts_share_blackboard_values() {
        let (device, queue) = create_test_device();
        let mut gpu =
            GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [8, 8]);
        let mut graph = RenderGraph::new();
        let mut state = PhaseState::new(TextureFormat::Bgra8Unorm, false);
        let frame = PreparedFrame::new(TextureFormat::Bgra8Unorm, false);
        let view = PreparedView::new(0, Default::default(), [8, 8], false);
        let published = {
            let mut setup = GraphPassSetupContext::new(&mut graph, &mut state, &frame, &view);
            let slot = setup.create_texture(TextureSpec::rgba16f("blackboard_target").persistent());
            setup.blackboard_set("custom_target", slot.handle());
            slot.handle()
        };
        let pass = CompiledPass {
            handle: PassHandle(0, 7),
            index: 0,
            name: Cow::Borrowed("blackboard_execute"),
            pass_type: PassType::Render,
            reads: Vec::new(),
            writes: Vec::new(),
            color_outputs: Vec::new(),
            depth_stencil: None,
            copy_ops: Vec::new(),
            flags: PassFlags::empty(),
            dep_level: 0,
        };
        let view_state = completed_view_state(&view, state.scene_shadows().cloned());
        let execution = ViewExecutionContext {
            frame: &frame,
            view: &view,
            view_state: &view_state,
            view_index: 0,
        };
        let textures = [];
        let buffers = [];
        let texture_descs = [];
        let buffer_descs = [];
        let alias_redirects = FxHashMap::default();
        let resources = PhysicalResources {
            handle_token: 7,
            textures: &textures,
            buffers: &buffers,
            texture_descs: &texture_descs,
            buffer_descs: &buffer_descs,
            alias_redirects: &alias_redirects,
            blackboard: graph.blackboard_ref(),
        };
        let ctx = GraphPassExecuteContext::new(&mut gpu, &pass, &resources, &execution);

        assert_eq!(
            ctx.blackboard_get::<TextureHandle>("custom_target"),
            Some(&published)
        );
    }

    #[test]
    fn postfx_and_finalize_contexts_share_blackboard_values() {
        let (device, queue) = create_test_device();
        let mut gpu =
            GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [8, 8]);
        let mut graph = RenderGraph::new();
        let mut state = PhaseState::new(TextureFormat::Bgra8Unorm, false);
        let frame = PreparedFrame::new(TextureFormat::Bgra8Unorm, false);
        let view = PreparedView::new(0, Default::default(), [8, 8], false);

        {
            let mut setup = PostFxPassSetupContext::new(&mut graph, &mut state, &frame, &view);
            setup.blackboard_set("postfx_value", 41u32);
            *setup
                .blackboard_get_mut::<u32>("postfx_value")
                .expect("postfx setup should retrieve blackboard value") += 1;
        }

        let pass = CompiledPass {
            handle: PassHandle(0, 7),
            index: 0,
            name: Cow::Borrowed("blackboard_postfx_execute"),
            pass_type: PassType::Render,
            reads: Vec::new(),
            writes: Vec::new(),
            color_outputs: Vec::new(),
            depth_stencil: None,
            copy_ops: Vec::new(),
            flags: PassFlags::empty(),
            dep_level: 0,
        };
        let view_state = completed_view_state(&view, state.scene_shadows().cloned());
        let execution = ViewExecutionContext {
            frame: &frame,
            view: &view,
            view_state: &view_state,
            view_index: 0,
        };
        let textures = [];
        let buffers = [];
        let texture_descs = [];
        let buffer_descs = [];
        let alias_redirects = FxHashMap::default();
        let resources = PhysicalResources {
            handle_token: 7,
            textures: &textures,
            buffers: &buffers,
            texture_descs: &texture_descs,
            buffer_descs: &buffer_descs,
            alias_redirects: &alias_redirects,
            blackboard: graph.blackboard_ref(),
        };
        let postfx_execute = PostFxPassExecuteContext::new(&mut gpu, &pass, &resources, &execution);
        assert_eq!(
            postfx_execute.blackboard_get::<u32>("postfx_value"),
            Some(&42)
        );

        let mut frame_slots = state.into_slots();
        let mut scene_gbuffer = Default::default();
        let completed_views = [];
        {
            let mut finalize_state = FinalizePhaseState::new(
                TextureFormat::Bgra8Unorm,
                false,
                &mut frame_slots,
                &mut scene_gbuffer,
                &completed_views,
            );
            let mut setup = RenderPassSetupContext::new(&mut graph, &mut finalize_state, &frame);
            setup.blackboard_set("finalize_value", 7u32);
        }
        let finalize_execution = FinalizeExecutionContext {
            frame: &frame,
            completed_views: &completed_views,
        };
        let resources = PhysicalResources {
            handle_token: 7,
            textures: &textures,
            buffers: &buffers,
            texture_descs: &texture_descs,
            buffer_descs: &buffer_descs,
            alias_redirects: &alias_redirects,
            blackboard: graph.blackboard_ref(),
        };
        let finalize_execute =
            RenderPassExecuteContext::new(&mut gpu, &pass, &resources, &finalize_execution);
        assert_eq!(
            finalize_execute.blackboard_get::<u32>("finalize_value"),
            Some(&7)
        );
    }

    #[test]
    fn graph_pass_setup_exposes_scene_lighting_resources() {
        let (device, queue) = create_test_device();
        let gpu = GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [8, 8]);
        let mut gpu_scene = GpuScene::new(&gpu);
        gpu_scene.table_mut::<LightTable>().set_all(
            &gpu,
            &[crate::render::GpuLight {
                pos_radius: [1.0, 2.0, 3.0, 4.0],
                color: [0.8, 0.7, 0.6, 1.0],
                falloff: [1.0, 0.0, 0.0, 0.0],
                dir_shadow: [0.0, 0.0, 0.0, -1.0],
            }],
        );
        let settings = RenderSettings {
            ambient_color: crate::render::Color::new(0.2, 0.3, 0.4, 1.0),
            ..Default::default()
        };
        let mut graph = RenderGraph::new();
        let mut state = PhaseState::new(TextureFormat::Bgra8Unorm, false);
        let mut frame = PreparedFrame::new(TextureFormat::Bgra8Unorm, false);
        let _ = frame.insert_payload(&settings);
        let _ = frame.insert_payload(&gpu_scene);
        let view = PreparedView::new(0, Default::default(), [8, 8], false);
        let ctx = GraphPassSetupContext::new(&mut graph, &mut state, &frame, &view);

        let lighting = ctx
            .scene_lighting()
            .expect("GpuScene payload should expose scene lighting");

        assert_eq!(lighting.ambient_color(), [0.2, 0.3, 0.4, 1.0]);
        assert_eq!(lighting.light_count(), 1);
        assert_eq!(lighting.point_light_count(), 1);
        assert_eq!(lighting.directional_light_count(), 0);
        assert!(lighting.scene_bind_group().is_none());
        assert!(std::ptr::eq(
            lighting.light_bind_group_layout(),
            gpu_scene.table::<LightTable>().bind_group_layout()
        ));
    }
}
