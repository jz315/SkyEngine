use crate::gpu::GpuContext;
use crate::render::execution::{PreparedFrame, RenderServices};
use crate::render::pipeline::PipelineStep;

use super::frame::FrameExecutionSummary;
use super::state::RenderResourceHub;
use super::{elapsed_ms, timing_start};

pub(crate) struct RenderExecutor;

impl RenderExecutor {
    #[inline]
    pub(crate) fn new() -> Self {
        Self
    }

    pub(crate) fn execute_prepared_frame(
        &mut self,
        steps: &mut [PipelineStep],
        resources: &mut RenderResourceHub,
        fallback_texture: &crate::render::gpu::Texture,
        gpu: &mut GpuContext,
        frame: &PreparedFrame<'_>,
    ) -> FrameExecutionSummary {
        let mut pipeline = super::pipeline_runtime::build_runtime_pipeline(steps, gpu);
        let mut render_services = RenderServices::new(
            &mut resources.draw_functions,
            &mut resources.material_registry,
            &resources.mesh_registry,
            fallback_texture,
        );
        let execute_start = timing_start();
        let stats = pipeline.execute_frame_with_services(gpu, frame, &mut render_services);
        let execute_ms = elapsed_ms(execute_start);
        FrameExecutionSummary { stats, execute_ms }
    }
}

impl Default for RenderExecutor {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}
