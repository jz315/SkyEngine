//! Fullscreen pass helpers.

use std::borrow::Cow;
use std::sync::Arc;

use rustc_hash::FxHashMap;

use crate::gpu::GpuContext;

const FULLSCREEN_COMMON: &str = include_str!("../shaders/common/fullscreen.wgsl");

/// Stateless fullscreen triangle drawer.
pub struct FullscreenPass;

impl FullscreenPass {
    #[inline]
    pub fn draw(pass: &mut wgpu::RenderPass<'_>) {
        pass.draw(0..3, 0..1);
    }
}

/// A compiled fullscreen pipeline with per-format pipeline caching.
pub struct FullscreenPipeline {
    shader: Arc<wgpu::ShaderModule>,
    fs_entry: &'static str,
    bind_group_layouts: Vec<wgpu::BindGroupLayout>,
    pipelines: FxHashMap<wgpu::TextureFormat, Arc<wgpu::RenderPipeline>>,
    blend: Option<wgpu::BlendState>,
    label: Cow<'static, str>,
}

impl FullscreenPipeline {
    fn create_pipeline(
        &self,
        ctx: &GpuContext,
        target_format: wgpu::TextureFormat,
    ) -> wgpu::RenderPipeline {
        let bind_group_layout_refs: Vec<Option<&wgpu::BindGroupLayout>> =
            self.bind_group_layouts.iter().map(Some).collect();

        let pipeline_layout =
            ctx.device()
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some(&format!("{}_layout", self.label)),
                    bind_group_layouts: &bind_group_layout_refs,
                    immediate_size: 0,
                });

        ctx.device()
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(&format!("{}_pipeline_{target_format:?}", self.label)),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &self.shader,
                    entry_point: Some("vs_fullscreen"),
                    buffers: &[],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &self.shader,
                    entry_point: Some(self.fs_entry),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: target_format,
                        blend: self.blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
    }

    pub fn new(
        ctx: &GpuContext,
        fragment_source: &str,
        fs_entry: &'static str,
        bind_group_layouts: &[&wgpu::BindGroupLayout],
        target_format: wgpu::TextureFormat,
        blend: Option<wgpu::BlendState>,
        label: impl Into<Cow<'static, str>>,
    ) -> Self {
        let label = label.into();
        let shader = ctx
            .device()
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(&format!("{}_shader", label)),
                source: wgpu::ShaderSource::Wgsl(compose_fullscreen_shader(fragment_source)),
            });

        let bind_group_layouts: Vec<wgpu::BindGroupLayout> = bind_group_layouts
            .iter()
            .map(|layout| (*layout).clone())
            .collect();
        let mut this = Self {
            shader: Arc::new(shader),
            fs_entry,
            bind_group_layouts,
            pipelines: FxHashMap::default(),
            blend,
            label,
        };
        let pipeline = Arc::new(this.create_pipeline(ctx, target_format));
        this.pipelines.insert(target_format, pipeline);
        this
    }

    /// The compiled render pipeline.
    pub fn pipeline(
        &mut self,
        ctx: &GpuContext,
        target_format: wgpu::TextureFormat,
    ) -> Arc<wgpu::RenderPipeline> {
        if let Some(existing) = self.pipelines.get(&target_format) {
            return existing.clone();
        }
        let pipeline = Arc::new(self.create_pipeline(ctx, target_format));
        self.pipelines.insert(target_format, pipeline.clone());
        pipeline
    }

    #[cfg(test)]
    fn pipeline_count(&self) -> usize {
        self.pipelines.len()
    }
}

pub fn compose_fullscreen_shader(fragment_source: &str) -> Cow<'static, str> {
    Cow::Owned(format!("{FULLSCREEN_COMMON}\n{fragment_source}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_device() -> (wgpu::Device, wgpu::Queue) {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .expect("No suitable GPU adapter found for render tests");

        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("render_test_device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::Performance,
            ..Default::default()
        }))
        .expect("Failed to create test GPU device")
    }

    #[test]
    fn fullscreen_pipeline_caches_per_target_format() {
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
                label: Some("fullscreen_test_bgl"),
                entries: &[],
            });

        let mut pipeline = FullscreenPipeline::new(
            &ctx,
            "@fragment fn fs_main(in: FullscreenOutput) -> @location(0) vec4<f32> { return vec4<f32>(in.uv, 0.0, 1.0); }",
            "fs_main",
            &[&bgl],
            wgpu::TextureFormat::Rgba8Unorm,
            None,
            "fullscreen_test",
        );

        assert_eq!(pipeline.pipeline_count(), 1);
        let _ = pipeline.pipeline(&ctx, wgpu::TextureFormat::Rgba16Float);
        assert_eq!(pipeline.pipeline_count(), 2);
    }
}
