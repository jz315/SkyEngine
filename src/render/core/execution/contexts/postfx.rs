use super::*;

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
