use crate::gpu::GpuContext;
use crate::render::execution::{PreparedFrame, PreparedView, ViewExecutionContext};
use crate::render::graph::RenderGraphError;

use super::contexts::{
    ComputePassExecuteContext, ComputePassSetupContext, PostFxPassExecuteContext,
    PostFxPassSetupContext, RenderPassExecuteContext, RenderPassSetupContext,
};

pub trait ComputePass: Send + 'static {
    fn name(&self) -> &'static str;

    fn setup(&mut self, _ctx: &mut ComputePassSetupContext<'_, '_>) {}

    fn execute(
        &mut self,
        _ctx: &mut ComputePassExecuteContext<'_, '_>,
    ) -> Result<(), RenderGraphError> {
        Ok(())
    }
}

pub trait RenderPass: Send + 'static {
    fn name(&self) -> &'static str;

    fn setup(&mut self, _ctx: &mut RenderPassSetupContext<'_, '_, '_>) {}

    fn execute(
        &mut self,
        _ctx: &mut RenderPassExecuteContext<'_, '_>,
    ) -> Result<(), RenderGraphError> {
        Ok(())
    }
}

pub trait PostFxPass: Send + 'static {
    fn name(&self) -> &'static str;

    fn is_enabled(&self, _frame: &PreparedFrame<'_>, _view: &PreparedView<'_>) -> bool {
        true
    }

    fn requires_hdr_input(&self) -> bool {
        false
    }

    fn setup(&mut self, _ctx: &mut PostFxPassSetupContext<'_, '_>) {}

    fn execute(
        &mut self,
        _ctx: &mut PostFxPassExecuteContext<'_, '_>,
    ) -> Result<(), RenderGraphError> {
        Ok(())
    }

    fn draw_calls(&self, _execution: &ViewExecutionContext<'_>) -> usize {
        0
    }

    fn resize(&mut self, _ctx: &GpuContext, _width: u32, _height: u32) {}
}
