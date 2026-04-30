//! Compute pipeline helpers.

use std::borrow::Cow;
use std::sync::Arc;

use crate::gpu::GpuContext;

/// A lazily-created compute pipeline with stable bind group layouts.
pub struct ComputePipelineCache {
    shader: Arc<wgpu::ShaderModule>,
    entry: &'static str,
    bind_group_layouts: Vec<wgpu::BindGroupLayout>,
    pipeline: Option<Arc<wgpu::ComputePipeline>>,
    label: Cow<'static, str>,
}

impl ComputePipelineCache {
    pub fn new(
        ctx: &GpuContext,
        source: &str,
        entry: &'static str,
        bind_group_layouts: &[&wgpu::BindGroupLayout],
        label: impl Into<Cow<'static, str>>,
    ) -> Self {
        let label = label.into();
        let shader = ctx
            .device()
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(&format!("{}_shader", label)),
                source: wgpu::ShaderSource::Wgsl(source.into()),
            });
        Self::from_shader_module(ctx, Arc::new(shader), entry, bind_group_layouts, label)
    }

    pub fn from_shader_module(
        _ctx: &GpuContext,
        shader: Arc<wgpu::ShaderModule>,
        entry: &'static str,
        bind_group_layouts: &[&wgpu::BindGroupLayout],
        label: impl Into<Cow<'static, str>>,
    ) -> Self {
        let bind_group_layouts = bind_group_layouts
            .iter()
            .map(|layout| (*layout).clone())
            .collect();
        Self {
            shader,
            entry,
            bind_group_layouts,
            pipeline: None,
            label: label.into(),
        }
    }

    fn create_pipeline(&self, ctx: &GpuContext) -> wgpu::ComputePipeline {
        let bind_group_layout_refs: Vec<&wgpu::BindGroupLayout> =
            self.bind_group_layouts.iter().collect();
        let pipeline_layout =
            ctx.device()
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some(&format!("{}_layout", self.label)),
                    bind_group_layouts: &bind_group_layout_refs,
                    push_constant_ranges: &[],
                });
        ctx.device()
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(&format!("{}_pipeline", self.label)),
                layout: Some(&pipeline_layout),
                module: &self.shader,
                entry_point: Some(self.entry),
                compilation_options: Default::default(),
                cache: None,
            })
    }

    pub fn pipeline(&mut self, ctx: &GpuContext) -> Arc<wgpu::ComputePipeline> {
        if let Some(existing) = self.pipeline.as_ref() {
            return existing.clone();
        }
        let pipeline = Arc::new(self.create_pipeline(ctx));
        self.pipeline = Some(pipeline.clone());
        pipeline
    }

    #[cfg(test)]
    fn pipeline_count(&self) -> usize {
        usize::from(self.pipeline.is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_device() -> (wgpu::Device, wgpu::Queue) {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .expect("No suitable GPU adapter found for render tests");

        pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("render_compute_test_device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
            },
            None,
        ))
        .expect("Failed to create test GPU device")
    }

    #[test]
    fn compute_pipeline_cache_reuses_pipeline() {
        let (device, queue) = create_test_device();
        let ctx = crate::gpu::GpuContext::new_headless(
            device,
            queue,
            wgpu::TextureFormat::Bgra8Unorm,
            [4, 4],
        );
        let bgl = ctx
            .device()
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("compute_test_bgl"),
                entries: &[],
            });

        let mut cache = ComputePipelineCache::new(
            &ctx,
            "@compute @workgroup_size(1) fn cs_main() {}",
            "cs_main",
            &[&bgl],
            "compute_test",
        );

        assert_eq!(cache.pipeline_count(), 0);
        let first = cache.pipeline(&ctx);
        assert_eq!(cache.pipeline_count(), 1);
        let second = cache.pipeline(&ctx);
        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(cache.pipeline_count(), 1);
    }
}
