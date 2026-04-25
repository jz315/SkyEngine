use std::sync::{Arc, Mutex};

use crate::gpu::GpuContext;
use crate::render::graph::{CompiledPass, PhysicalResources, RenderGraph, RenderGraphError};
use crate::render::view::ViewportRect;

use super::{
    create_scene_texture, ensure_scene_texture, FinalizeExecutionContext, FinalizePhaseState,
    FrameFinalizeNode, FramePipeline, FrameSetupNode, FrameViewNode, PhaseState, PreparedFrame,
    PreparedView, SceneTextureKind, SetupExecutionContext, ViewExecutionContext,
};

fn create_test_device() -> (wgpu::Device, wgpu::Queue) {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        compatible_surface: None,
        force_fallback_adapter: false,
    }))
    .expect("No suitable GPU adapter found for frame pipeline tests");

    pollster::block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: Some("frame_pipeline_test_device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::Performance,
        },
        None,
    ))
    .expect("Failed to create test GPU device")
}

#[derive(Clone, Copy)]
struct FrameToken(u32);

#[derive(Clone, Copy)]
struct ViewToken(u32);

struct SetupRecorder {
    log: Arc<Mutex<Vec<String>>>,
}

impl FrameSetupNode for SetupRecorder {
    fn name(&self) -> &'static str {
        "setup"
    }

    fn setup(
        &mut self,
        graph: &mut RenderGraph,
        state: &mut PhaseState,
        frame: &PreparedFrame<'_>,
    ) {
        assert_eq!(frame.payload::<FrameToken>().map(|token| token.0), Some(99));
        self.log.lock().unwrap().push("setup".to_string());
        let setup_tex = graph.create_texture(|b| {
            b.name("setup_seed")
                .size(crate::render::graph::TargetSize::Exact(8, 8))
                .format(frame.surface_format());
        });
        graph.add_render_pass("setup_seed", |s| {
            s.write_color(0, setup_tex);
        });
        state.set_current_color(setup_tex, frame.surface_format());
    }

    fn execute(
        &mut self,
        _pass: &CompiledPass,
        _ctx: &mut GpuContext,
        _resources: &PhysicalResources<'_>,
        _execution: &SetupExecutionContext<'_>,
    ) -> Result<(), RenderGraphError> {
        Ok(())
    }
}

struct SeedViewNode {
    log: Arc<Mutex<Vec<String>>>,
}

impl FrameViewNode for SeedViewNode {
    fn name(&self) -> &'static str {
        "seed_view"
    }

    fn setup(
        &mut self,
        graph: &mut RenderGraph,
        state: &mut PhaseState,
        frame: &PreparedFrame<'_>,
        view: &PreparedView<'_>,
    ) {
        let token = view
            .payload::<ViewToken>()
            .expect("seed_view requires a view token")
            .0;
        assert_eq!(frame.payload::<FrameToken>().map(|token| token.0), Some(99));
        assert!(
            state.current_color().is_some(),
            "setup phase should seed current_color"
        );
        self.log.lock().unwrap().push(format!("seed_{token}"));

        let next = graph.create_texture(|b| {
            b.name(format!("seed_view_{token}"))
                .size(crate::render::graph::TargetSize::Exact(
                    view.target_size()[0],
                    view.target_size()[1],
                ))
                .format(frame.surface_format());
        });
        graph.add_render_pass(format!("seed_view_pass_{token}"), |s| {
            s.write_color(0, next);
        });
        state.set_current_color(next, frame.surface_format());
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

struct ObserveViewNode {
    log: Arc<Mutex<Vec<String>>>,
}

impl FrameViewNode for ObserveViewNode {
    fn name(&self) -> &'static str {
        "observe_view"
    }

    fn setup(
        &mut self,
        graph: &mut RenderGraph,
        state: &mut PhaseState,
        _frame: &PreparedFrame<'_>,
        view: &PreparedView<'_>,
    ) {
        let token = view
            .payload::<ViewToken>()
            .expect("observe_view requires a view token")
            .0;
        let current = state
            .current_color()
            .expect("observe_view should see current_color from the previous node");
        self.log.lock().unwrap().push(format!("observe_{token}"));

        let sink = graph.create_texture(|b| {
            b.name(format!("observe_view_{token}"))
                .size(crate::render::graph::TargetSize::Exact(
                    view.target_size()[0],
                    view.target_size()[1],
                ))
                .format(current.format());
        });
        graph.add_render_pass(format!("observe_view_pass_{token}"), |s| {
            s.read(current.handle());
            s.write_color(0, sink);
        });
        state.set_current_color(sink, current.format());
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

struct FinalizeRecorder {
    log: Arc<Mutex<Vec<String>>>,
}

impl FrameFinalizeNode for FinalizeRecorder {
    fn name(&self) -> &'static str {
        "finalize"
    }

    fn setup(
        &mut self,
        graph: &mut RenderGraph,
        state: &mut FinalizePhaseState<'_>,
        frame: &PreparedFrame<'_>,
    ) {
        assert_eq!(frame.payload::<FrameToken>().map(|token| token.0), Some(99));
        assert_eq!(state.completed_views().len(), 2);
        assert!(state
            .completed_views()
            .iter()
            .all(|view| view.slots().current_color().is_some()));
        self.log.lock().unwrap().push("finalize".to_string());

        let final_tex = graph.create_texture(|b| {
            b.name("finalize_sink")
                .size(crate::render::graph::TargetSize::Exact(8, 8))
                .format(frame.surface_format())
                .persistent();
        });
        graph.add_render_pass("finalize_pass", |s| {
            s.write_color(0, final_tex);
        });
        state.set_current_color(final_tex, frame.surface_format());
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

struct CopyUploadNode;

impl FrameSetupNode for CopyUploadNode {
    fn name(&self) -> &'static str {
        "copy_upload"
    }

    fn setup(
        &mut self,
        graph: &mut RenderGraph,
        _state: &mut PhaseState,
        _frame: &PreparedFrame<'_>,
    ) {
        let dst = graph.create_texture(|b| {
            b.name("copy_upload_target")
                .size(crate::render::graph::TargetSize::Exact(1, 1))
                .format(wgpu::TextureFormat::Rgba8Unorm)
                .persistent();
        });
        graph.add_copy_pass("copy_upload", |setup| {
            setup.upload_to_texture(vec![255, 255, 255, 255], dst, 1, 1, 4);
        });
    }

    fn execute(
        &mut self,
        _pass: &CompiledPass,
        _ctx: &mut GpuContext,
        _resources: &PhysicalResources<'_>,
        _execution: &SetupExecutionContext<'_>,
    ) -> Result<(), RenderGraphError> {
        panic!("copy passes should be executed internally by RenderGraph")
    }
}

struct MultiPassDrawCountNode;

impl FrameViewNode for MultiPassDrawCountNode {
    fn name(&self) -> &'static str {
        "multi_pass_draw_count"
    }

    fn setup(
        &mut self,
        graph: &mut RenderGraph,
        _state: &mut PhaseState,
        _frame: &PreparedFrame<'_>,
        view: &PreparedView<'_>,
    ) {
        let intermediate = graph.create_texture(|b| {
            b.name("multi_pass_intermediate")
                .size(crate::render::graph::TargetSize::Exact(
                    view.target_size()[0],
                    view.target_size()[1],
                ))
                .format(wgpu::TextureFormat::Bgra8Unorm);
        });
        let sink = graph.create_texture(|b| {
            b.name("multi_pass_sink")
                .size(crate::render::graph::TargetSize::Exact(
                    view.target_size()[0],
                    view.target_size()[1],
                ))
                .format(wgpu::TextureFormat::Bgra8Unorm)
                .persistent();
        });
        graph.add_render_pass("multi_pass_first", |setup| {
            setup.write_color(0, intermediate);
        });
        graph.add_render_pass("multi_pass_second", |setup| {
            setup.read(intermediate);
            setup.write_color(0, sink);
        });
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

    fn draw_calls(&self, _execution: &ViewExecutionContext<'_>) -> usize {
        1
    }
}

#[test]
fn phases_run_in_setup_view_finalize_order_and_views_sort_by_order() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let frame_token = FrameToken(99);
    let view_token_a = ViewToken(7);
    let view_token_b = ViewToken(3);

    let mut pipeline = FramePipeline::new();
    pipeline.add_setup_node(Box::new(SetupRecorder {
        log: Arc::clone(&log),
    }));
    pipeline.add_view_node(Box::new(SeedViewNode {
        log: Arc::clone(&log),
    }));
    pipeline.add_view_node(Box::new(ObserveViewNode {
        log: Arc::clone(&log),
    }));
    pipeline.add_finalize_node(Box::new(FinalizeRecorder {
        log: Arc::clone(&log),
    }));

    let mut frame = PreparedFrame::new(wgpu::TextureFormat::Bgra8Unorm, false);
    let _ = frame.insert_payload(&frame_token);
    let mut view_a = PreparedView::new(10, ViewportRect::new(0, 0, 8, 8), [8, 8], true);
    let _ = view_a.insert_payload(&view_token_a);
    let mut view_b = PreparedView::new(0, ViewportRect::new(8, 0, 8, 8), [8, 8], false);
    let _ = view_b.insert_payload(&view_token_b);
    frame.add_view(view_a);
    frame.add_view(view_b);

    pipeline.debug_prepare_frame(&frame);

    let log = log.lock().unwrap().clone();
    assert_eq!(
        log,
        vec![
            "setup",
            "seed_3",
            "observe_3",
            "seed_7",
            "observe_7",
            "finalize",
        ]
    );
}

#[test]
fn copy_passes_are_included_in_frame_stats() {
    let (device, queue) = create_test_device();
    let mut ctx = GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [1, 1]);
    let mut pipeline = FramePipeline::new();
    pipeline.add_setup_node(Box::new(CopyUploadNode));

    let frame = PreparedFrame::new(wgpu::TextureFormat::Bgra8Unorm, false);

    ctx.begin_frame()
        .expect("headless begin_frame should succeed");
    let stats = pipeline.execute_frame(&mut ctx, &frame);
    ctx.end_frame();

    assert_eq!(stats.passes, 1);
    assert_eq!(stats.draw_calls, 0);
}

#[test]
fn multi_pass_nodes_count_draw_calls_once_per_dispatch() {
    let (device, queue) = create_test_device();
    let mut ctx = GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [2, 2]);
    let mut pipeline = FramePipeline::new();
    pipeline.add_view_node(Box::new(MultiPassDrawCountNode));

    let mut frame = PreparedFrame::new(wgpu::TextureFormat::Bgra8Unorm, false);
    frame.add_view(PreparedView::new(
        0,
        ViewportRect::new(0, 0, 2, 2),
        [2, 2],
        true,
    ));

    ctx.begin_frame()
        .expect("headless begin_frame should succeed");
    let stats = pipeline.execute_frame(&mut ctx, &frame);
    ctx.end_frame();

    assert_eq!(stats.passes, 2);
    assert_eq!(stats.draw_calls, 1);
}

#[test]
fn scene_gbuffer_slots_do_not_mirror_into_generic_resource_slots() {
    let mut graph = RenderGraph::new();
    let color = graph.create_texture(|b| {
        b.name("scene_color_test")
            .size(crate::render::graph::TargetSize::Exact(4, 4))
            .format(wgpu::TextureFormat::Rgba16Float);
    });
    let depth = graph.create_texture(|b| {
        b.name("scene_depth_test")
            .size(crate::render::graph::TargetSize::Exact(4, 4))
            .format(wgpu::TextureFormat::Depth32Float);
    });

    let mut state = PhaseState::new(wgpu::TextureFormat::Bgra8Unorm, false);
    state.set_scene_color(color, wgpu::TextureFormat::Rgba16Float);
    state.set_scene_depth(depth, wgpu::TextureFormat::Depth32Float);

    assert_eq!(state.scene_color().map(|slot| slot.handle()), Some(color));
    assert_eq!(state.scene_depth().map(|slot| slot.handle()), Some(depth));
    assert_eq!(state.texture_slot("scene_color"), None);
    assert_eq!(state.texture_slot("scene_depth"), None);
}

#[test]
fn scene_texture_allocator_reuses_persistent_slots_and_replaces_transient_ones() {
    let mut graph = RenderGraph::new();
    let mut state = PhaseState::new(wgpu::TextureFormat::Bgra8Unorm, false);

    let first_depth = ensure_scene_texture(
        &mut graph,
        &mut state,
        [8, 8],
        SceneTextureKind::Depth,
        wgpu::TextureFormat::Depth32Float,
        "scene_depth",
    );
    let reused_depth = ensure_scene_texture(
        &mut graph,
        &mut state,
        [8, 8],
        SceneTextureKind::Depth,
        wgpu::TextureFormat::Depth32Float,
        "scene_depth",
    );
    let first_normal = create_scene_texture(
        &mut graph,
        &mut state,
        [8, 8],
        SceneTextureKind::Normal,
        wgpu::TextureFormat::Rgba8Unorm,
        "scene_normal",
    );
    let replaced_normal = create_scene_texture(
        &mut graph,
        &mut state,
        [8, 8],
        SceneTextureKind::Normal,
        wgpu::TextureFormat::Rgba8Unorm,
        "scene_normal",
    );

    assert_eq!(first_depth.handle(), reused_depth.handle());
    assert_ne!(first_normal.handle(), replaced_normal.handle());
    assert_eq!(
        state.scene_depth().map(|slot| slot.handle()),
        Some(reused_depth.handle())
    );
    assert_eq!(
        state.scene_normal().map(|slot| slot.handle()),
        Some(replaced_normal.handle())
    );
}
