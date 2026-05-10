use crate::render::execution::{
    ComputePassExecuteContext, ComputePassSetupContext, PostFxPassExecuteContext,
    PostFxPassSetupContext,
};
use crate::render::execution::{PreparedFrame, PreparedView, ViewExecutionContext};
use crate::render::gi::GiRuntime;
use crate::render::graph::RenderGraphError;
use crate::render::pipeline::{ComputePass, PostFxPass};
use crate::render::SceneView;

#[derive(Default)]
pub struct GiUpdateCompute;

impl GiUpdateCompute {
    fn is_first_lit_view(frame: &PreparedFrame<'_>, view: &PreparedView<'_>) -> bool {
        frame
            .views()
            .iter()
            .find(|candidate| {
                candidate
                    .payload::<SceneView>()
                    .is_some_and(|scene_view| !scene_view.is_shadow())
            })
            .is_some_and(|candidate| std::ptr::eq(candidate, view))
    }
}

impl ComputePass for GiUpdateCompute {
    fn name(&self) -> &'static str {
        "gi_update"
    }

    fn setup(&mut self, ctx: &mut ComputePassSetupContext<'_, '_>) {
        if !Self::is_first_lit_view(ctx.frame(), ctx.view()) {
            return;
        }
        let Some(gi) = ctx.frame_payload::<GiRuntime>() else {
            return;
        };
        let Some(descriptor) = gi.update_descriptor() else {
            return;
        };
        let marker = ctx.graph().create_buffer(|builder| {
            builder
                .name("gi_update_marker")
                .size(4)
                .usage(wgpu::BufferUsages::COPY_DST)
                .persistent();
        });
        ctx.graph().add_compute_pass(descriptor.label, |setup| {
            setup.write_buffer(marker);
            setup.with_flags(descriptor.flags);
        });
    }

    fn execute(
        &mut self,
        ctx: &mut ComputePassExecuteContext<'_, '_>,
    ) -> Result<(), RenderGraphError> {
        let execution = ctx.execution();
        if !Self::is_first_lit_view(execution.frame(), execution.view()) {
            return Ok(());
        }
        let Some(gi) = execution.frame_payload::<GiRuntime>() else {
            return Ok(());
        };
        gi.update(ctx)
    }
}

#[derive(Default)]
pub struct GiCompositePass;

impl PostFxPass for GiCompositePass {
    fn name(&self) -> &'static str {
        "gi_composite"
    }

    fn is_enabled(&self, frame: &PreparedFrame<'_>, view: &PreparedView<'_>) -> bool {
        if view
            .payload::<SceneView>()
            .is_some_and(SceneView::is_shadow)
        {
            return false;
        }
        frame
            .payload::<GiRuntime>()
            .is_some_and(|gi| gi.composite_descriptor().is_some())
    }

    fn requires_hdr_input(&self) -> bool {
        true
    }

    fn setup(&mut self, ctx: &mut PostFxPassSetupContext<'_, '_>) {
        let Some(gi) = ctx.frame_payload::<GiRuntime>() else {
            return;
        };
        if gi.composite_descriptor().is_none() {
            return;
        }
        gi.setup_composite(ctx);
    }

    fn execute(
        &mut self,
        ctx: &mut PostFxPassExecuteContext<'_, '_>,
    ) -> Result<(), RenderGraphError> {
        let execution = ctx.execution();
        let Some(gi) = execution.frame_payload::<GiRuntime>() else {
            return Ok(());
        };
        if gi.composite_descriptor().is_none() {
            return Ok(());
        }
        gi.execute_composite(ctx)
    }

    fn draw_calls(&self, execution: &ViewExecutionContext<'_>) -> usize {
        if execution
            .frame_payload::<GiRuntime>()
            .is_some_and(|gi| gi.composite_descriptor().is_some())
        {
            1
        } else {
            0
        }
    }

    fn resize(&mut self, ctx: &crate::gpu::GpuContext, width: u32, height: u32) {
        let _ = (ctx, width, height);
    }
}
