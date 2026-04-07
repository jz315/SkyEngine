use crate::ecs::World;
use crate::gpu::GpuContext;
use crate::render::ecs::RenderSettings2D;
use crate::render::gpu_scene2d::GpuScene2D;
use crate::render::pipeline::{
    FramePayloads2D, PreparedRenderWorld2D, RenderPipeline, SceneCache2D, SceneExtractor,
};
use crate::render::Color;
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Renderer2DConfig {
    path: RenderPath2D,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RenderPath2D {
    Unlit,
    LitHdr,
}

impl Renderer2DConfig {
    #[inline]
    pub const fn unlit() -> Self {
        Self {
            path: RenderPath2D::Unlit,
        }
    }

    #[inline]
    pub const fn lit_hdr() -> Self {
        Self {
            path: RenderPath2D::LitHdr,
        }
    }

    #[inline]
    fn uses_hdr(self) -> bool {
        matches!(self.path, RenderPath2D::LitHdr)
    }

    fn default_settings(self) -> RenderSettings2D {
        use crate::render::ecs::{BloomSettings, ToneMapSettings, VignetteSettings};
        match self.path {
            RenderPath2D::Unlit => RenderSettings2D {
                clear_color: Color::rgb(0.02, 0.02, 0.06),
                ambient_color: Color::BLACK,
                bloom: BloomSettings {
                    enabled: false,
                    ..Default::default()
                },
                tonemap: ToneMapSettings {
                    enabled: false,
                    ..Default::default()
                },
                vignette: VignetteSettings {
                    enabled: false,
                    ..Default::default()
                },
            },
            RenderPath2D::LitHdr => RenderSettings2D::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct RendererTimingStats {
    pub frame_ms: f64,
    pub extract_ms: f64,
    pub resize_ms: f64,
    pub prepare_ms: f64,
    pub upload_ms: f64,
    pub execute_ms: f64,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct RendererStats {
    pub view_count: usize,
    pub sprite_count: usize,
    pub light_count: usize,
    pub draw_calls: usize,
    pub passes: usize,
    pub dirty_sprite_slots: usize,
    pub dirty_light_slots: usize,
    pub visible_sprite_upload_count: usize,
    pub visible_light_upload_count: usize,
    pub timings: RendererTimingStats,
}

pub struct Renderer2D {
    config: Renderer2DConfig,
    pipeline: RenderPipeline,
    extractor: SceneExtractor,
    prepared_world: PreparedRenderWorld2D,
    gpu_scene: GpuScene2D,
    scene_cache: SceneCache2D,
    surface_size: [u32; 2],
    last_stats: RendererStats,
}

impl Renderer2D {
    pub fn new(gpu: &GpuContext, config: Renderer2DConfig) -> Self {
        let pipeline = if config.uses_hdr() {
            RenderPipeline::lit_hdr(gpu)
        } else {
            RenderPipeline::unlit(gpu)
        };
        Self {
            config,
            pipeline,
            extractor: SceneExtractor::new(),
            prepared_world: PreparedRenderWorld2D::new(),
            gpu_scene: GpuScene2D::new(gpu),
            scene_cache: SceneCache2D::new(),
            surface_size: gpu.surface_size(),
            last_stats: RendererStats::default(),
        }
    }

    pub fn from_pipeline(
        gpu: &GpuContext,
        config: Renderer2DConfig,
        pipeline: RenderPipeline,
    ) -> Self {
        Self {
            config,
            pipeline,
            extractor: SceneExtractor::new(),
            prepared_world: PreparedRenderWorld2D::new(),
            gpu_scene: GpuScene2D::new(gpu),
            scene_cache: SceneCache2D::new(),
            surface_size: gpu.surface_size(),
            last_stats: RendererStats::default(),
        }
    }

    pub(crate) fn resize(&mut self, gpu: &GpuContext, width: u32, height: u32) {
        self.pipeline.resize(gpu, width, height);
        self.surface_size = [width, height];
    }

    pub(crate) fn surface_lost(&mut self) {}

    /// Render the current ECS world into the active frame.
    pub fn render_world(&mut self, gpu: &mut GpuContext, world: &World) {
        let frame_start = Instant::now();
        let mut scene_cache = std::mem::take(&mut self.scene_cache);
        let extract_start = Instant::now();
        self.extractor.sync_incremental(
            world,
            &mut scene_cache,
            gpu.surface_size(),
            self.config.default_settings(),
        );
        let extract_ms = elapsed_ms(extract_start);
        self.render_cached_scene(gpu, &mut scene_cache, frame_start, extract_ms);
        self.scene_cache = scene_cache;
    }

    #[inline]
    /// Statistics from the most recent [`render_world`](Self::render_world) call.
    pub fn stats(&self) -> RendererStats {
        self.last_stats
    }

    fn render_cached_scene(
        &mut self,
        gpu: &mut GpuContext,
        scene: &mut SceneCache2D,
        frame_start: Instant,
        extract_ms: f64,
    ) {
        let [width, height] = gpu.surface_size();
        let resize_start = Instant::now();
        let resize_ms = if self.surface_size != [width, height] {
            self.resize(gpu, width, height);
            elapsed_ms(resize_start)
        } else {
            0.0
        };

        let prepare_start = Instant::now();
        self.prepared_world.prepare_scene(scene, [width, height]);
        let prepare_ms = elapsed_ms(prepare_start);

        let upload_start = Instant::now();
        self.gpu_scene
            .upload_scene_frame(gpu, scene, &self.prepared_world);
        let upload_ms = elapsed_ms(upload_start);

        let execute_start = Instant::now();
        self.pipeline
            .begin_frame(&scene.settings, gpu.surface_format(), gpu.has_surface());
        for (index, view) in self.gpu_scene.views().iter().copied().enumerate() {
            self.pipeline.enqueue_view(view, index == 0);
        }
        let frame_payloads = FramePayloads2D::new().with_gpu_scene(&self.gpu_scene);
        let execution = self.pipeline.execute_frame(gpu, &frame_payloads);
        let execute_ms = elapsed_ms(execute_start);

        self.last_stats = RendererStats {
            view_count: self.gpu_scene.views().len(),
            sprite_count: self.gpu_scene.sprite_count(),
            light_count: self.gpu_scene.light_count(),
            draw_calls: execution.draw_calls,
            passes: execution.passes,
            dirty_sprite_slots: self.gpu_scene.dirty_sprite_slot_uploads(),
            dirty_light_slots: self.gpu_scene.dirty_light_slot_uploads(),
            visible_sprite_upload_count: self.gpu_scene.visible_sprite_upload_count(),
            visible_light_upload_count: self.gpu_scene.visible_light_upload_count(),
            timings: RendererTimingStats {
                frame_ms: elapsed_ms(frame_start),
                extract_ms,
                resize_ms,
                prepare_ms,
                upload_ms,
                execute_ms,
            },
        };
    }
}

#[inline]
fn elapsed_ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::{EntityId, World};
    use crate::render::ecs::{
        BloomSettings, PointLight2D, PrimaryCamera2D, RenderView2D, Sprite2D, Transform2D,
        ViewportRect,
    };
    use crate::render::graph::{
        CompiledPass, PhysicalResources, RenderGraph, RenderGraphError, TargetSize,
    };
    use crate::render::pipeline::{
        ColorResolveNode, CompositeNode, FeatureExecutionContext2D, LightNode, PipelineState2D,
        RenderFeature2D, SpritePass,
    };
    use crate::render::Camera2D;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    fn sprite_slot(renderer: &Renderer2D, entity: EntityId) -> usize {
        renderer
            .scene_cache
            .sprite_slot_for_entity(entity)
            .expect("sprite entity should have a stable scene slot")
    }

    fn light_slot(renderer: &Renderer2D, entity: EntityId) -> usize {
        renderer
            .scene_cache
            .light_slot_for_entity(entity)
            .expect("light entity should have a stable scene slot")
    }

    fn create_test_device() -> (wgpu::Device, wgpu::Queue) {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .expect("No suitable GPU adapter found for renderer tests");

        pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("renderer2d_test_device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
            },
            None,
        ))
        .expect("Failed to create test GPU device")
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
            let format = state.current_format().unwrap_or(state.surface_format());
            let sink = graph.create_texture(|b| {
                b.name("keep_alive_sink")
                    .size(TargetSize::Exact(
                        state.view_size()[0],
                        state.view_size()[1],
                    ))
                    .format(format)
                    .persistent();
            });
            graph.add_render_pass("keep_alive", |s| {
                s.read(input);
                s.write_color(0, sink);
            });
            state.set_current(sink, format);
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

    struct ResizeCounterNode {
        count: Arc<AtomicUsize>,
    }

    impl RenderFeature2D for ResizeCounterNode {
        fn name(&self) -> &'static str {
            "resize_counter"
        }

        fn setup(&mut self, graph: &mut RenderGraph, state: &mut PipelineState2D) {
            let format = state.current_format().unwrap_or(state.surface_format());
            let sink = graph.create_texture(|b| {
                b.name("resize_counter_sink")
                    .size(TargetSize::Exact(
                        state.view_size()[0],
                        state.view_size()[1],
                    ))
                    .format(format)
                    .persistent();
            });
            graph.add_render_pass("resize_counter", |s| {
                s.write_color(0, sink);
            });
            state.set_current(sink, format);
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

        fn resize(&mut self, _ctx: &GpuContext, _width: u32, _height: u32) {
            self.count.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn render_once(renderer: &mut Renderer2D, ctx: &mut GpuContext, world: &World) {
        ctx.begin_frame()
            .expect("headless begin_frame should succeed");
        renderer.render_world(ctx, world);
        ctx.end_frame();
    }

    #[test]
    fn renderer_lifecycle_methods_work_headless() {
        let (device, queue) = create_test_device();
        let mut ctx =
            GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [64, 64]);
        let mut renderer = Renderer2D::new(&ctx, Renderer2DConfig::lit_hdr());
        let world = World::new();

        renderer.resize(&ctx, 128, 72);
        renderer.surface_lost();
        render_once(&mut renderer, &mut ctx, &world);
    }

    #[test]
    fn render_world_succeeds_with_primary_camera_and_sprite() {
        let (device, queue) = create_test_device();
        let mut ctx =
            GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [32, 32]);
        let mut renderer = Renderer2D::new(&ctx, Renderer2DConfig::unlit());

        let mut world = World::new();
        world.spawn((Camera2D::new(32.0, 32.0), PrimaryCamera2D));
        world.spawn((Transform2D::default(), Sprite2D::new(8.0, 8.0)));

        render_once(&mut renderer, &mut ctx, &world);

        assert_eq!(renderer.stats().sprite_count, 1);
        assert_eq!(renderer.stats().view_count, 1);
    }

    #[test]
    fn fallback_view_is_created_when_world_has_none() {
        let (device, queue) = create_test_device();
        let mut ctx =
            GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [48, 24]);
        let mut renderer = Renderer2D::new(&ctx, Renderer2DConfig::unlit());
        let mut world = World::new();

        world.spawn((Transform2D::new(1.0, 2.0), Sprite2D::new(4.0, 4.0)));
        render_once(&mut renderer, &mut ctx, &world);

        assert_eq!(renderer.scene_cache.views.len(), 1);
        assert_eq!(renderer.scene_cache.views[0].render.viewport.width, 48);
        assert_eq!(renderer.scene_cache.views[0].render.viewport.height, 24);
        assert_eq!(renderer.scene_cache.views[0].camera.viewport_width(), 48.0);
        assert_eq!(renderer.scene_cache.views[0].camera.viewport_height(), 24.0);
    }

    #[test]
    fn point_lights_extract_from_transform_and_component() {
        let (device, queue) = create_test_device();
        let mut ctx =
            GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [32, 32]);
        let mut renderer = Renderer2D::new(&ctx, Renderer2DConfig::lit_hdr());
        let mut world = World::new();

        let entity = world.spawn((
            Transform2D::new(12.0, 18.0),
            PointLight2D::new(42.0).intensity(2.5).temperature(3000.0),
        ));

        render_once(&mut renderer, &mut ctx, &world);
        assert_eq!(renderer.scene_cache.active_light_count(), 1);
        let slot = light_slot(&renderer, entity);
        let item = renderer.scene_cache.light_item(slot).unwrap();
        assert_eq!(item.transform.x, 12.0);
        assert_eq!(item.transform.y, 18.0);
        assert_eq!(item.light.radius, 42.0);
        assert_eq!(item.light.intensity, 2.5);
        assert_eq!(item.light.temperature, 3000.0);
    }

    #[test]
    fn render_settings_resource_overrides_defaults() {
        let (device, queue) = create_test_device();
        let mut ctx =
            GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [32, 32]);
        let mut renderer = Renderer2D::new(&ctx, Renderer2DConfig::lit_hdr());
        let mut world = World::new();
        let custom = RenderSettings2D {
            clear_color: Color::RED,
            ..RenderSettings2D::default()
        };

        world.insert_resource(custom);
        render_once(&mut renderer, &mut ctx, &world);

        assert_eq!(renderer.scene_cache.settings.clear_color.r, Color::RED.r);
        assert_eq!(renderer.scene_cache.settings.clear_color.g, Color::RED.g);
    }

    #[test]
    fn render_world_keeps_hidden_objects_in_incremental_scene_cache() {
        let (device, queue) = create_test_device();
        let mut ctx =
            GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [32, 32]);
        let mut renderer = Renderer2D::new(&ctx, Renderer2DConfig::unlit());
        let mut world = World::new();

        let entity = world.spawn((
            Transform2D::default(),
            Sprite2D::new(4.0, 4.0).visible(false),
        ));

        render_once(&mut renderer, &mut ctx, &world);

        assert_eq!(renderer.stats().sprite_count, 0);
        assert_eq!(renderer.scene_cache.active_sprite_count(), 1);
        assert_eq!(renderer.scene_cache.sprite_slot_capacity(), 1);

        world.get_mut::<Sprite2D>(entity).unwrap().visible = true;

        render_once(&mut renderer, &mut ctx, &world);

        assert_eq!(renderer.stats().sprite_count, 1);
        assert_eq!(renderer.scene_cache.active_sprite_count(), 1);
        assert_eq!(renderer.scene_cache.sprite_slot_capacity(), 1);
    }

    #[test]
    fn incremental_world_sync_updates_slot_mapping_after_despawn() {
        let (device, queue) = create_test_device();
        let mut ctx =
            GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [64, 64]);
        let mut renderer = Renderer2D::new(&ctx, Renderer2DConfig::unlit());
        let mut world = World::new();

        let first = world.spawn((Transform2D::new(1.0, 0.0), Sprite2D::new(4.0, 4.0)));
        let second = world.spawn((Transform2D::new(2.0, 0.0), Sprite2D::new(4.0, 4.0)));

        render_once(&mut renderer, &mut ctx, &world);

        let first_slot = sprite_slot(&renderer, first);
        let second_slot = sprite_slot(&renderer, second);
        assert_ne!(first_slot, second_slot);

        world.despawn(first);

        render_once(&mut renderer, &mut ctx, &world);

        assert_eq!(renderer.stats().sprite_count, 1);
        assert_eq!(renderer.scene_cache.active_sprite_count(), 1);
        assert_eq!(sprite_slot(&renderer, second), second_slot);
        assert_eq!(
            renderer
                .scene_cache
                .sprite_item(second_slot)
                .unwrap()
                .transform
                .x,
            2.0
        );

        world.get_mut::<Transform2D>(second).unwrap().x = 5.0;

        render_once(&mut renderer, &mut ctx, &world);

        assert_eq!(
            renderer
                .scene_cache
                .sprite_item(second_slot)
                .unwrap()
                .transform
                .x,
            5.0
        );
        assert_eq!(renderer.stats().sprite_count, 1);

        let third = world.spawn((Transform2D::new(7.0, 0.0), Sprite2D::new(4.0, 4.0)));
        render_once(&mut renderer, &mut ctx, &world);
        assert_eq!(sprite_slot(&renderer, third), first_slot);
    }

    #[test]
    fn incremental_world_sync_refreshes_sprite_values_without_archetype_changes() {
        let (device, queue) = create_test_device();
        let mut ctx =
            GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [64, 64]);
        let mut renderer = Renderer2D::new(&ctx, Renderer2DConfig::unlit());
        let mut world = World::new();

        let entity = world.spawn((Transform2D::new(1.0, 2.0), Sprite2D::new(4.0, 4.0)));
        render_once(&mut renderer, &mut ctx, &world);
        let slot = sprite_slot(&renderer, entity);
        assert_eq!(
            renderer.scene_cache.sprite_item(slot).unwrap().transform.x,
            1.0
        );

        world.get_mut::<Transform2D>(entity).unwrap().x = 9.0;
        render_once(&mut renderer, &mut ctx, &world);
        assert_eq!(
            renderer.scene_cache.sprite_item(slot).unwrap().transform.x,
            9.0
        );
    }

    #[test]
    fn incremental_world_sync_refreshes_light_values_without_archetype_changes() {
        let (device, queue) = create_test_device();
        let mut ctx =
            GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [64, 64]);
        let mut renderer = Renderer2D::new(&ctx, Renderer2DConfig::lit_hdr());
        let mut world = World::new();

        let entity = world.spawn((Transform2D::new(3.0, 4.0), PointLight2D::new(16.0)));
        render_once(&mut renderer, &mut ctx, &world);
        let slot = light_slot(&renderer, entity);
        assert_eq!(
            renderer.scene_cache.light_item(slot).unwrap().light.radius,
            16.0
        );

        world.get_mut::<PointLight2D>(entity).unwrap().radius = 48.0;
        render_once(&mut renderer, &mut ctx, &world);
        assert_eq!(
            renderer.scene_cache.light_item(slot).unwrap().light.radius,
            48.0
        );
    }

    #[test]
    fn extractor_uses_explicit_render_views_when_present() {
        let (device, queue) = create_test_device();
        let mut ctx =
            GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [64, 64]);
        let mut renderer = Renderer2D::new(&ctx, Renderer2DConfig::unlit());
        let mut world = World::new();

        world.spawn((
            Camera2D::new(64.0, 64.0),
            RenderView2D::new(ViewportRect::new(8, 4, 20, 12))
                .order(3)
                .layer_mask(0x2),
            PrimaryCamera2D,
        ));

        render_once(&mut renderer, &mut ctx, &world);

        assert_eq!(renderer.scene_cache.views.len(), 1);
        assert_eq!(renderer.scene_cache.views[0].render.order, 3);
        assert_eq!(renderer.scene_cache.views[0].render.layer_mask, 0x2);
        assert_eq!(renderer.scene_cache.views[0].render.viewport.width, 20);
    }

    #[test]
    fn pipeline_rebuilds_when_postfx_config_changes() {
        let (device, queue) = create_test_device();
        let ctx =
            GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [32, 32]);
        let mut pipeline = RenderPipeline::lit_hdr(&ctx);

        let settings_a = RenderSettings2D::default();
        pipeline.rebuild_if_needed(&settings_a, wgpu::TextureFormat::Bgra8Unorm);

        let settings_b = RenderSettings2D {
            bloom: BloomSettings {
                enabled: false,
                ..Default::default()
            },
            ..RenderSettings2D::default()
        };
        pipeline.rebuild_if_needed(&settings_b, wgpu::TextureFormat::Bgra8Unorm);
    }

    #[test]
    fn multi_view_stats_report_view_count() {
        let (device, queue) = create_test_device();
        let mut ctx =
            GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [64, 64]);
        let mut renderer = Renderer2D::new(&ctx, Renderer2DConfig::unlit());
        let mut world = World::new();
        world.spawn((
            Camera2D::new(32.0, 64.0),
            RenderView2D::new(ViewportRect::new(0, 0, 32, 64)),
            PrimaryCamera2D,
        ));
        world.spawn((
            Camera2D::new(32.0, 64.0),
            RenderView2D::new(ViewportRect::new(32, 0, 32, 64)).order(1),
        ));

        render_once(&mut renderer, &mut ctx, &world);

        assert_eq!(renderer.stats().view_count, 2);
    }

    #[test]
    fn single_view_frame_executes_graph_once() {
        let (device, queue) = create_test_device();
        let mut ctx =
            GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [64, 64]);
        let mut renderer = Renderer2D::new(&ctx, Renderer2DConfig::unlit());
        let world = World::new();

        render_once(&mut renderer, &mut ctx, &world);

        assert_eq!(renderer.pipeline.execute_count(), 1);
    }

    #[test]
    fn multi_view_frame_executes_graph_once() {
        let (device, queue) = create_test_device();
        let mut ctx =
            GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [64, 64]);
        let mut renderer = Renderer2D::new(&ctx, Renderer2DConfig::unlit());
        let mut world = World::new();
        world.spawn((
            Camera2D::new(32.0, 64.0),
            RenderView2D::new(ViewportRect::new(0, 0, 32, 64)),
            PrimaryCamera2D,
        ));
        world.spawn((
            Camera2D::new(32.0, 64.0),
            RenderView2D::new(ViewportRect::new(32, 0, 32, 64)).order(1),
        ));

        render_once(&mut renderer, &mut ctx, &world);

        assert_eq!(renderer.pipeline.execute_count(), 1);
    }

    #[test]
    fn sparse_upload_stats_track_dirty_slots_without_full_rewrites() {
        let (device, queue) = create_test_device();
        let mut ctx =
            GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [64, 64]);
        let mut renderer = Renderer2D::new(&ctx, Renderer2DConfig::unlit());
        let mut world = World::new();
        let first = world.spawn((Transform2D::new(1.0, 0.0), Sprite2D::new(8.0, 8.0)));
        let _second = world.spawn((Transform2D::new(2.0, 0.0), Sprite2D::new(8.0, 8.0)));

        render_once(&mut renderer, &mut ctx, &world);
        assert_eq!(renderer.stats().dirty_sprite_slots, 2);
        assert_eq!(renderer.stats().visible_sprite_upload_count, 2);

        render_once(&mut renderer, &mut ctx, &world);
        assert_eq!(renderer.stats().dirty_sprite_slots, 0);
        assert_eq!(renderer.stats().visible_sprite_upload_count, 2);

        world.get_mut::<Transform2D>(first).unwrap().x = 9.0;
        render_once(&mut renderer, &mut ctx, &world);
        assert_eq!(renderer.stats().dirty_sprite_slots, 1);
        assert_eq!(renderer.stats().visible_sprite_upload_count, 2);
    }

    #[test]
    fn surface_lost_and_resize_still_allow_followup_render() {
        let (device, queue) = create_test_device();
        let mut ctx =
            GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [64, 64]);
        let mut renderer = Renderer2D::new(&ctx, Renderer2DConfig::unlit());
        let mut world = World::new();
        world.spawn((Transform2D::default(), Sprite2D::new(8.0, 8.0)));

        render_once(&mut renderer, &mut ctx, &world);

        ctx.resize_surface(96, 48);
        renderer.surface_lost();

        render_once(&mut renderer, &mut ctx, &world);

        assert_eq!(renderer.stats().view_count, 1);
        assert_eq!(renderer.stats().sprite_count, 1);
    }

    #[test]
    fn renderer_only_resizes_pipeline_when_surface_size_changes() {
        let (device, queue) = create_test_device();
        let mut ctx =
            GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [64, 64]);
        let resize_count = Arc::new(AtomicUsize::new(0));
        let mut pipeline = RenderPipeline::new();
        pipeline.add(Box::new(ResizeCounterNode {
            count: resize_count.clone(),
        }));
        pipeline.add(Box::new(KeepAliveNode));
        let mut renderer = Renderer2D::from_pipeline(&ctx, Renderer2DConfig::unlit(), pipeline);
        let mut world = World::new();
        world.spawn((Transform2D::default(), Sprite2D::new(8.0, 8.0)));

        render_once(&mut renderer, &mut ctx, &world);
        render_once(&mut renderer, &mut ctx, &world);
        assert_eq!(resize_count.load(Ordering::Relaxed), 0);

        ctx.resize_surface(96, 48);
        render_once(&mut renderer, &mut ctx, &world);
        assert_eq!(resize_count.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn renderer_stats_report_actual_executed_passes() {
        let (device, queue) = create_test_device();
        let mut ctx =
            GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [64, 64]);
        let mut pipeline = RenderPipeline::new();
        pipeline.add(Box::new(SpritePass::hdr(&ctx)));
        pipeline.add(Box::new(LightNode::new(&ctx)));
        pipeline.add(Box::new(CompositeNode::new(&ctx)));
        pipeline.add(Box::new(ColorResolveNode::new(&ctx)));
        pipeline.add(Box::new(KeepAliveNode));

        let mut renderer = Renderer2D::from_pipeline(&ctx, Renderer2DConfig::lit_hdr(), pipeline);
        let mut world = World::new();
        world.insert_resource(RenderSettings2D {
            tonemap: crate::render::ToneMapSettings {
                enabled: false,
                ..Default::default()
            },
            ..RenderSettings2D::default()
        });
        world.spawn((Transform2D::default(), Sprite2D::new(8.0, 8.0)));
        world.spawn((Transform2D::default(), PointLight2D::new(24.0)));

        render_once(&mut renderer, &mut ctx, &world);

        assert_eq!(renderer.stats().view_count, 1);
        assert_eq!(renderer.stats().sprite_count, 1);
        assert_eq!(renderer.stats().light_count, 1);
        assert_eq!(renderer.stats().draw_calls, 4);
        assert_eq!(renderer.stats().passes, 5);
    }

    #[test]
    fn unlit_draw_calls_ignore_non_executed_light_work() {
        let (device, queue) = create_test_device();
        let mut ctx =
            GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [64, 64]);
        let mut pipeline = RenderPipeline::new();
        pipeline.add(Box::new(SpritePass::surface(&ctx)));
        pipeline.add(Box::new(KeepAliveNode));
        let mut renderer = Renderer2D::from_pipeline(&ctx, Renderer2DConfig::unlit(), pipeline);
        let mut world = World::new();
        world.spawn((Transform2D::default(), Sprite2D::new(8.0, 8.0)));
        world.spawn((Transform2D::default(), PointLight2D::new(32.0)));

        render_once(&mut renderer, &mut ctx, &world);

        assert_eq!(renderer.stats().draw_calls, 1);
        assert_eq!(renderer.stats().passes, 2);
    }
}
