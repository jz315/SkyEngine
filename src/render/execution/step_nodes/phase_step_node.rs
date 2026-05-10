use super::*;

#[derive(Clone, Copy)]
enum BuiltInPhaseKind {
    Opaque,
    Transparent,
}

pub(crate) struct PhaseStepNode<'a> {
    phase: &'a mut dyn RenderPhase,
}

impl<'a> PhaseStepNode<'a> {
    #[inline]
    pub(crate) fn new(phase: &'a mut dyn RenderPhase) -> Self {
        Self { phase }
    }
}

fn built_in_phase_name(kind: BuiltInPhaseKind) -> &'static str {
    match kind {
        BuiltInPhaseKind::Opaque => "opaque_phase",
        BuiltInPhaseKind::Transparent => "transparent_phase",
    }
}

fn built_in_phase_has_items(kind: BuiltInPhaseKind, view: &PreparedView<'_>) -> bool {
    if view
        .payload::<SceneView>()
        .is_some_and(SceneView::is_shadow)
    {
        return false;
    }
    match kind {
        BuiltInPhaseKind::Opaque => view
            .payload::<OpaquePhase>()
            .is_some_and(|phase| !phase.is_empty()),
        BuiltInPhaseKind::Transparent => view
            .payload::<TransparentPhase>()
            .is_some_and(|phase| !phase.is_empty()),
    }
}

fn built_in_phase_setup(kind: BuiltInPhaseKind, ctx: &mut PhaseSetupContext<'_, '_>) {
    if !built_in_phase_has_items(kind, ctx.view()) {
        return;
    }

    let phase_name = built_in_phase_name(kind);
    let target_size = ctx.view().target_size();
    let current = match kind {
        BuiltInPhaseKind::Opaque => bind_current_as_scene_color(ctx.state(), phase_name),
        BuiltInPhaseKind::Transparent => require_current_color(ctx.state(), phase_name),
    };
    let existing_depth = ctx.state().scene_depth().is_some();
    let depth_handle = match kind {
        BuiltInPhaseKind::Opaque => Some({
            let (graph, state) = ctx.graph_and_state();
            ensure_scene_texture(
                graph,
                state,
                target_size,
                SceneTexture::Depth,
                DEFAULT_DEPTH_FORMAT,
                "scene_depth",
            )
            .handle()
        }),
        BuiltInPhaseKind::Transparent => ctx.state().scene_depth().map(|slot| slot.handle()),
    };
    let shadow_graph_resources = if matches!(kind, BuiltInPhaseKind::Opaque) {
        ctx.view()
            .payload::<SceneView>()
            .and_then(|scene_view| scene_view.shadow_binding())
            .and_then(|binding| {
                let key = SceneShadowGraphResources::blackboard_key(binding);
                ctx.blackboard_get::<SceneShadowGraphResources>(&key)
                    .cloned()
            })
    } else {
        None
    };
    ctx.graph().add_render_pass(phase_name, |setup| {
        if let Some(shadows) = shadow_graph_resources.as_ref() {
            setup.read(shadows.directional_shadow_atlas());
            setup.read(shadows.directional_transparent_shadow_atlas());
        }
        setup.write_color_loaded(0, current.handle());
        match kind {
            BuiltInPhaseKind::Opaque => {
                if existing_depth {
                    setup.set_depth_stencil_loaded(
                        depth_handle.expect("opaque phase should reuse a depth target"),
                    );
                } else {
                    setup.set_depth_stencil(
                        depth_handle.expect("opaque phase should allocate a depth target"),
                    );
                }
            }
            BuiltInPhaseKind::Transparent => {
                if let Some(depth) = depth_handle {
                    setup.set_depth_stencil_loaded(depth);
                }
            }
        }
    });
}

fn built_in_phase_execute(
    kind: BuiltInPhaseKind,
    ctx: &mut PhaseExecuteContext<'_, '_, '_>,
) -> Result<(), RenderGraphError> {
    let phase_name = built_in_phase_name(kind);
    let (gpu, pass, resources, execution, draw_services) = ctx.split();
    let (draw_functions, material_registry, mesh_registry, fallback_texture) =
        draw_services.split();

    let target_handle = pass_first_write_texture(pass, phase_name, "output");
    let target = require_render_target(resources, target_handle, phase_name, "output");
    let load = pass
        .color_outputs
        .first()
        .map(|output| match output.load {
            LoadOp::Clear(color) => wgpu::LoadOp::Clear(wgpu::Color {
                r: color[0] as f64,
                g: color[1] as f64,
                b: color[2] as f64,
                a: color[3] as f64,
            }),
            LoadOp::Load => wgpu::LoadOp::Load,
            LoadOp::DontCare => wgpu::LoadOp::Load,
        })
        .unwrap_or(wgpu::LoadOp::Load);
    let depth_target = pass
        .depth_stencil
        .as_ref()
        .map(|depth| require_render_target(resources, depth.handle, phase_name, "depth"));
    let depth_format = depth_target.map(|target| target.format());
    let depth_attachment = pass
        .depth_stencil
        .as_ref()
        .zip(depth_target)
        .map(|(depth, target)| wgpu::RenderPassDepthStencilAttachment {
            view: target.view(),
            depth_ops: Some(wgpu::Operations {
                load: match depth.clear_depth {
                    Some(value) => wgpu::LoadOp::Clear(value),
                    None => wgpu::LoadOp::Load,
                },
                store: wgpu::StoreOp::Store,
            }),
            stencil_ops: None,
        });

    let gpu_scene = execution.frame_payload::<GpuScene>().ok_or_else(|| {
        RenderGraphError::ExecutionFailed("missing GpuScene frame payload".into())
    })?;
    let scene_view = execution
        .view_payload::<SceneView>()
        .ok_or_else(|| RenderGraphError::ExecutionFailed("missing SceneView payload".into()))?;
    gpu_scene.write_view_uniform(gpu.queue(), &scene_view.view_uniform);

    let phase_items = match kind {
        BuiltInPhaseKind::Opaque => execution
            .view_payload::<OpaquePhase>()
            .map(|phase| phase.items())
            .ok_or_else(|| {
                RenderGraphError::ExecutionFailed("missing OpaquePhase payload".into())
            })?,
        BuiltInPhaseKind::Transparent => execution
            .view_payload::<TransparentPhase>()
            .map(|phase| phase.items())
            .ok_or_else(|| {
                RenderGraphError::ExecutionFailed("missing TransparentPhase payload".into())
            })?,
    };

    let device = gpu.device().clone();
    let sampler_linear = gpu.sampler_linear().clone();
    let sampler_nearest = gpu.sampler_nearest().clone();
    let mut color_load = load;
    let mut cursor = 0usize;

    while cursor < phase_items.len() {
        let item = &phase_items[cursor];
        if draw_functions.is_standalone(item.draw_function_id) {
            let mut standalone_ctx = StandaloneDrawContext::new(
                gpu,
                target,
                execution.frame(),
                execution.view(),
                &device,
                &sampler_linear,
                &sampler_nearest,
                gpu_scene.view_bind_group(),
                gpu_scene.view_bind_group_layout(),
                gpu_scene.model_bind_group_layout(),
                material_registry,
                Some(fallback_texture),
                target.format(),
                depth_format,
            );
            draw_functions
                .draw_standalone(item.draw_function_id, &mut standalone_ctx, item)
                .map_err(|error| RenderGraphError::ExecutionFailed(error.to_string()))?;
            color_load = wgpu::LoadOp::Load;
            cursor += 1;
            continue;
        }

        let chunk_start = cursor;
        while cursor < phase_items.len()
            && !draw_functions.is_standalone(phase_items[cursor].draw_function_id)
        {
            cursor += 1;
        }

        let color_attachments = [Some(wgpu::RenderPassColorAttachment {
            view: target.view(),
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: color_load,
                store: wgpu::StoreOp::Store,
            },
        })];
        let mut draw_result = Ok(());
        {
            let mut frame = gpu.frame();
            let mut render_pass = frame.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some(phase_name),
                color_attachments: &color_attachments,
                depth_stencil_attachment: depth_attachment.clone(),
                ..Default::default()
            });
            let scene_shadows =
                execution.view_state().scene_shadows().cloned().or_else(|| {
                    match (
                        execution.frame_payload::<ShadowSceneBindingLayout>(),
                        execution.view_payload::<ShadowViewBinding>(),
                    ) {
                        (Some(layout), Some(shadow_view)) => Some(
                            SceneShadowResources::from_directional_shadow(layout, shadow_view),
                        ),
                        _ => None,
                    }
                });
            let gi_descriptor = execution
                .frame_payload::<GiRuntime>()
                .map(GiRuntime::shader_descriptor);
            let gi_sampling = execution
                .frame_payload::<GiRuntime>()
                .map(GiRuntime::sampling_binding);
            let mut draw_ctx = DrawContext::new(
                &device,
                &sampler_nearest,
                &mut render_pass,
                gpu_scene.view_bind_group(),
                gpu_scene.view_bind_group_layout(),
                gpu_scene.model_bind_group_layout(),
                execution
                    .frame_payload::<Vec<[f32; 16]>>()
                    .map(std::vec::Vec::as_slice),
                Some(gpu_scene),
                scene_shadows,
                gi_sampling,
                gi_descriptor.as_ref().map(|descriptor| descriptor.source),
                gi_descriptor
                    .as_ref()
                    .map_or(0, |descriptor| gi_shader_key(descriptor.key)),
                material_registry,
                mesh_registry,
                Some(fallback_texture),
                target.format(),
                depth_format,
            );
            let mut batch_start = chunk_start;
            while batch_start < cursor {
                let draw_function_id = phase_items[batch_start].draw_function_id;
                let batch_key = phase_items[batch_start].batch_key;
                let mut batch_end = batch_start + 1;
                while batch_end < cursor
                    && phase_items[batch_end].draw_function_id == draw_function_id
                    && phase_items[batch_end].batch_key == batch_key
                {
                    batch_end += 1;
                }

                if draw_result.is_ok() {
                    draw_result = draw_functions.draw_batch(
                        draw_function_id,
                        &mut draw_ctx,
                        &phase_items[batch_start..batch_end],
                    );
                }
                batch_start = batch_end;
            }
        }
        draw_result.map_err(|error| RenderGraphError::ExecutionFailed(error.to_string()))?;
        color_load = wgpu::LoadOp::Load;
    }

    Ok(())
}

fn built_in_phase_draw_calls(
    kind: BuiltInPhaseKind,
    execution: &ViewExecutionContext<'_>,
    draw_functions: &DrawFunctionRegistry,
) -> usize {
    let phase_items = match kind {
        BuiltInPhaseKind::Opaque => execution
            .view_payload::<OpaquePhase>()
            .map(|phase| phase.items()),
        BuiltInPhaseKind::Transparent => execution
            .view_payload::<TransparentPhase>()
            .map(|phase| phase.items()),
    };
    let Some(phase_items) = phase_items else {
        return 0;
    };
    if phase_items.is_empty() {
        return 0;
    }

    let mut draw_calls = 0usize;
    let mut cursor = 0usize;
    while cursor < phase_items.len() {
        let draw_function_id = phase_items[cursor].draw_function_id;
        let batch_key = phase_items[cursor].batch_key;
        let mut batch_end = cursor + 1;
        while batch_end < phase_items.len()
            && phase_items[batch_end].draw_function_id == draw_function_id
            && phase_items[batch_end].batch_key == batch_key
        {
            batch_end += 1;
        }
        draw_calls +=
            draw_functions.draw_call_count(draw_function_id, &phase_items[cursor..batch_end]);
        cursor = batch_end;
    }
    draw_calls
}

fn gi_shader_key(key: &str) -> u64 {
    use std::hash::{Hash, Hasher};

    let mut hasher = rustc_hash::FxHasher::default();
    key.hash(&mut hasher);
    hasher.finish()
}

impl FrameViewNode<dyn RuntimeRenderServices + '_> for PhaseStepNode<'_> {
    fn name(&self) -> &'static str {
        self.phase.name()
    }

    fn is_enabled(&self, frame: &PreparedFrame<'_>) -> bool {
        frame
            .views()
            .iter()
            .any(|view| self.phase.is_enabled(frame, view))
    }

    fn is_view_enabled(&self, frame: &PreparedFrame<'_>, view: &PreparedView<'_>) -> bool {
        self.phase.is_enabled(frame, view)
    }

    fn setup(
        &mut self,
        graph: &mut RenderGraph,
        state: &mut PhaseState,
        frame: &PreparedFrame<'_>,
        view: &PreparedView<'_>,
    ) {
        let mut context = PhaseSetupContext::new(graph, state, frame, view);
        self.phase.setup(&mut context);
    }

    fn execute(
        &mut self,
        pass: &CompiledPass,
        ctx: &mut GpuContext,
        resources: &PhysicalResources<'_>,
        execution: &ViewExecutionContext<'_>,
        services: &mut (dyn RuntimeRenderServices + '_),
    ) -> Result<(), RenderGraphError> {
        if !self.is_view_enabled(execution.frame(), execution.view()) {
            return Ok(());
        }
        let (draw_functions, material_registry, mesh_registry, fallback_texture) =
            services.split_phase_services();
        let draw_services = PhaseDrawServices::new(
            draw_functions,
            material_registry,
            mesh_registry,
            fallback_texture,
        );
        let mut context = PhaseExecuteContext::new(ctx, pass, resources, execution, draw_services);
        self.phase.execute(&mut context)
    }

    fn draw_calls(
        &self,
        execution: &ViewExecutionContext<'_>,
        services: &(dyn RuntimeRenderServices + '_),
    ) -> usize {
        if !self.is_view_enabled(execution.frame(), execution.view()) {
            return 0;
        }
        let name = self.phase.name();
        match name {
            "opaque" => built_in_phase_draw_calls(
                BuiltInPhaseKind::Opaque,
                execution,
                services.draw_functions(),
            ),
            "transparent" => built_in_phase_draw_calls(
                BuiltInPhaseKind::Transparent,
                execution,
                services.draw_functions(),
            ),
            _ => self.phase.draw_calls(execution),
        }
    }

    fn resize(&mut self, ctx: &GpuContext, width: u32, height: u32) {
        self.phase.resize(ctx, width, height);
    }
}

impl RenderPhase for OpaquePhase {
    fn name(&self) -> &'static str {
        "opaque"
    }

    fn is_enabled(&self, _frame: &PreparedFrame<'_>, view: &PreparedView<'_>) -> bool {
        built_in_phase_has_items(BuiltInPhaseKind::Opaque, view)
    }

    fn setup(&mut self, ctx: &mut PhaseSetupContext<'_, '_>) {
        built_in_phase_setup(BuiltInPhaseKind::Opaque, ctx);
    }

    fn execute(
        &mut self,
        ctx: &mut PhaseExecuteContext<'_, '_, '_>,
    ) -> Result<(), RenderGraphError> {
        built_in_phase_execute(BuiltInPhaseKind::Opaque, ctx)
    }
}

impl RenderPhase for TransparentPhase {
    fn name(&self) -> &'static str {
        "transparent"
    }

    fn is_enabled(&self, _frame: &PreparedFrame<'_>, view: &PreparedView<'_>) -> bool {
        built_in_phase_has_items(BuiltInPhaseKind::Transparent, view)
    }

    fn setup(&mut self, ctx: &mut PhaseSetupContext<'_, '_>) {
        built_in_phase_setup(BuiltInPhaseKind::Transparent, ctx);
    }

    fn execute(
        &mut self,
        ctx: &mut PhaseExecuteContext<'_, '_, '_>,
    ) -> Result<(), RenderGraphError> {
        built_in_phase_execute(BuiltInPhaseKind::Transparent, ctx)
    }
}
