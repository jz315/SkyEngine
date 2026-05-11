use super::common::*;

#[derive(Clone)]
struct CountingComputePass {
    setup_calls: Arc<AtomicUsize>,
    execute_calls: Arc<AtomicUsize>,
}

impl ComputePass for CountingComputePass {
    fn name(&self) -> &'static str {
        "counting_compute"
    }

    fn setup(&mut self, ctx: &mut ComputePassSetupContext<'_, '_>) {
        self.setup_calls.fetch_add(1, Ordering::Relaxed);
        let current = ctx
            .state()
            .current_color()
            .expect("scene color should exist before custom compute");
        ctx.graph().add_compute_pass(self.name(), |setup| {
            setup.readwrite(current.handle());
        });
    }

    fn execute(
        &mut self,
        ctx: &mut ComputePassExecuteContext<'_, '_>,
    ) -> Result<(), RenderGraphError> {
        self.execute_calls.fetch_add(1, Ordering::Relaxed);
        assert!(ctx.scene_view().is_some());
        Ok(())
    }
}

#[derive(Clone)]
struct CountingPostFxPass {
    setup_calls: Arc<AtomicUsize>,
    execute_calls: Arc<AtomicUsize>,
}

impl PostFxPass for CountingPostFxPass {
    fn name(&self) -> &'static str {
        "counting_postfx"
    }

    fn setup(&mut self, ctx: &mut PostFxPassSetupContext<'_, '_>) {
        self.setup_calls.fetch_add(1, Ordering::Relaxed);
        let current = ctx
            .state()
            .current_color()
            .expect("scene color should exist before custom postfx");
        ctx.graph().add_render_pass(self.name(), |setup| {
            setup.read(current.handle());
            setup.write_color_loaded(0, current.handle());
        });
    }

    fn execute(
        &mut self,
        ctx: &mut PostFxPassExecuteContext<'_, '_>,
    ) -> Result<(), RenderGraphError> {
        self.execute_calls.fetch_add(1, Ordering::Relaxed);
        assert!(ctx.scene_view().is_some());
        Ok(())
    }
}

#[derive(Clone)]
struct CountingFinalizePass {
    setup_calls: Arc<AtomicUsize>,
    execute_calls: Arc<AtomicUsize>,
}

impl RenderPass for CountingFinalizePass {
    fn name(&self) -> &'static str {
        "counting_finalize"
    }

    fn setup(&mut self, ctx: &mut RenderPassSetupContext<'_, '_, '_>) {
        self.setup_calls.fetch_add(1, Ordering::Relaxed);
        let first_view = ctx
            .state()
            .completed_views()
            .first()
            .expect("finalize pass should see at least one prepared view");
        let input = first_view
            .slots()
            .current_color()
            .expect("view should expose a current color slot");
        let size = first_view.target_size();
        let sink = ctx.graph().create_texture(|builder| {
            builder
                .name("counting_finalize_sink")
                .size(TargetSize::Exact(size[0], size[1]))
                .format(input.format())
                .persistent();
        });
        ctx.graph().add_render_pass(self.name(), |setup| {
            setup.read(input.handle());
            setup.write_color(0, sink);
        });
        let _ = ctx.state().set_current_color(sink, input.format());
    }

    fn execute(
        &mut self,
        ctx: &mut RenderPassExecuteContext<'_, '_>,
    ) -> Result<(), RenderGraphError> {
        self.execute_calls.fetch_add(1, Ordering::Relaxed);
        assert_eq!(ctx.completed_views().len(), 1);
        assert_eq!(ctx.pass().name.as_ref(), self.name());
        Ok(())
    }
}

#[derive(Clone)]
struct CountingPhase {
    setup_calls: Arc<AtomicUsize>,
    execute_calls: Arc<AtomicUsize>,
}

impl RenderPhase for CountingPhase {
    fn name(&self) -> &'static str {
        "counting_phase"
    }

    fn setup(&mut self, ctx: &mut PhaseSetupContext<'_, '_>) {
        self.setup_calls.fetch_add(1, Ordering::Relaxed);
        let current = ctx
            .state()
            .current_color()
            .expect("scene color should exist before custom phase");
        ctx.graph().add_render_pass(self.name(), |setup| {
            setup.write_color_loaded(0, current.handle());
        });
    }

    fn execute(
        &mut self,
        ctx: &mut PhaseExecuteContext<'_, '_, '_>,
    ) -> Result<(), RenderGraphError> {
        self.execute_calls.fetch_add(1, Ordering::Relaxed);
        assert!(ctx.scene_view().is_some());

        let (gpu, pass, resources, _execution, _draw_services) = ctx.split();
        let output_handle =
            crate::render::execution::pass_first_write_texture(pass, self.name(), "output");
        let output = crate::render::execution::require_render_target(
            resources,
            output_handle,
            self.name(),
            "output",
        );
        let color_attachments = [Some(wgpu::RenderPassColorAttachment {
            view: output.view(),
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Load,
                store: wgpu::StoreOp::Store,
            },
        })];
        let mut frame = gpu.frame();
        let _render_pass = frame.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(self.name()),
            color_attachments: &color_attachments,
            depth_stencil_attachment: None,
            ..Default::default()
        });
        Ok(())
    }
}

#[derive(Clone)]
struct ViewFilteredPhase {
    enabled_order: i32,
    setup_calls: Arc<AtomicUsize>,
    execute_calls: Arc<AtomicUsize>,
}

impl RenderPhase for ViewFilteredPhase {
    fn name(&self) -> &'static str {
        "view_filtered_phase"
    }

    fn is_enabled(&self, _frame: &PreparedFrame<'_>, view: &PreparedView<'_>) -> bool {
        view.order() == self.enabled_order
    }

    fn setup(&mut self, ctx: &mut PhaseSetupContext<'_, '_>) {
        assert_eq!(ctx.view().order(), self.enabled_order);
        self.setup_calls.fetch_add(1, Ordering::Relaxed);
        let current = ctx
            .state()
            .current_color()
            .expect("scene color should exist before filtered phase");
        ctx.graph().add_render_pass(self.name(), |setup| {
            setup.write_color_loaded(0, current.handle());
        });
    }

    fn execute(
        &mut self,
        ctx: &mut PhaseExecuteContext<'_, '_, '_>,
    ) -> Result<(), RenderGraphError> {
        assert_eq!(ctx.view().order(), self.enabled_order);
        self.execute_calls.fetch_add(1, Ordering::Relaxed);
        let (gpu, pass, resources, _execution, _draw_services) = ctx.split();
        let output_handle =
            crate::render::execution::pass_first_write_texture(pass, self.name(), "output");
        let output = crate::render::execution::require_render_target(
            resources,
            output_handle,
            self.name(),
            "output",
        );
        let color_attachments = [Some(wgpu::RenderPassColorAttachment {
            view: output.view(),
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Load,
                store: wgpu::StoreOp::Store,
            },
        })];
        let mut frame = gpu.frame();
        let _render_pass = frame.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(self.name()),
            color_attachments: &color_attachments,
            depth_stencil_attachment: None,
            ..Default::default()
        });
        Ok(())
    }
}

#[derive(Clone)]
struct ViewFilteredPostFxPass {
    enabled_order: i32,
    setup_calls: Arc<AtomicUsize>,
    execute_calls: Arc<AtomicUsize>,
}

impl PostFxPass for ViewFilteredPostFxPass {
    fn name(&self) -> &'static str {
        "view_filtered_postfx"
    }

    fn is_enabled(&self, _frame: &PreparedFrame<'_>, view: &PreparedView<'_>) -> bool {
        view.order() == self.enabled_order
    }

    fn setup(&mut self, ctx: &mut PostFxPassSetupContext<'_, '_>) {
        assert_eq!(ctx.view().order(), self.enabled_order);
        self.setup_calls.fetch_add(1, Ordering::Relaxed);
        let current = ctx
            .state()
            .current_color()
            .expect("scene color should exist before filtered postfx");
        ctx.graph().add_render_pass(self.name(), |setup| {
            setup.read(current.handle());
            setup.write_color_loaded(0, current.handle());
        });
    }

    fn execute(
        &mut self,
        ctx: &mut PostFxPassExecuteContext<'_, '_>,
    ) -> Result<(), RenderGraphError> {
        assert_eq!(ctx.view().order(), self.enabled_order);
        self.execute_calls.fetch_add(1, Ordering::Relaxed);
        let (gpu, pass, resources, _execution) = ctx.split();
        let output_handle =
            crate::render::execution::pass_first_write_texture(pass, self.name(), "output");
        let output = crate::render::execution::require_render_target(
            resources,
            output_handle,
            self.name(),
            "output",
        );
        let color_attachments = [Some(wgpu::RenderPassColorAttachment {
            view: output.view(),
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Load,
                store: wgpu::StoreOp::Store,
            },
        })];
        let mut frame = gpu.frame();
        let _render_pass = frame.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(self.name()),
            color_attachments: &color_attachments,
            depth_stencil_attachment: None,
            ..Default::default()
        });
        Ok(())
    }
}

#[test]
fn custom_pipeline_steps_receive_setup_and_execute_contexts() {
    let (device, queue) = create_test_device();
    let mut ctx =
        GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [64, 64]);

    let compute_setup = Arc::new(AtomicUsize::new(0));
    let compute_execute = Arc::new(AtomicUsize::new(0));
    let postfx_setup = Arc::new(AtomicUsize::new(0));
    let postfx_execute = Arc::new(AtomicUsize::new(0));
    let finalize_setup = Arc::new(AtomicUsize::new(0));
    let finalize_execute = Arc::new(AtomicUsize::new(0));

    let pipeline = RenderPipelineBuilder::new()
        .add_compute(CountingComputePass {
            setup_calls: compute_setup.clone(),
            execute_calls: compute_execute.clone(),
        })
        .add_postfx(CountingPostFxPass {
            setup_calls: postfx_setup.clone(),
            execute_calls: postfx_execute.clone(),
        })
        .add_pass(CountingFinalizePass {
            setup_calls: finalize_setup.clone(),
            execute_calls: finalize_execute.clone(),
        })
        .build();
    let mut renderer = RenderRuntime::from_asset(pipeline);
    let world = World::new();

    ctx.begin_frame()
        .expect("headless begin_frame should succeed");
    renderer.render_world(&mut ctx, &world);
    ctx.end_frame();

    assert_eq!(compute_setup.load(Ordering::Relaxed), 1);
    assert_eq!(compute_execute.load(Ordering::Relaxed), 1);
    assert_eq!(postfx_setup.load(Ordering::Relaxed), 1);
    assert_eq!(postfx_execute.load(Ordering::Relaxed), 1);
    assert_eq!(finalize_setup.load(Ordering::Relaxed), 1);
    assert_eq!(finalize_execute.load(Ordering::Relaxed), 1);
}

#[test]
fn custom_phase_steps_receive_setup_and_execute_contexts() {
    let (device, queue) = create_test_device();
    let mut ctx =
        GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [64, 64]);

    let phase_setup = Arc::new(AtomicUsize::new(0));
    let phase_execute = Arc::new(AtomicUsize::new(0));

    let pipeline = RenderPipelineBuilder::new()
        .add_phase(CountingPhase {
            setup_calls: phase_setup.clone(),
            execute_calls: phase_execute.clone(),
        })
        .build();
    let mut renderer = RenderRuntime::from_asset(pipeline);
    let world = World::new();

    ctx.begin_frame()
        .expect("headless begin_frame should succeed");
    renderer.render_world(&mut ctx, &world);
    ctx.end_frame();

    assert_eq!(phase_setup.load(Ordering::Relaxed), 1);
    assert_eq!(phase_execute.load(Ordering::Relaxed), 1);
}

#[test]
fn view_specific_phase_and_postfx_skip_disabled_views() {
    let (device, queue) = create_test_device();
    let mut ctx =
        GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [64, 64]);

    let phase_setup = Arc::new(AtomicUsize::new(0));
    let phase_execute = Arc::new(AtomicUsize::new(0));
    let postfx_setup = Arc::new(AtomicUsize::new(0));
    let postfx_execute = Arc::new(AtomicUsize::new(0));

    let pipeline = RenderPipelineBuilder::new()
        .add_phase(ViewFilteredPhase {
            enabled_order: 1,
            setup_calls: phase_setup.clone(),
            execute_calls: phase_execute.clone(),
        })
        .add_postfx(ViewFilteredPostFxPass {
            enabled_order: 1,
            setup_calls: postfx_setup.clone(),
            execute_calls: postfx_execute.clone(),
        })
        .build();
    let mut renderer = RenderRuntime::from_asset(pipeline);

    let mut world = World::new();
    world.spawn((
        Transform::default(),
        CameraMarker::new(),
        Projection::orthographic_fixed(32.0, 32.0),
        CameraViewport::new(ViewportRect::new(0, 0, 32, 32)).order(10),
    ));
    world.spawn((
        Transform::default(),
        CameraMarker::new(),
        Projection::orthographic_fixed(32.0, 32.0),
        CameraViewport::new(ViewportRect::new(32, 0, 32, 32)).order(20),
    ));

    ctx.begin_frame()
        .expect("headless begin_frame should succeed");
    renderer.render_world(&mut ctx, &world);
    ctx.end_frame();

    assert_eq!(renderer.stats().view_count, 2);
    assert_eq!(phase_setup.load(Ordering::Relaxed), 1);
    assert_eq!(phase_execute.load(Ordering::Relaxed), 1);
    assert_eq!(postfx_setup.load(Ordering::Relaxed), 1);
    assert_eq!(postfx_execute.load(Ordering::Relaxed), 1);
}

#[test]
fn builtin_bloom_executes_explicit_graph_passes_for_each_view() {
    let (device, queue) = create_test_device();
    let mut ctx =
        GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [64, 32]);

    let mut renderer = RenderRuntime::from_asset(RenderPipelineAsset::forward_2d());
    let mut world = World::new();
    world.insert_resource(RenderSettings::default());
    world.spawn((
        Transform::default(),
        CameraMarker::new(),
        Projection::orthographic_fixed(32.0, 32.0),
        CameraViewport::new(ViewportRect::new(0, 0, 32, 32)).order(0),
    ));
    world.spawn((
        Transform::default(),
        CameraMarker::new(),
        Projection::orthographic_fixed(32.0, 32.0),
        CameraViewport::new(ViewportRect::new(32, 0, 32, 32)).order(1),
    ));

    ctx.begin_frame()
        .expect("headless begin_frame should succeed");
    renderer.render_world(&mut ctx, &world);
    ctx.end_frame();

    let stats = renderer.stats();
    assert_eq!(stats.view_count, 2);
    assert!(
        stats.passes >= 2 * crate::render::postfx::bloom::BLOOM_GRAPH_PASS_COUNT,
        "expected both views to register bloom's expanded graph passes, got {} passes",
        stats.passes
    );
}
