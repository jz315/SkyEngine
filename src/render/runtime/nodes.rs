use crate::gpu::GpuContext;
use crate::render::execution::{
    bind_current_as_scene_color, ensure_scene_texture, pass_first_write_texture,
    require_current_color, require_render_target, FinalizeExecutionContext, FinalizePhaseState,
    FrameFinalizeNode, FrameViewNode, PhaseState, PreparedFrame, PreparedView, SceneTextureKind,
    ViewExecutionContext,
};
use crate::render::gpu::GpuScene;
use crate::render::gpu::Texture;
use crate::render::graph::{
    CompiledPass, LoadOp, PhysicalResources, RenderGraph, RenderGraphError, TargetSize,
};
use crate::render::lighting::shadow::{ShadowSceneBindingLayout, ShadowViewBinding};
use crate::render::phase::{
    DrawContext, DrawFunctionRegistry, OpaquePhase, StandaloneDrawContext, TransparentPhase,
};
use crate::render::pipeline::{
    ComputePass, PostFxPass, RenderPass, RenderPhase, RenderPhaseExecuteContext,
    RenderPhaseSetupContext,
};
use crate::render::resources::material::MaterialRegistry;
use crate::render::resources::mesh::MeshRegistry;
use crate::render::view::SceneView;
use crate::render::{
    ComputePassExecuteContext, ComputePassSetupContext, PostFxPassExecuteContext,
    PostFxPassSetupContext, RenderPassExecuteContext, RenderPassSetupContext,
};
use crate::render::{RenderSettings, DEFAULT_DEPTH_FORMAT};

pub(crate) struct SceneColorSeedNode {
    format: wgpu::TextureFormat,
}

impl SceneColorSeedNode {
    #[inline]
    pub(crate) fn new(format: wgpu::TextureFormat) -> Self {
        Self { format }
    }
}

impl FrameViewNode for SceneColorSeedNode {
    fn name(&self) -> &'static str {
        "scene_color_seed"
    }

    fn setup(
        &mut self,
        graph: &mut RenderGraph,
        state: &mut PhaseState,
        _frame: &PreparedFrame<'_>,
        view: &PreparedView<'_>,
    ) {
        if view
            .payload::<SceneView>()
            .is_some_and(|scene_view| !scene_view.presents_to_surface())
        {
            return;
        }
        let target = graph.create_texture(|builder| {
            builder
                .name("scene_color_seed")
                .size(TargetSize::Exact(
                    view.target_size()[0],
                    view.target_size()[1],
                ))
                .format(self.format);
        });
        graph.add_render_pass("scene_color_seed", |setup| {
            setup.write_color(0, target);
        });
        state.set_current_color(target, self.format);
        state.set_scene_color(target, self.format);
    }

    fn execute(
        &mut self,
        pass: &CompiledPass,
        ctx: &mut GpuContext,
        resources: &PhysicalResources<'_>,
        execution: &ViewExecutionContext<'_>,
    ) -> Result<(), RenderGraphError> {
        let color = execution
            .frame_payload::<RenderSettings>()
            .copied()
            .unwrap_or_default()
            .clear_color
            .to_wgpu();
        let output_handle = pass_first_write_texture(pass, self.name(), "output");
        let output = require_render_target(resources, output_handle, self.name(), "output");
        let color_attachments = [Some(wgpu::RenderPassColorAttachment {
            view: output.view(),
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(color),
                store: wgpu::StoreOp::Store,
            },
        })];
        let mut frame = ctx.frame();
        let _render_pass = frame.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("scene_color_seed"),
            color_attachments: &color_attachments,
            depth_stencil_attachment: None,
            ..Default::default()
        });
        Ok(())
    }
}

#[derive(Clone, Copy)]
enum BuiltInPhaseKind {
    Opaque,
    Transparent,
}

pub(crate) struct PhaseStepNode {
    phase: *mut dyn RenderPhase,
    draw_functions: *mut DrawFunctionRegistry,
    materials: *mut MaterialRegistry,
    mesh_registry: *const MeshRegistry,
    fallback_texture: *const Texture,
}

unsafe impl Send for PhaseStepNode {}

impl PhaseStepNode {
    #[inline]
    pub(crate) fn new(
        phase: &mut dyn RenderPhase,
        draw_functions: &mut DrawFunctionRegistry,
        materials: &mut MaterialRegistry,
        mesh_registry: &MeshRegistry,
        fallback_texture: &Texture,
    ) -> Self {
        Self {
            phase,
            draw_functions,
            materials,
            mesh_registry,
            fallback_texture,
        }
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

fn built_in_phase_setup(kind: BuiltInPhaseKind, ctx: &mut RenderPhaseSetupContext<'_, '_>) {
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
                SceneTextureKind::Depth,
                DEFAULT_DEPTH_FORMAT,
                "scene_depth",
            )
            .handle()
        }),
        BuiltInPhaseKind::Transparent => ctx.state().scene_depth().map(|slot| slot.handle()),
    };
    ctx.graph().add_render_pass(phase_name, |setup| {
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
    ctx: &mut RenderPhaseExecuteContext<'_, '_, '_>,
) -> Result<(), RenderGraphError> {
    let phase_name = built_in_phase_name(kind);
    let (
        gpu,
        pass,
        resources,
        execution,
        draw_functions,
        material_registry,
        mesh_registry,
        fallback_texture,
    ) = ctx.split();

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
            let mut draw_ctx = DrawContext::new(
                &device,
                &sampler_linear,
                &sampler_nearest,
                &mut render_pass,
                gpu_scene.view_bind_group(),
                gpu_scene.view_bind_group_layout(),
                gpu_scene.model_bind_group_layout(),
                execution
                    .frame_payload::<Vec<[f32; 16]>>()
                    .map(std::vec::Vec::as_slice),
                Some(gpu_scene),
                execution.frame_payload::<ShadowSceneBindingLayout>(),
                execution.view_payload::<ShadowViewBinding>(),
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

impl FrameViewNode for PhaseStepNode {
    fn name(&self) -> &'static str {
        unsafe { (&*self.phase).name() }
    }

    fn is_enabled(&self, frame: &PreparedFrame<'_>) -> bool {
        frame
            .views()
            .iter()
            .any(|view| unsafe { (&*self.phase).is_enabled(frame, view) })
    }

    fn is_view_enabled(&self, frame: &PreparedFrame<'_>, view: &PreparedView<'_>) -> bool {
        unsafe { (&*self.phase).is_enabled(frame, view) }
    }

    fn setup(
        &mut self,
        graph: &mut RenderGraph,
        state: &mut PhaseState,
        frame: &PreparedFrame<'_>,
        view: &PreparedView<'_>,
    ) {
        let mut context = RenderPhaseSetupContext::new(graph, state, frame, view);
        unsafe {
            (&mut *self.phase).setup(&mut context);
        }
    }

    fn execute(
        &mut self,
        pass: &CompiledPass,
        ctx: &mut GpuContext,
        resources: &PhysicalResources<'_>,
        execution: &ViewExecutionContext<'_>,
    ) -> Result<(), RenderGraphError> {
        if !self.is_view_enabled(execution.frame(), execution.view()) {
            return Ok(());
        }
        let mut context = RenderPhaseExecuteContext::new(
            ctx,
            pass,
            resources,
            execution,
            unsafe { &mut *self.draw_functions },
            unsafe { &mut *self.materials },
            unsafe { &*self.mesh_registry },
            unsafe { &*self.fallback_texture },
        );
        unsafe { (&mut *self.phase).execute(&mut context) }
    }

    fn draw_calls(&self, execution: &ViewExecutionContext<'_>) -> usize {
        if !self.is_view_enabled(execution.frame(), execution.view()) {
            return 0;
        }
        let name = unsafe { (&*self.phase).name() };
        match name {
            "opaque" => built_in_phase_draw_calls(BuiltInPhaseKind::Opaque, execution, unsafe {
                &*self.draw_functions
            }),
            "transparent" => {
                built_in_phase_draw_calls(BuiltInPhaseKind::Transparent, execution, unsafe {
                    &*self.draw_functions
                })
            }
            _ => unsafe { (&*self.phase).draw_calls(execution) },
        }
    }

    fn resize(&mut self, ctx: &GpuContext, width: u32, height: u32) {
        unsafe {
            (&mut *self.phase).resize(ctx, width, height);
        }
    }
}

impl RenderPhase for OpaquePhase {
    fn name(&self) -> &'static str {
        "opaque"
    }

    fn is_enabled(&self, _frame: &PreparedFrame<'_>, view: &PreparedView<'_>) -> bool {
        built_in_phase_has_items(BuiltInPhaseKind::Opaque, view)
    }

    fn setup(&mut self, ctx: &mut RenderPhaseSetupContext<'_, '_>) {
        built_in_phase_setup(BuiltInPhaseKind::Opaque, ctx);
    }

    fn execute(
        &mut self,
        ctx: &mut RenderPhaseExecuteContext<'_, '_, '_>,
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

    fn setup(&mut self, ctx: &mut RenderPhaseSetupContext<'_, '_>) {
        built_in_phase_setup(BuiltInPhaseKind::Transparent, ctx);
    }

    fn execute(
        &mut self,
        ctx: &mut RenderPhaseExecuteContext<'_, '_, '_>,
    ) -> Result<(), RenderGraphError> {
        built_in_phase_execute(BuiltInPhaseKind::Transparent, ctx)
    }
}

pub(crate) struct HeadlessKeepAliveNode;

impl FrameViewNode for HeadlessKeepAliveNode {
    fn name(&self) -> &'static str {
        "scene_headless_keepalive"
    }

    fn is_enabled(&self, frame: &PreparedFrame<'_>) -> bool {
        !frame.has_surface()
    }

    fn setup(
        &mut self,
        graph: &mut RenderGraph,
        state: &mut PhaseState,
        _frame: &PreparedFrame<'_>,
        view: &PreparedView<'_>,
    ) {
        if view
            .payload::<SceneView>()
            .is_some_and(|scene_view| scene_view.is_shadow())
        {
            return;
        }
        let input = require_current_color(state, self.name());
        let sink = graph.create_texture(|builder| {
            builder
                .name("scene_headless_keepalive")
                .size(TargetSize::Exact(
                    view.target_size()[0],
                    view.target_size()[1],
                ))
                .format(input.format())
                .persistent();
        });
        graph.add_render_pass("scene_headless_keepalive", |setup| {
            setup.read(input.handle());
            setup.write_color(0, sink);
        });
        state.set_current_color(sink, input.format());
    }

    fn execute(
        &mut self,
        _pass: &CompiledPass,
        _ctx: &mut GpuContext,
        _resources: &PhysicalResources<'_>,
        _execution: &ViewExecutionContext<'_>,
    ) -> Result<(), RenderGraphError> {
        Ok(())
    }
}

pub(crate) struct ComputeStepNode {
    compute: *mut dyn ComputePass,
}

impl ComputeStepNode {
    #[inline]
    pub(crate) fn new(compute: &mut dyn ComputePass) -> Self {
        Self { compute }
    }
}

unsafe impl Send for ComputeStepNode {}

impl FrameViewNode for ComputeStepNode {
    fn name(&self) -> &'static str {
        unsafe { (&*self.compute).name() }
    }

    fn setup(
        &mut self,
        graph: &mut RenderGraph,
        state: &mut PhaseState,
        frame: &PreparedFrame<'_>,
        view: &PreparedView<'_>,
    ) {
        let mut context = ComputePassSetupContext::new(graph, state, frame, view);
        unsafe {
            (&mut *self.compute).setup(&mut context);
        }
    }

    fn execute(
        &mut self,
        pass: &CompiledPass,
        ctx: &mut GpuContext,
        resources: &PhysicalResources<'_>,
        execution: &ViewExecutionContext<'_>,
    ) -> Result<(), RenderGraphError> {
        let mut context = ComputePassExecuteContext::new(ctx, pass, resources, execution);
        unsafe {
            (&mut *self.compute).execute(&mut context)?;
        }
        Ok(())
    }
}

pub(crate) struct PostFxStepNode {
    fx: *mut dyn PostFxPass,
}

impl PostFxStepNode {
    #[inline]
    pub(crate) fn new(fx: &mut dyn PostFxPass) -> Self {
        Self { fx }
    }
}

unsafe impl Send for PostFxStepNode {}

impl FrameViewNode for PostFxStepNode {
    fn name(&self) -> &'static str {
        unsafe { (&*self.fx).name() }
    }

    fn is_enabled(&self, frame: &PreparedFrame<'_>) -> bool {
        frame
            .views()
            .iter()
            .any(|view| unsafe { (&*self.fx).is_enabled(frame, view) })
    }

    fn is_view_enabled(&self, frame: &PreparedFrame<'_>, view: &PreparedView<'_>) -> bool {
        if view
            .payload::<SceneView>()
            .is_some_and(|scene_view| !scene_view.presents_to_surface())
        {
            return false;
        }
        unsafe { (&*self.fx).is_enabled(frame, view) }
    }

    fn setup(
        &mut self,
        graph: &mut RenderGraph,
        state: &mut PhaseState,
        frame: &PreparedFrame<'_>,
        view: &PreparedView<'_>,
    ) {
        let mut context = PostFxPassSetupContext::new(graph, state, frame, view);
        unsafe {
            (&mut *self.fx).setup(&mut context);
        }
    }

    fn execute(
        &mut self,
        pass: &CompiledPass,
        ctx: &mut GpuContext,
        resources: &PhysicalResources<'_>,
        execution: &ViewExecutionContext<'_>,
    ) -> Result<(), RenderGraphError> {
        if !self.is_view_enabled(execution.frame(), execution.view()) {
            return Ok(());
        }
        let mut context = PostFxPassExecuteContext::new(ctx, pass, resources, execution);
        unsafe {
            (&mut *self.fx).execute(&mut context)?;
        }
        Ok(())
    }

    fn draw_calls(&self, execution: &ViewExecutionContext<'_>) -> usize {
        if !self.is_view_enabled(execution.frame(), execution.view()) {
            return 0;
        }
        unsafe { (&*self.fx).draw_calls(execution) }
    }

    fn resize(&mut self, ctx: &GpuContext, width: u32, height: u32) {
        unsafe {
            (&mut *self.fx).resize(ctx, width, height);
        }
    }
}

pub(crate) struct RenderPassStepNode {
    pass: *mut dyn RenderPass,
}

impl RenderPassStepNode {
    #[inline]
    pub(crate) fn new(pass: &mut dyn RenderPass) -> Self {
        Self { pass }
    }
}

unsafe impl Send for RenderPassStepNode {}

impl FrameFinalizeNode for RenderPassStepNode {
    fn name(&self) -> &'static str {
        unsafe { (&*self.pass).name() }
    }

    fn setup(
        &mut self,
        graph: &mut RenderGraph,
        state: &mut FinalizePhaseState<'_>,
        frame: &PreparedFrame<'_>,
    ) {
        let mut context = RenderPassSetupContext::new(graph, state, frame);
        unsafe {
            (&mut *self.pass).setup(&mut context);
        }
    }

    fn execute(
        &mut self,
        pass: &CompiledPass,
        ctx: &mut GpuContext,
        resources: &PhysicalResources<'_>,
        execution: &FinalizeExecutionContext<'_>,
    ) -> Result<(), RenderGraphError> {
        let mut context = RenderPassExecuteContext::new(ctx, pass, resources, execution);
        unsafe {
            (&mut *self.pass).execute(&mut context)?;
        }
        Ok(())
    }
}
