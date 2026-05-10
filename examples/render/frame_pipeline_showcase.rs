//! Minimal expert-only three-phase `expert::FramePipeline` example.
//!
//! Run with:
//!
//! ```bash
//! cargo run --example frame_pipeline_showcase --features app
//! ```

use sky_engine::gpu::GpuContext;
use sky_engine::render::expert::{
    CompiledPass, FinalizeExecutionContext, FinalizePhaseState, FrameFinalizeNode, FramePipeline,
    FrameSetupNode, FrameViewNode, PhaseState, PhysicalResources, PreparedFrame, PreparedView,
    RenderGraph, RenderGraphError, TargetSize, ViewExecutionContext, ViewportRect,
};

struct SetupSeed;

impl FrameSetupNode for SetupSeed {
    fn name(&self) -> &'static str {
        "setup_seed"
    }

    fn setup(
        &mut self,
        graph: &mut RenderGraph,
        state: &mut PhaseState,
        frame: &PreparedFrame<'_>,
    ) {
        let seed = graph.create_texture(|b| {
            b.name("showcase_seed")
                .size(TargetSize::Exact(64, 64))
                .format(frame.surface_format());
        });
        graph.add_render_pass("showcase_setup_seed", |s| {
            s.write_color(0, seed);
        });
        state.set_current_color(seed, frame.surface_format());
    }

    fn execute(
        &mut self,
        _pass: &CompiledPass,
        _ctx: &mut GpuContext,
        _resources: &PhysicalResources<'_>,
        _execution: &sky_engine::render::expert::SetupExecutionContext<'_>,
    ) -> Result<(), RenderGraphError> {
        Ok(())
    }
}

struct ViewCopy;

impl FrameViewNode for ViewCopy {
    fn name(&self) -> &'static str {
        "view_copy"
    }

    fn setup(
        &mut self,
        graph: &mut RenderGraph,
        state: &mut PhaseState,
        frame: &PreparedFrame<'_>,
        view: &PreparedView<'_>,
    ) {
        let input = state
            .current_color()
            .expect("setup phase should seed current_color");
        let output = graph.create_texture(|b| {
            b.name(format!("view_copy_{}", view.order()))
                .size(TargetSize::Exact(
                    view.target_size()[0],
                    view.target_size()[1],
                ))
                .format(frame.surface_format());
        });
        graph.add_render_pass(format!("showcase_view_copy_{}", view.order()), |s| {
            s.read(input.handle());
            s.write_color(0, output);
        });
        state.set_current_color(output, frame.surface_format());
    }

    fn execute(
        &mut self,
        _pass: &CompiledPass,
        _ctx: &mut GpuContext,
        _resources: &PhysicalResources<'_>,
        _execution: &ViewExecutionContext<'_>,
        _services: &mut (),
    ) -> Result<(), RenderGraphError> {
        Ok(())
    }
}

struct FinalizeSink;

impl FrameFinalizeNode for FinalizeSink {
    fn name(&self) -> &'static str {
        "finalize_sink"
    }

    fn setup(
        &mut self,
        graph: &mut RenderGraph,
        state: &mut FinalizePhaseState<'_>,
        frame: &PreparedFrame<'_>,
    ) {
        let output = graph.create_texture(|b| {
            b.name("showcase_final")
                .size(TargetSize::Exact(64, 64))
                .format(frame.surface_format())
                .persistent();
        });
        graph.add_render_pass("showcase_finalize", |s| {
            s.write_color(0, output);
        });
        state.set_current_color(output, frame.surface_format());
    }

    fn execute(
        &mut self,
        _pass: &CompiledPass,
        _ctx: &mut GpuContext,
        _resources: &PhysicalResources<'_>,
        _execution: &FinalizeExecutionContext<'_>,
    ) -> Result<(), RenderGraphError> {
        Ok(())
    }
}

fn create_device() -> (wgpu::Device, wgpu::Queue) {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        compatible_surface: None,
        force_fallback_adapter: false,
    }))
    .expect("No suitable GPU adapter found");

    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("frame_pipeline_showcase_device"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::default(),
        memory_hints: wgpu::MemoryHints::Performance,
        ..Default::default()
    }))
    .expect("Failed to create showcase device")
}

fn main() {
    let (device, queue) = create_device();
    let mut ctx =
        GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [64, 64]);

    let mut pipeline = FramePipeline::new();
    pipeline.add_setup_node(Box::new(SetupSeed));
    pipeline.add_view_node(Box::new(ViewCopy));
    pipeline.add_finalize_node(Box::new(FinalizeSink));

    let mut frame = PreparedFrame::new(ctx.surface_format(), false);
    frame.add_view(PreparedView::new(
        1,
        ViewportRect::new(0, 0, 32, 64),
        [32, 64],
        true,
    ));
    frame.add_view(PreparedView::new(
        0,
        ViewportRect::new(32, 0, 32, 64),
        [32, 64],
        false,
    ));

    ctx.begin_frame()
        .expect("headless begin_frame should succeed");
    let stats = pipeline.execute_frame(&mut ctx, &frame);
    ctx.end_frame();

    println!(
        "FramePipeline showcase finished: {} passes, {} draw calls",
        stats.passes, stats.draw_calls
    );
}
