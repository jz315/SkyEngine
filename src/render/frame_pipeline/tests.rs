use std::sync::{Arc, Mutex};

use crate::gpu::GpuContext;
use crate::render::core::viewport::ViewportRect;
use crate::render::graph::{CompiledPass, PhysicalResources, RenderGraph, RenderGraphError};

use super::{
    FinalizeExecutionContext, FinalizePhaseState, FrameFinalizeNode, FramePipeline, FrameSetupNode,
    FrameViewNode, PhaseState, PreparedFrame, PreparedView, SetupExecutionContext,
    ViewExecutionContext,
};

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
