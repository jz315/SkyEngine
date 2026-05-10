use super::*;

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
