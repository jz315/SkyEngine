use crate::gpu::GpuContext;
use crate::render::execution::{FramePipelineCache, PreparedFrame, RenderServices};
use crate::render::pipeline::PipelineStep;

use super::frame::FrameExecutionSummary;
use super::state::RenderResourceHub;
use super::{elapsed_ms, timing_start, ViewportBlitNode};

pub(crate) struct RenderExecutor {
    viewport_blit: Option<ViewportBlitNode>,
    pipeline_cache: Option<FramePipelineCache>,
}

impl RenderExecutor {
    #[inline]
    pub(crate) fn new() -> Self {
        Self {
            viewport_blit: None,
            pipeline_cache: Some(FramePipelineCache::default()),
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
        let cache = self.pipeline_cache.take().unwrap_or_default();
        let mut pipeline =
            super::pipeline_runtime::build_runtime_pipeline(steps, gpu, viewport_blit, cache);
        let mut render_services = RenderServices::new(
            &mut resources.draw_functions,
            &mut resources.material_registry,
            &resources.mesh_registry,
            fallback_texture,
        );
        let execute_start = timing_start();
        #[cfg(feature = "profile")]
        let _scope = sky_profile::profile_scope!("render", "FramePipeline::execute_frame");
        let result = pipeline.try_execute_frame_with_services(gpu, frame, &mut render_services);
        let mut cache = pipeline.into_cache();
        if result.is_err() {
            cache.invalidate_gpu_resources();
        }
        self.pipeline_cache = Some(cache);
        let execute_ms = elapsed_ms(execute_start);
        match result {
            Ok(stats) => FrameExecutionSummary {
                stats,
                execute_ms,
                succeeded: true,
            },
            Err(error) => {
                log::error!(
                    target: "sky_engine::render::graph",
                    "frame render graph failed; acquired frame will be cleared: {error}"
                );
                FrameExecutionSummary {
                    stats: Default::default(),
                    execute_ms,
                    succeeded: false,
                }
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn cached_viewport_blit_ptr(&self) -> Option<usize> {
        self.viewport_blit
            .as_ref()
            .map(|node| std::ptr::from_ref(node) as usize)
    }

    #[cfg(test)]
    pub(crate) fn cached_transient_texture_ptr(&self) -> Option<usize> {
        self.pipeline_cache
            .as_ref()
            .and_then(FramePipelineCache::cached_transient_texture_ptr)
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
    use crate::render::execution::{PreparedFrame, PreparedView};
    use crate::render::gpu::Texture;
    use crate::render::phase::DrawFunctionRegistry;
    use crate::render::resources::material::MaterialRegistry;
    use crate::render::resources::mesh::MeshRegistry;
    use crate::render::view::ViewportRect;

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
        let mut frame = PreparedFrame::new(gpu.surface_format(), false);
        frame.add_view(PreparedView::new(
            0,
            ViewportRect::new(0, 0, 16, 16),
            [16, 16],
            true,
        ));

        gpu.begin_frame()
            .expect("first headless frame should begin");
        executor.execute_prepared_frame(&mut steps, &mut resources, &fallback, &mut gpu, &frame);
        gpu.end_frame();
        let first = executor
            .cached_viewport_blit_ptr()
            .expect("first frame should create cached viewport blit node");
        let first_texture = executor
            .cached_transient_texture_ptr()
            .expect("first frame should cache its transient scene target");

        gpu.begin_frame()
            .expect("second headless frame should begin");
        executor.execute_prepared_frame(&mut steps, &mut resources, &fallback, &mut gpu, &frame);
        gpu.end_frame();

        assert_eq!(executor.cached_viewport_blit_ptr(), Some(first));
        assert_eq!(
            executor.cached_transient_texture_ptr(),
            Some(first_texture),
            "runtime frames should reuse the same pooled scene target"
        );
    }
}
