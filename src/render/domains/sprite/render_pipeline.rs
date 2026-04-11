use crate::gpu::GpuContext;
use crate::render::frame_pipeline::{
    FrameExecutionStats, FramePipeline, FrameViewNode, PreparedFrame,
};
use crate::render::output_chain::{
    BloomNode, ColorResolveNode, ToneMapNode, ViewportBlitNode, VignetteNode,
};

use super::{SpriteCompositeNode, SpriteLightNode, SpriteSceneNode};

/// Expert-facing sprite-domain adapter layered on top of
/// [`crate::render::frame_pipeline::FramePipeline`].
pub struct SpriteFramePipeline {
    frame_pipeline: FramePipeline,
}

impl SpriteFramePipeline {
    /// Create an empty sprite frame pipeline.
    pub fn new() -> Self {
        Self {
            frame_pipeline: FramePipeline::new(),
        }
    }

    /// Full lit HDR preset: sprites -> lights -> composite -> bloom -> vignette ->
    /// tone map or resolve -> blit.
    pub fn lit_hdr(ctx: &GpuContext) -> Self {
        let mut pipeline = Self::new();
        pipeline.add(Box::new(SpriteSceneNode::hdr(ctx)));
        pipeline.add(Box::new(SpriteLightNode::new(ctx)));
        pipeline.add(Box::new(SpriteCompositeNode::new(ctx)));
        pipeline.add(Box::new(BloomNode::new(ctx)));
        pipeline.add(Box::new(VignetteNode::new(ctx)));
        pipeline.add(Box::new(ToneMapNode::new(ctx)));
        pipeline.add(Box::new(ColorResolveNode::new(ctx)));
        pipeline.add(Box::new(ViewportBlitNode::new(ctx)));
        pipeline
    }

    /// Sprites-only preset.
    pub fn unlit(ctx: &GpuContext) -> Self {
        let mut pipeline = Self::new();
        pipeline.add(Box::new(SpriteSceneNode::surface(ctx)));
        pipeline.add(Box::new(ViewportBlitNode::new(ctx)));
        pipeline
    }

    /// Add a frame view node to the sprite-domain view phase.
    pub fn add(&mut self, node: Box<dyn FrameViewNode>) -> &mut Self {
        self.frame_pipeline.add_view_node(node);
        self
    }

    /// Resize all view-node resources.
    pub fn resize(&mut self, ctx: &GpuContext, width: u32, height: u32) {
        self.frame_pipeline.resize(ctx, width, height);
    }

    /// Execute one prepared frame.
    pub fn execute_frame(
        &mut self,
        ctx: &mut GpuContext,
        frame: &PreparedFrame<'_>,
    ) -> FrameExecutionStats {
        self.frame_pipeline.execute_frame(ctx, frame)
    }

    /// Number of registered view nodes.
    pub fn pass_count(&self) -> usize {
        self.frame_pipeline.view_node_count()
    }

    #[cfg(test)]
    pub(crate) fn last_light_ambient(&self) -> Option<[f32; 4]> {
        self.frame_pipeline.debug_last_light_ambient()
    }

    #[cfg(test)]
    pub(crate) fn debug_declared_pass_names(&self) -> Vec<String> {
        self.frame_pipeline.graph_debug_declared_pass_names()
    }

    #[cfg(test)]
    pub(crate) fn debug_declared_surface_loads(
        &self,
        pass_name: &str,
    ) -> Vec<crate::render::graph::LoadOp> {
        self.frame_pipeline
            .graph_debug_declared_surface_loads(pass_name)
    }

    #[cfg(test)]
    pub(crate) fn debug_prepare_frame(&mut self, frame: &PreparedFrame<'_>) {
        self.frame_pipeline.debug_prepare_frame(frame);
    }
}

impl Default for SpriteFramePipeline {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::domains::sprite::GpuScene2D;
    use crate::render::domains::sprite::{PreparedRenderWorld2D, PreparedView2D, SceneCache2D};
    use crate::render::ecs::ToneMapSettings;
    use crate::render::frame_pipeline::{
        FrameViewNode, PhaseState, PreparedView, ViewExecutionContext,
    };
    use crate::render::graph::{
        CompiledPass, PhysicalResources, RenderGraph, RenderGraphError, TargetSize,
    };
    use crate::render::{Camera2D, Color, RenderSettings, ViewportRect};

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
                label: Some("sprite_frame_pipeline_test_device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
            },
            None,
        ))
        .expect("Failed to create test GPU device")
    }

    fn test_view(x: u32, y: u32, width: u32, height: u32, order: i32) -> PreparedView2D {
        let camera = Camera2D::new(width as f32, height as f32);
        PreparedView2D {
            order,
            viewport: ViewportRect::new(x, y, width, height),
            layer_mask: u32::MAX,
            view_uniform: camera.uniform(),
            cull_camera_2d: Some(camera),
            sprite_offset: 0,
            sprite_count: 0,
            draw_span_offset: 0,
            draw_span_count: 0,
            light_offset: 0,
            light_count: 0,
        }
    }

    struct KeepAliveNode;

    impl FrameViewNode for KeepAliveNode {
        fn name(&self) -> &'static str {
            "keep_alive"
        }

        fn setup(
            &mut self,
            graph: &mut RenderGraph,
            state: &mut PhaseState,
            _frame: &PreparedFrame<'_>,
            view: &PreparedView<'_>,
        ) {
            let input = state
                .current_color()
                .expect("KeepAliveNode requires current input");
            let sink = graph.create_texture(|b| {
                b.name("keep_alive_sink")
                    .size(TargetSize::Exact(
                        view.target_size()[0],
                        view.target_size()[1],
                    ))
                    .format(input.format())
                    .persistent();
            });
            graph.add_render_pass("keep_alive", |s| {
                s.read(input.handle());
                s.write_color(0, sink);
            });
            state.set_current_color(sink, input.format());
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

    #[test]
    fn lit_hdr_registers_color_resolve_when_tonemap_disabled() {
        let (device, queue) = create_test_device();
        let ctx =
            GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [64, 64]);
        let mut pipeline = SpriteFramePipeline::lit_hdr(&ctx);
        let settings = RenderSettings {
            tonemap: ToneMapSettings {
                enabled: false,
                ..Default::default()
            },
            ..RenderSettings::default()
        };
        let gpu_scene = GpuScene2D::new(&ctx);
        let view_payload = test_view(0, 0, 64, 64, 0);
        let mut frame = PreparedFrame::new(ctx.surface_format(), false);
        let _ = frame.insert_payload(&settings);
        let _ = frame.insert_payload(&gpu_scene);
        let mut view =
            PreparedView::new(0, view_payload.viewport, view_payload.viewport.size(), true);
        let _ = view.insert_payload(&view_payload);
        frame.add_view(view);

        pipeline.debug_prepare_frame(&frame);

        let names = pipeline.debug_declared_pass_names();
        assert!(names.iter().any(|name| name == "color_resolve"));
        assert!(!names.iter().any(|name| name == "tonemap"));
    }

    #[test]
    fn unlit_pipeline_does_not_register_tonemap_or_resolve() {
        let (device, queue) = create_test_device();
        let ctx =
            GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [64, 64]);
        let mut pipeline = SpriteFramePipeline::unlit(&ctx);
        let settings = RenderSettings::default();
        let gpu_scene = GpuScene2D::new(&ctx);
        let view_payload = test_view(0, 0, 64, 64, 0);
        let mut frame = PreparedFrame::new(ctx.surface_format(), true);
        let _ = frame.insert_payload(&settings);
        let _ = frame.insert_payload(&gpu_scene);
        let mut view =
            PreparedView::new(0, view_payload.viewport, view_payload.viewport.size(), true);
        let _ = view.insert_payload(&view_payload);
        frame.add_view(view);

        pipeline.debug_prepare_frame(&frame);

        let names = pipeline.debug_declared_pass_names();
        assert!(!names.iter().any(|name| name == "tonemap"));
        assert!(!names.iter().any(|name| name == "color_resolve"));
    }

    #[test]
    fn later_views_use_surface_load_instead_of_clear() {
        let (device, queue) = create_test_device();
        let ctx =
            GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [64, 64]);
        let mut pipeline = SpriteFramePipeline::unlit(&ctx);
        let settings = RenderSettings::default();
        let gpu_scene = GpuScene2D::new(&ctx);
        let views = [test_view(0, 0, 32, 64, 0), test_view(32, 0, 32, 64, 1)];
        let mut frame = PreparedFrame::new(ctx.surface_format(), true);
        let _ = frame.insert_payload(&settings);
        let _ = frame.insert_payload(&gpu_scene);
        for (index, view_payload) in views.iter().enumerate() {
            let mut view = PreparedView::new(
                view_payload.order,
                view_payload.viewport,
                view_payload.viewport.size(),
                index == 0,
            );
            let _ = view.insert_payload(view_payload);
            frame.add_view(view);
        }

        pipeline.debug_prepare_frame(&frame);

        let loads = pipeline.debug_declared_surface_loads("viewport_blit");
        assert_eq!(loads.len(), 2);
        assert!(matches!(loads[0], crate::render::graph::LoadOp::Clear(_)));
        assert!(matches!(loads[1], crate::render::graph::LoadOp::Load));
    }

    #[test]
    fn light_node_uses_ambient_color_instead_of_clear_color() {
        let (device, queue) = create_test_device();
        let mut ctx =
            GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [32, 32]);
        let mut pipeline = SpriteFramePipeline::new();
        pipeline.add(Box::new(SpriteSceneNode::hdr(&ctx)));
        pipeline.add(Box::new(SpriteLightNode::new(&ctx)));
        pipeline.add(Box::new(SpriteCompositeNode::new(&ctx)));
        pipeline.add(Box::new(ColorResolveNode::new(&ctx)));
        pipeline.add(Box::new(KeepAliveNode));

        let mut scene = SceneCache2D::new();
        scene.settings = RenderSettings {
            clear_color: Color::RED,
            ambient_color: Color::GREEN,
            tonemap: ToneMapSettings {
                enabled: false,
                ..Default::default()
            },
            ..RenderSettings::default()
        };
        let mut prepared = PreparedRenderWorld2D::new();
        let camera = Camera2D::new(32.0, 32.0);
        let projection = crate::render::scene::Projection::orthographic(32.0, 32.0);
        let views = [crate::render::scene::SceneView::new(
            0,
            ViewportRect::new(0, 0, 32, 32),
            [32, 32],
            true,
            u32::MAX,
            crate::render::Transform::default(),
            projection,
            camera.uniform(),
            Some(camera),
        )];
        prepared.prepare_scene(&mut scene, &views, [32, 32]);
        let mut gpu_scene = GpuScene2D::new(&ctx);
        gpu_scene.upload_scene_frame(&ctx, &mut scene, &prepared);

        let mut frame = PreparedFrame::new(ctx.surface_format(), false);
        let _ = frame.insert_payload(&scene.settings);
        let _ = frame.insert_payload(&gpu_scene);
        let prepared_view = gpu_scene.views()[0];
        let mut view = PreparedView::new(
            prepared_view.order,
            prepared_view.viewport,
            prepared_view.viewport.size(),
            true,
        );
        let _ = view.insert_payload(&prepared_view);
        frame.add_view(view);

        ctx.begin_frame()
            .expect("headless begin_frame should succeed");
        let _ = pipeline.execute_frame(&mut ctx, &frame);
        ctx.end_frame();

        assert_eq!(pipeline.last_light_ambient(), Some(Color::GREEN.to_array()));
        assert_ne!(pipeline.last_light_ambient(), Some(Color::RED.to_array()));
    }
}
