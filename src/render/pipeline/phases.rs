use crate::gpu::GpuContext;
use crate::render::execution::{PreparedFrame, PreparedView, ViewExecutionContext};
use crate::render::graph::RenderGraphError;

use super::contexts::{RenderPhaseExecuteContext, RenderPhaseSetupContext};

pub trait RenderPhase: Send + 'static {
    fn name(&self) -> &'static str;

    fn is_enabled(&self, _frame: &PreparedFrame<'_>, _view: &PreparedView<'_>) -> bool {
        true
    }

    fn setup(&mut self, _ctx: &mut RenderPhaseSetupContext<'_, '_>) {}

    fn execute(
        &mut self,
        ctx: &mut RenderPhaseExecuteContext<'_, '_, '_>,
    ) -> Result<(), RenderGraphError>;

    fn draw_calls(&self, _execution: &ViewExecutionContext<'_>) -> usize {
        0
    }

    fn resize(&mut self, _ctx: &GpuContext, _width: u32, _height: u32) {}
}
