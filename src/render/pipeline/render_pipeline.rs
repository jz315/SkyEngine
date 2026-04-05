//! Composable render pipeline built from [`RenderFeature2D`] features.

use crate::gpu::GpuContext;
use crate::render::ecs::RenderSettings2D;
use crate::render::gpu_scene2d::GpuScene2D;
use crate::render::graph::RenderGraph;
use crate::render::pipeline::{
    BloomNode, ColorResolveNode, CompositeNode, FeatureExecutionContext2D, FramePipelineState2D,
    LightNode, PreparedView2D, RenderFeature2D, SpritePass, ToneMapNode, ViewportBlitNode,
    VignetteNode,
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct PipelineExecutionStats2D {
    pub passes: usize,
    pub draw_calls: usize,
}

/// A composable 2D rendering pipeline.
pub struct RenderPipeline {
    features: Vec<Box<dyn RenderFeature2D>>,
    graph: RenderGraph,
    frame_state: Option<FramePipelineState2D>,
    #[cfg(test)]
    execute_count: usize,
    #[cfg(test)]
    last_executed_pass_names: Vec<String>,
}

impl RenderPipeline {
    /// Create an empty pipeline.
    pub fn new() -> Self {
        Self {
            features: Vec::new(),
            graph: RenderGraph::new(),
            frame_state: None,
            #[cfg(test)]
            execute_count: 0,
            #[cfg(test)]
            last_executed_pass_names: Vec::new(),
        }
    }

    /// Full lit HDR preset: sprites → lights → composite → bloom → vignette →
    /// tone map or resolve → blit.
    pub fn lit_hdr(ctx: &GpuContext) -> Self {
        let mut p = Self::new();
        p.add(Box::new(SpritePass::hdr(ctx)));
        p.add(Box::new(LightNode::new(ctx)));
        p.add(Box::new(CompositeNode::new(ctx)));
        p.add(Box::new(BloomNode::new(ctx)));
        p.add(Box::new(VignetteNode::new(ctx)));
        p.add(Box::new(ToneMapNode::new(ctx)));
        p.add(Box::new(ColorResolveNode::new(ctx)));
        p.add(Box::new(ViewportBlitNode::new(ctx)));
        p
    }

    /// Sprites-only preset.
    pub fn unlit(ctx: &GpuContext) -> Self {
        let mut p = Self::new();
        p.add(Box::new(SpritePass::surface(ctx)));
        p.add(Box::new(ViewportBlitNode::new(ctx)));
        p
    }

    /// Add a feature to the end of the pipeline.
    pub fn add(&mut self, feature: Box<dyn RenderFeature2D>) -> &mut Self {
        self.features.push(feature);
        self
    }

    /// Legacy entry point retained as a no-op because graphs are rebuilt per frame.
    pub fn rebuild_if_needed(
        &mut self,
        _settings: &RenderSettings2D,
        _surface_format: wgpu::TextureFormat,
    ) {
    }

    /// Apply render settings to all features.
    pub fn apply_settings(&mut self, settings: &RenderSettings2D) {
        for feature in &mut self.features {
            feature.apply_settings(settings);
        }
    }

    /// Resize all features.
    pub fn resize(&mut self, ctx: &GpuContext, width: u32, height: u32) {
        for feature in &mut self.features {
            feature.resize(ctx, width, height);
        }
    }

    pub(crate) fn begin_frame(
        &mut self,
        settings: &RenderSettings2D,
        surface_format: wgpu::TextureFormat,
        has_surface: bool,
    ) {
        self.apply_settings(settings);
        self.graph.clear_frame();
        self.frame_state = Some(FramePipelineState2D::new(
            *settings,
            surface_format,
            has_surface,
        ));
        #[cfg(test)]
        {
            self.last_executed_pass_names.clear();
        }
    }

    pub(crate) fn enqueue_view(&mut self, view: PreparedView2D, clear_surface: bool) {
        let frame_state = self
            .frame_state
            .as_mut()
            .expect("RenderPipeline::begin_frame must be called before enqueue_view");
        let view_index = frame_state.push_view(view, clear_surface);
        let settings = *frame_state.settings();
        let has_surface = frame_state.has_surface();

        for (feature_index, feature) in self.features.iter_mut().enumerate() {
            if !feature.is_enabled(&settings, has_surface) {
                continue;
            }

            let pass_count_before = self.graph.pass_count();
            feature.setup(&mut self.graph, frame_state.view_state_mut(view_index));
            let new_passes = self.graph.pass_handles_from(pass_count_before);
            frame_state.register_passes(&new_passes, feature_index, view_index);
        }
    }

    pub(crate) fn execute_frame(
        &mut self,
        ctx: &mut GpuContext,
        gpu_scene: &GpuScene2D,
    ) -> PipelineExecutionStats2D {
        let frame_state = self
            .frame_state
            .take()
            .expect("RenderPipeline::begin_frame must be called before execute_frame");
        let features = &mut self.features;
        let graph = &mut self.graph;
        let mut stats = PipelineExecutionStats2D::default();
        #[cfg(test)]
        let last_executed_pass_names = &mut self.last_executed_pass_names;

        graph.execute(ctx, |compiled_pass, ctx, resources| {
            let Some(dispatch) = frame_state.dispatch_entry(compiled_pass.handle) else {
                return Ok(());
            };

            stats.passes += 1;
            #[cfg(test)]
            {
                last_executed_pass_names.push(compiled_pass.name.to_string());
            }

            let execution: FeatureExecutionContext2D<'_> =
                frame_state.execution_context(dispatch.view_index, gpu_scene);
            stats.draw_calls += features[dispatch.feature_index].draw_calls(&execution);
            features[dispatch.feature_index].execute(compiled_pass, ctx, resources, &execution)?;
            Ok(())
        });

        #[cfg(test)]
        {
            self.execute_count += 1;
        }

        stats
    }

    /// Number of features currently in the pipeline.
    pub fn pass_count(&self) -> usize {
        self.features.len()
    }

    #[cfg(test)]
    pub(crate) fn execute_count(&self) -> usize {
        self.execute_count
    }

    #[cfg(test)]
    pub(crate) fn last_light_ambient(&self) -> Option<[f32; 4]> {
        self.features
            .iter()
            .find_map(|feature| feature.debug_last_light_ambient())
    }
}

impl Default for RenderPipeline {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::ecs::{RenderSettings2D, ToneMapSettings, ViewportRect};
    use crate::render::graph::{
        CompiledPass, LoadOp, PhysicalResources, RenderGraph, RenderGraphError, TargetSize,
    };
    use crate::render::pipeline::{
        PipelineState2D, PreparedRenderWorld2D, RenderFeature2D, SceneCache2D, SceneView2D,
    };
    use crate::render::{Camera2D, Color, RenderView2D};

    fn create_test_device() -> (wgpu::Device, wgpu::Queue) {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .expect("No suitable GPU adapter found for pipeline tests");

        pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("render_pipeline_test_device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
            },
            None,
        ))
        .expect("Failed to create test GPU device")
    }

    fn test_view(x: u32, y: u32, width: u32, height: u32, order: i32) -> PreparedView2D {
        PreparedView2D {
            order,
            viewport: ViewportRect::new(x, y, width, height),
            layer_mask: u32::MAX,
            camera: crate::render::Camera2D::new(width as f32, height as f32),
            sprite_offset: 0,
            sprite_count: 0,
            draw_span_offset: 0,
            draw_span_count: 0,
            light_offset: 0,
            light_count: 0,
        }
    }

    #[derive(Default)]
    struct KeepAliveNode;

    impl RenderFeature2D for KeepAliveNode {
        fn name(&self) -> &'static str {
            "keep_alive"
        }

        fn setup(&mut self, graph: &mut RenderGraph, state: &mut PipelineState2D) {
            let input = state
                .current()
                .expect("KeepAliveNode requires current input");
            let sink = graph.create_texture(|b| {
                b.name("keep_alive_sink")
                    .size(TargetSize::Exact(
                        state.view_size()[0],
                        state.view_size()[1],
                    ))
                    .format(state.surface_format())
                    .persistent();
            });
            graph.add_render_pass("keep_alive", |s| {
                s.read(input);
                s.write_color(0, sink);
            });
            state.set_current(sink);
        }

        fn execute(
            &mut self,
            _pass: &CompiledPass,
            _ctx: &mut GpuContext,
            _resources: &PhysicalResources<'_>,
            _execution: &FeatureExecutionContext2D<'_>,
        ) -> Result<(), RenderGraphError> {
            Ok(())
        }

        fn draw_calls(&self, _execution: &FeatureExecutionContext2D<'_>) -> usize {
            0
        }
    }

    #[test]
    fn lit_hdr_registers_color_resolve_when_tonemap_disabled() {
        let (device, queue) = create_test_device();
        let ctx =
            GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [64, 64]);
        let mut pipeline = RenderPipeline::lit_hdr(&ctx);
        let settings = RenderSettings2D {
            tonemap: ToneMapSettings {
                enabled: false,
                ..Default::default()
            },
            ..RenderSettings2D::default()
        };

        pipeline.begin_frame(&settings, ctx.surface_format(), false);
        pipeline.enqueue_view(test_view(0, 0, 64, 64, 0), true);

        let names = pipeline.graph.debug_declared_pass_names();
        assert!(names.iter().any(|name| name == "color_resolve"));
        assert!(!names.iter().any(|name| name == "tonemap"));
    }

    #[test]
    fn unlit_pipeline_does_not_register_tonemap_or_resolve() {
        let (device, queue) = create_test_device();
        let ctx =
            GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [64, 64]);
        let mut pipeline = RenderPipeline::unlit(&ctx);
        let settings = RenderSettings2D::default();

        pipeline.begin_frame(&settings, ctx.surface_format(), true);
        pipeline.enqueue_view(test_view(0, 0, 64, 64, 0), true);

        let names = pipeline.graph.debug_declared_pass_names();
        assert!(!names.iter().any(|name| name == "tonemap"));
        assert!(!names.iter().any(|name| name == "color_resolve"));
    }

    #[test]
    fn later_views_use_surface_load_instead_of_clear() {
        let (device, queue) = create_test_device();
        let ctx =
            GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [64, 64]);
        let mut pipeline = RenderPipeline::unlit(&ctx);
        let settings = RenderSettings2D::default();

        pipeline.begin_frame(&settings, ctx.surface_format(), true);
        pipeline.enqueue_view(test_view(0, 0, 32, 64, 0), true);
        pipeline.enqueue_view(test_view(32, 0, 32, 64, 1), false);

        let loads = pipeline.graph.debug_declared_surface_loads("viewport_blit");
        assert_eq!(loads.len(), 2);
        assert!(matches!(loads[0], LoadOp::Clear(_)));
        assert!(matches!(loads[1], LoadOp::Load));
    }

    #[test]
    fn light_node_uses_ambient_color_instead_of_clear_color() {
        let (device, queue) = create_test_device();
        let mut ctx =
            GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [32, 32]);
        let mut pipeline = RenderPipeline::new();
        pipeline.add(Box::new(SpritePass::hdr(&ctx)));
        pipeline.add(Box::new(LightNode::new(&ctx)));
        pipeline.add(Box::new(CompositeNode::new(&ctx)));
        pipeline.add(Box::new(ColorResolveNode::new(&ctx)));
        pipeline.add(Box::new(KeepAliveNode));

        let mut scene = SceneCache2D::new();
        scene.settings = RenderSettings2D {
            clear_color: Color::RED,
            ambient_color: Color::GREEN,
            tonemap: ToneMapSettings {
                enabled: false,
                ..Default::default()
            },
            ..RenderSettings2D::default()
        };
        scene.views.push(SceneView2D::new(
            Camera2D::new(32.0, 32.0),
            RenderView2D::new(ViewportRect::new(0, 0, 32, 32)),
        ));

        let mut prepared = PreparedRenderWorld2D::new();
        prepared.prepare_scene(&mut scene, [32, 32]);
        let mut gpu_scene = GpuScene2D::new(&ctx);
        gpu_scene.upload_scene_frame(&ctx, &mut scene, &prepared);

        ctx.begin_frame()
            .expect("headless begin_frame should succeed");
        pipeline.begin_frame(&scene.settings, ctx.surface_format(), false);
        pipeline.enqueue_view(gpu_scene.views()[0], true);
        let _ = pipeline.execute_frame(&mut ctx, &gpu_scene);
        ctx.end_frame();

        assert_eq!(pipeline.last_light_ambient(), Some(Color::GREEN.to_array()));
        assert_ne!(pipeline.last_light_ambient(), Some(Color::RED.to_array()));
    }
}
