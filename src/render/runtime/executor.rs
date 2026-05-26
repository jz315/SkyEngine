use crate::gpu::GpuContext;
use crate::render::execution::{PreparedFrame, RenderServices};
use crate::render::pipeline::PipelineStep;

use super::frame::FrameExecutionSummary;
use super::state::RenderResourceHub;
use super::{elapsed_ms, timing_start, ViewportBlitNode};

pub(crate) struct RenderExecutor {
    viewport_blit: Option<ViewportBlitNode>,
}

impl RenderExecutor {
    #[inline]
    pub(crate) fn new() -> Self {
        Self {
            viewport_blit: None,
        }
    }

    pub(crate) fn execute_prepared_frame(
        &mut self,
        steps: &mut [PipelineStep],
        resources: &mut RenderResourceHub,
        fallback_texture: &crate::render::gpu::Texture,
        gpu: &mut GpuContext,
        frame: &PreparedFrame<'_>,
    ) -> FrameExecutionSummary {
        let viewport_blit = self
            .viewport_blit
            .get_or_insert_with(|| ViewportBlitNode::new(gpu));
        let mut pipeline =
            super::pipeline_runtime::build_runtime_pipeline(steps, gpu, viewport_blit);
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

    #[cfg(test)]
    pub(crate) fn cached_viewport_blit_ptr(&self) -> Option<usize> {
        self.viewport_blit
            .as_ref()
            .map(|node| std::ptr::from_ref(node) as usize)
    }
}

impl Default for RenderExecutor {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::execution::PreparedFrame;
    use crate::render::gpu::Texture;
    use crate::render::phase::DrawFunctionRegistry;
    use crate::render::resources::material::MaterialRegistry;
    use crate::render::resources::mesh::MeshRegistry;

    fn create_test_device() -> (wgpu::Device, wgpu::Queue) {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .expect("No suitable GPU adapter found for executor tests");

        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("executor_test_device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::Performance,
            ..Default::default()
        }))
        .expect("Failed to create test GPU device")
    }

    #[test]
    fn executor_reuses_viewport_blit_node_across_frames() {
        let (device, queue) = create_test_device();
        let mut gpu =
            GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [16, 16]);
        let fallback = Texture::white_pixel(&gpu);
        let mut executor = RenderExecutor::new();
        let mut steps = Vec::new();
        let mut resources = RenderResourceHub {
            draw_functions: DrawFunctionRegistry::new(),
            material_registry: MaterialRegistry::new(),
            mesh_registry: MeshRegistry::default(),
        };
        let frame = PreparedFrame::new(gpu.surface_format(), false);

        executor.execute_prepared_frame(&mut steps, &mut resources, &fallback, &mut gpu, &frame);
        let first = executor
            .cached_viewport_blit_ptr()
            .expect("first frame should create cached viewport blit node");
        executor.execute_prepared_frame(&mut steps, &mut resources, &fallback, &mut gpu, &frame);

        assert_eq!(executor.cached_viewport_blit_ptr(), Some(first));
    }
}
