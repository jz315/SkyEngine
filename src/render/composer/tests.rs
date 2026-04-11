use std::any::Any;
use std::sync::{Arc, Mutex};

#[cfg(feature = "live2d")]
use crate::ecs::EntityId;
use crate::ecs::World;
use crate::gpu::GpuContext;
use crate::render::frame_pipeline::{
    FrameViewNode, PhaseState, PreparedFrame, PreparedView, TextureFormat, ViewExecutionContext,
};
use crate::render::graph::{
    CompiledPass, PhysicalResources, RenderGraph, RenderGraphError, TargetSize,
};
use crate::render::pipeline::{
    OutputChainConfig, RenderFeature, RenderFeatureExecuteContext, RenderFeatureSetupContext,
    RenderPipelineAsset,
};
use crate::render::scene::{
    Projection, RenderInjectionPoint, RenderOutputFormat, RenderQueueDesc, RenderQueueSort,
    SceneView, SCENE_HDR_FORMAT,
};
use crate::render::{
    Camera, CameraViewport, Color, MainCamera, RenderComposer, RenderDomain, RenderSettings,
    SpriteRenderer, Transform, ViewportRect,
};
#[cfg(feature = "live2d")]
use crate::render::{OrderInLayer, SortingLayer};

#[cfg(feature = "live2d")]
use crate::render::domains::live2d::{
    live2d_instance_visible_in_view, sort_live2d_scene_instances, Live2DSceneInstance,
};
use crate::render::scene::orthographic_cull_camera;

fn create_test_device() -> (wgpu::Device, wgpu::Queue) {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        compatible_surface: None,
        force_fallback_adapter: false,
    }))
    .expect("No suitable GPU adapter found for scene pipeline tests");

    pollster::block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: Some("scene_pipeline_test_device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::Performance,
        },
        None,
    ))
    .expect("Failed to create scene pipeline test GPU device")
}

struct LoggingNode {
    name: &'static str,
    log: Arc<Mutex<Vec<&'static str>>>,
}

impl FrameViewNode for LoggingNode {
    fn name(&self) -> &'static str {
        self.name
    }

    fn setup(
        &mut self,
        graph: &mut RenderGraph,
        state: &mut PhaseState,
        frame: &PreparedFrame<'_>,
        view: &PreparedView<'_>,
    ) {
        let input = state
            .current_color()
            .expect("logging node should see current_color");
        let output = graph.create_texture(|builder| {
            builder
                .name(self.name)
                .size(TargetSize::Exact(
                    view.target_size()[0],
                    view.target_size()[1],
                ))
                .format(frame.surface_format());
        });
        graph.add_render_pass(self.name, |setup| {
            setup.read(input.handle());
            setup.write_color(0, output);
        });
        state.set_current_color(output, frame.surface_format());
    }

    fn execute(
        &mut self,
        _pass: &CompiledPass,
        _ctx: &mut GpuContext,
        _resources: &PhysicalResources<'_>,
        _execution: &ViewExecutionContext<'_>,
    ) -> Result<(), RenderGraphError> {
        self.log.lock().unwrap().push(self.name);
        Ok(())
    }
}

struct LoggingProducer {
    name: &'static str,
    log: Arc<Mutex<Vec<&'static str>>>,
}

impl RenderDomain for LoggingProducer {
    fn name(&self) -> &'static str {
        self.name
    }

    fn collect_views(&self, views: &mut Vec<SceneView>) {
        if views.is_empty() {
            let projection = Projection::orthographic(64.0, 64.0);
            views.push(SceneView::new(
                0,
                ViewportRect::new(0, 0, 64, 64),
                [64, 64],
                true,
                u32::MAX,
                Transform::default(),
                projection,
                projection.view_uniform(Transform::default(), [64, 64]),
                orthographic_cull_camera(Transform::default(), projection),
            ));
        }
    }

    fn create_view_nodes(&mut self, _ctx: &GpuContext) -> Vec<Box<dyn FrameViewNode>> {
        vec![Box::new(LoggingNode {
            name: self.name,
            log: Arc::clone(&self.log),
        })]
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

struct LoggingFeature {
    name: &'static str,
    log: Arc<Mutex<Vec<&'static str>>>,
}

impl RenderFeature for LoggingFeature {
    fn name(&self) -> &'static str {
        self.name
    }

    fn setup(&mut self, ctx: &mut RenderFeatureSetupContext<'_, '_>) {
        let input = ctx
            .current_color()
            .expect("logging feature should see current_color");
        let target_size = ctx.scene_view().target_size;
        let output = ctx.graph_mut().create_texture(|builder| {
            builder
                .name(self.name)
                .size(TargetSize::Exact(target_size[0], target_size[1]))
                .format(input.format());
        });
        ctx.graph_mut().add_render_pass(self.name, |setup| {
            setup.read(input.handle());
            setup.write_color(0, output);
        });
        ctx.set_current_color(output, input.format());
    }

    fn execute(
        &mut self,
        _ctx: &mut RenderFeatureExecuteContext<'_, '_>,
    ) -> Result<(), RenderGraphError> {
        self.log.lock().unwrap().push(self.name);
        Ok(())
    }
}

struct FormatReportingProducer {
    name: &'static str,
    configured_format: Arc<Mutex<Option<TextureFormat>>>,
    hdr_output: bool,
}

impl RenderDomain for FormatReportingProducer {
    fn name(&self) -> &'static str {
        self.name
    }

    fn configure_target_format(&mut self, target_format: TextureFormat) {
        *self.configured_format.lock().unwrap() = Some(target_format);
    }

    fn output_format_hint(
        &self,
        input_format: TextureFormat,
        _surface_format: TextureFormat,
    ) -> TextureFormat {
        if self.hdr_output {
            SCENE_HDR_FORMAT
        } else {
            input_format
        }
    }

    fn collect_views(&self, views: &mut Vec<SceneView>) {
        if views.is_empty() {
            let projection = Projection::orthographic(64.0, 64.0);
            views.push(SceneView::new(
                0,
                ViewportRect::new(0, 0, 64, 64),
                [64, 64],
                true,
                u32::MAX,
                Transform::default(),
                projection,
                projection.view_uniform(Transform::default(), [64, 64]),
                orthographic_cull_camera(Transform::default(), projection),
            ));
        }
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

struct FormatChangingFeature {
    name: &'static str,
    output: RenderOutputFormat,
}

impl RenderFeature for FormatChangingFeature {
    fn name(&self) -> &'static str {
        self.name
    }

    fn output_format_hint(&self) -> RenderOutputFormat {
        self.output
    }

    fn setup(&mut self, _ctx: &mut RenderFeatureSetupContext<'_, '_>) {}

    fn execute(
        &mut self,
        _ctx: &mut RenderFeatureExecuteContext<'_, '_>,
    ) -> Result<(), RenderGraphError> {
        Ok(())
    }
}

struct ConditionalHdrFeature {
    name: &'static str,
}

impl RenderFeature for ConditionalHdrFeature {
    fn name(&self) -> &'static str {
        self.name
    }

    fn is_enabled(&self, frame: &PreparedFrame<'_>) -> bool {
        frame
            .payload::<RenderSettings>()
            .is_some_and(|settings| settings.clear_color.r >= 0.9)
    }

    fn output_format_hint(&self) -> RenderOutputFormat {
        RenderOutputFormat::Hdr
    }

    fn setup(&mut self, _ctx: &mut RenderFeatureSetupContext<'_, '_>) {}

    fn execute(
        &mut self,
        _ctx: &mut RenderFeatureExecuteContext<'_, '_>,
    ) -> Result<(), RenderGraphError> {
        Ok(())
    }
}

#[test]
fn programmable_pipeline_orders_stage_queue_feature_and_domain_execution() {
    let (device, queue) = create_test_device();
    let mut ctx =
        GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [64, 64]);
    let log = Arc::new(Mutex::new(Vec::new()));
    let asset = RenderPipelineAsset::builder()
        .add_stage("Transparent")
        .add_stage("Overlay")
        .add_queue(RenderQueueDesc::new(
            "transparent",
            "Transparent",
            RenderQueueSort::TransparentScene,
        ))
        .add_queue(RenderQueueDesc::new(
            "overlay",
            "Overlay",
            RenderQueueSort::OverlayStable,
        ))
        .add_feature(
            LoggingFeature {
                name: "before_transparent",
                log: Arc::clone(&log),
            },
            RenderInjectionPoint::before_stage("Transparent"),
        )
        .add_domain(
            LoggingProducer {
                name: "sprites",
                log: Arc::clone(&log),
            },
            "transparent",
        )
        .add_feature(
            LoggingFeature {
                name: "after_transparent_queue",
                log: Arc::clone(&log),
            },
            RenderInjectionPoint::after_queue("transparent"),
        )
        .add_domain(
            LoggingProducer {
                name: "overlay",
                log: Arc::clone(&log),
            },
            "overlay",
        )
        .add_feature(
            LoggingFeature {
                name: "before_present",
                log: Arc::clone(&log),
            },
            RenderInjectionPoint::BeforePresent,
        )
        .build();
    let mut renderer = RenderComposer::from_asset(asset);

    ctx.begin_frame()
        .expect("headless begin_frame should succeed");
    renderer.render_world(&mut ctx, &World::new());
    ctx.end_frame();

    assert_eq!(
        log.lock().unwrap().as_slice(),
        &[
            "before_transparent",
            "sprites",
            "after_transparent_queue",
            "overlay",
            "before_present",
        ]
    );
}

#[test]
fn universal_unlit_pipeline_renders_default_sprite_scene() {
    let (device, queue) = create_test_device();
    let mut ctx =
        GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [64, 64]);
    let mut renderer = RenderComposer::from_asset(RenderPipelineAsset::universal_unlit());
    let mut world = World::new();
    world.spawn((
        Transform::default(),
        Camera::new(),
        Projection::orthographic(64.0, 64.0),
        MainCamera,
    ));
    world.spawn((Transform::default(), SpriteRenderer::new(8.0, 8.0)));

    ctx.begin_frame()
        .expect("headless begin_frame should succeed");
    renderer.render_world(&mut ctx, &world);
    ctx.end_frame();

    let stats = renderer.stats();
    assert_eq!(stats.sprite_count, 1);
    assert_eq!(stats.view_count, 1);
    assert!(stats.passes >= 1);
}

#[test]
fn projection_view_uniforms_stay_finite_for_orthographic_and_perspective() {
    for projection in [
        Projection::orthographic(1280.0, 720.0),
        Projection::perspective(60.0f32.to_radians(), 0.1, 1000.0),
    ] {
        let uniform = projection.view_uniform(Transform::from_xyz(3.0, 4.0, 5.0), [1280, 720]);
        assert!(uniform.view_proj.iter().all(|value| value.is_finite()));
        assert!(uniform.camera.iter().all(|value| value.is_finite()));
        assert!(uniform.viewport.iter().all(|value| value.is_finite()));
    }
}

#[test]
fn domain_target_formats_follow_hdr_and_postfx_boundaries() {
    let (device, queue) = create_test_device();
    let mut ctx =
        GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [64, 64]);
    let hdr_queue_format = Arc::new(Mutex::new(None));
    let pre_postfx_format = Arc::new(Mutex::new(None));
    let post_postfx_format = Arc::new(Mutex::new(None));

    let asset = RenderPipelineAsset::builder()
        .add_stage("Transparent")
        .add_stage("Overlay")
        .add_queue(RenderQueueDesc::new(
            "scene_hdr",
            "Transparent",
            RenderQueueSort::TransparentScene,
        ))
        .add_queue(RenderQueueDesc::new(
            "scene_overlay",
            "Transparent",
            RenderQueueSort::OverlayStable,
        ))
        .add_queue(RenderQueueDesc::new(
            "overlay",
            "Overlay",
            RenderQueueSort::OverlayStable,
        ))
        .add_domain(
            FormatReportingProducer {
                name: "hdr_scene",
                configured_format: Arc::clone(&hdr_queue_format),
                hdr_output: true,
            },
            "scene_hdr",
        )
        .add_domain(
            FormatReportingProducer {
                name: "pre_postfx",
                configured_format: Arc::clone(&pre_postfx_format),
                hdr_output: false,
            },
            "scene_overlay",
        )
        .add_domain(
            FormatReportingProducer {
                name: "post_postfx",
                configured_format: Arc::clone(&post_postfx_format),
                hdr_output: false,
            },
            "overlay",
        )
        .output_chain(OutputChainConfig::after_stage("Transparent"))
        .build();
    let mut renderer = RenderComposer::from_asset(asset);

    ctx.begin_frame()
        .expect("headless begin_frame should succeed");
    renderer.render_world(&mut ctx, &World::new());
    ctx.end_frame();

    assert_eq!(
        *hdr_queue_format.lock().unwrap(),
        Some(wgpu::TextureFormat::Bgra8Unorm)
    );
    assert_eq!(*pre_postfx_format.lock().unwrap(), Some(SCENE_HDR_FORMAT));
    assert_eq!(
        *post_postfx_format.lock().unwrap(),
        Some(wgpu::TextureFormat::Bgra8Unorm)
    );
}

#[test]
fn feature_output_format_hints_affect_later_domain_formats() {
    let (device, queue) = create_test_device();
    let mut ctx =
        GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [64, 64]);
    let first_queue_format = Arc::new(Mutex::new(None));
    let second_queue_format = Arc::new(Mutex::new(None));
    let third_queue_format = Arc::new(Mutex::new(None));

    let asset = RenderPipelineAsset::builder()
        .add_stage("Transparent")
        .add_queue(RenderQueueDesc::new(
            "first",
            "Transparent",
            RenderQueueSort::TransparentScene,
        ))
        .add_queue(RenderQueueDesc::new(
            "second",
            "Transparent",
            RenderQueueSort::TransparentScene,
        ))
        .add_queue(RenderQueueDesc::new(
            "third",
            "Transparent",
            RenderQueueSort::TransparentScene,
        ))
        .add_domain(
            FormatReportingProducer {
                name: "first",
                configured_format: Arc::clone(&first_queue_format),
                hdr_output: false,
            },
            "first",
        )
        .add_feature(
            FormatChangingFeature {
                name: "after_first_to_hdr",
                output: RenderOutputFormat::Hdr,
            },
            RenderInjectionPoint::after_queue("first"),
        )
        .add_feature(
            FormatChangingFeature {
                name: "before_second_to_surface",
                output: RenderOutputFormat::Surface,
            },
            RenderInjectionPoint::before_queue("third"),
        )
        .add_domain(
            FormatReportingProducer {
                name: "second",
                configured_format: Arc::clone(&second_queue_format),
                hdr_output: false,
            },
            "second",
        )
        .add_domain(
            FormatReportingProducer {
                name: "third",
                configured_format: Arc::clone(&third_queue_format),
                hdr_output: false,
            },
            "third",
        )
        .build();
    let mut renderer = RenderComposer::from_asset(asset);

    ctx.begin_frame()
        .expect("headless begin_frame should succeed");
    renderer.render_world(&mut ctx, &World::new());
    ctx.end_frame();

    assert_eq!(
        *first_queue_format.lock().unwrap(),
        Some(wgpu::TextureFormat::Bgra8Unorm)
    );
    assert_eq!(*second_queue_format.lock().unwrap(), Some(SCENE_HDR_FORMAT));
    assert_eq!(
        *third_queue_format.lock().unwrap(),
        Some(wgpu::TextureFormat::Bgra8Unorm)
    );
}

#[test]
fn runtime_enabled_feature_hints_affect_later_domain_formats() {
    let (device, queue) = create_test_device();
    let mut ctx =
        GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [64, 64]);
    let first_queue_format = Arc::new(Mutex::new(None));
    let second_queue_format = Arc::new(Mutex::new(None));

    let asset = RenderPipelineAsset::builder()
        .add_stage("Transparent")
        .add_queue(RenderQueueDesc::new(
            "first",
            "Transparent",
            RenderQueueSort::TransparentScene,
        ))
        .add_queue(RenderQueueDesc::new(
            "second",
            "Transparent",
            RenderQueueSort::TransparentScene,
        ))
        .add_domain(
            FormatReportingProducer {
                name: "first",
                configured_format: Arc::clone(&first_queue_format),
                hdr_output: false,
            },
            "first",
        )
        .add_feature(
            ConditionalHdrFeature {
                name: "conditional_hdr",
            },
            RenderInjectionPoint::after_queue("first"),
        )
        .add_domain(
            FormatReportingProducer {
                name: "second",
                configured_format: Arc::clone(&second_queue_format),
                hdr_output: false,
            },
            "second",
        )
        .build();
    let mut renderer = RenderComposer::from_asset(asset);
    let mut world = World::new();

    ctx.begin_frame()
        .expect("headless begin_frame should succeed");
    renderer.render_world(&mut ctx, &world);
    ctx.end_frame();

    assert_eq!(
        *first_queue_format.lock().unwrap(),
        Some(wgpu::TextureFormat::Bgra8Unorm)
    );
    assert_eq!(
        *second_queue_format.lock().unwrap(),
        Some(wgpu::TextureFormat::Bgra8Unorm)
    );

    world.insert_resource(RenderSettings {
        clear_color: Color::RED,
        ..RenderSettings::default()
    });

    ctx.begin_frame()
        .expect("headless begin_frame should succeed");
    renderer.render_world(&mut ctx, &world);
    ctx.end_frame();

    assert_eq!(*second_queue_format.lock().unwrap(), Some(SCENE_HDR_FORMAT));
}

#[test]
fn output_chain_format_follows_runtime_tonemap_state() {
    let (device, queue) = create_test_device();
    let mut ctx =
        GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [64, 64]);
    let transparent_format = Arc::new(Mutex::new(None));
    let overlay_format = Arc::new(Mutex::new(None));

    let asset = RenderPipelineAsset::builder()
        .add_stage("Transparent")
        .add_stage("Overlay")
        .add_queue(RenderQueueDesc::new(
            "scene_hdr",
            "Transparent",
            RenderQueueSort::TransparentScene,
        ))
        .add_queue(RenderQueueDesc::new(
            "overlay",
            "Overlay",
            RenderQueueSort::OverlayStable,
        ))
        .add_domain(
            FormatReportingProducer {
                name: "scene_hdr",
                configured_format: Arc::clone(&transparent_format),
                hdr_output: true,
            },
            "scene_hdr",
        )
        .add_domain(
            FormatReportingProducer {
                name: "overlay",
                configured_format: Arc::clone(&overlay_format),
                hdr_output: false,
            },
            "overlay",
        )
        .output_chain(
            OutputChainConfig::after_stage("Transparent")
                .bloom(false)
                .vignette(false)
                .tonemap(true)
                .color_resolve(false),
        )
        .build();
    let mut renderer = RenderComposer::from_asset(asset);
    let mut world = World::new();
    world.insert_resource(RenderSettings {
        tonemap: crate::render::ToneMapSettings {
            enabled: false,
            ..Default::default()
        },
        ..RenderSettings::default()
    });

    ctx.begin_frame()
        .expect("headless begin_frame should succeed");
    renderer.render_world(&mut ctx, &world);
    ctx.end_frame();

    assert_eq!(
        *transparent_format.lock().unwrap(),
        Some(wgpu::TextureFormat::Bgra8Unorm)
    );
    assert_eq!(*overlay_format.lock().unwrap(), Some(SCENE_HDR_FORMAT));
}

#[test]
fn collect_world_views_uses_camera_projection_viewport_and_layer_mask() {
    let mut renderer = RenderComposer::from_asset(RenderPipelineAsset::overlay());
    renderer.surface_size = [800, 600];

    let mut world = World::new();
    world.spawn((
        Transform::from_xyz(12.0, -4.0, 8.0),
        Camera::new(),
        Projection::orthographic(320.0, 180.0),
        CameraViewport::new(ViewportRect::new(100, 50, 400, 300))
            .order(7)
            .layer_mask(0b0011),
        MainCamera,
    ));
    world.spawn((
        Transform::default(),
        Camera::new().enabled(false),
        Projection::orthographic(64.0, 64.0),
        CameraViewport::new(ViewportRect::new(0, 0, 64, 64)).order(99),
    ));

    let resolved = renderer.resolve_scene_transforms(&world);
    let views = renderer.collect_world_views(&world, &resolved);
    assert_eq!(views.len(), 1);

    let view = views[0];
    assert_eq!(view.order, 7);
    assert_eq!(view.viewport, ViewportRect::new(100, 50, 400, 300));
    assert_eq!(view.target_size, [400, 300]);
    assert_eq!(view.layer_mask, 0b0011);
    assert!(view.cull_camera_2d.is_some());
}

#[test]
fn collect_world_views_prefers_explicit_viewports_over_implicit_main_camera() {
    let mut renderer = RenderComposer::from_asset(RenderPipelineAsset::overlay());
    renderer.surface_size = [800, 600];

    let mut world = World::new();
    world.spawn((
        Transform::from_xyz(10.0, 20.0, 30.0),
        Camera::new(),
        Projection::orthographic(320.0, 180.0),
        MainCamera,
    ));
    world.spawn((
        Transform::from_xyz(-4.0, 6.0, 8.0),
        Camera::new(),
        Projection::orthographic(160.0, 90.0),
        CameraViewport::new(ViewportRect::new(50, 40, 320, 200)).order(5),
    ));

    let resolved = renderer.resolve_scene_transforms(&world);
    let views = renderer.collect_world_views(&world, &resolved);

    assert_eq!(views.len(), 1);
    assert_eq!(views[0].order, 5);
    assert_eq!(views[0].viewport, ViewportRect::new(50, 40, 320, 200));
    assert_eq!(views[0].view_uniform.camera, [-4.0, 6.0, 8.0, 1.0]);
}

#[test]
fn collect_world_views_uses_main_camera_for_implicit_view_selection() {
    let mut renderer = RenderComposer::from_asset(RenderPipelineAsset::overlay());
    renderer.surface_size = [800, 600];

    let mut world = World::new();
    world.spawn((
        Transform::from_xyz(1.0, 2.0, 3.0),
        Camera::new(),
        Projection::orthographic(320.0, 180.0),
    ));
    world.spawn((
        Transform::from_xyz(11.0, 12.0, 13.0),
        Camera::new(),
        Projection::orthographic(640.0, 360.0),
        MainCamera,
    ));

    let resolved = renderer.resolve_scene_transforms(&world);
    let views = renderer.collect_world_views(&world, &resolved);

    assert_eq!(views.len(), 1);
    assert_eq!(views[0].viewport, ViewportRect::new(0, 0, 800, 600));
    assert_eq!(views[0].target_size, [800, 600]);
    assert_eq!(views[0].view_uniform.camera, [11.0, 12.0, 13.0, 1.0]);
}

#[test]
fn perspective_screen_to_world_intersects_the_world_z_plane() {
    let projection = Projection::perspective(60.0f32.to_radians(), 0.1, 1000.0);
    let transform = Transform::from_xyz(10.0, 20.0, 10.0);

    let center = projection.screen_to_world(transform, [800, 600], [400.0, 300.0]);
    assert!((center[0] - 10.0).abs() <= 0.001);
    assert!((center[1] - 20.0).abs() <= 0.001);

    let top_left = projection.screen_to_world(transform, [800, 600], [0.0, 0.0]);
    assert!(top_left[0] < transform.x());
    assert!(top_left[1] > transform.y());
    assert!(top_left.iter().all(|value| value.is_finite()));
}

#[test]
fn orthographic_screen_to_world_respects_camera_rotation() {
    let projection = Projection::orthographic(100.0, 50.0);
    let transform = Transform::new(10.0, 20.0).with_rotation(std::f32::consts::FRAC_PI_2);

    let center = projection.screen_to_world(transform, [200, 100], [100.0, 50.0]);
    assert!((center[0] - 10.0).abs() <= 0.001);
    assert!((center[1] - 20.0).abs() <= 0.001);

    let right_edge = projection.screen_to_world(transform, [200, 100], [200.0, 50.0]);
    assert!((right_edge[0] - 10.0).abs() <= 0.001);
    assert!((right_edge[1] - 70.0).abs() <= 0.001);
}

#[test]
fn perspective_view_extraction_keeps_camera_depth_and_disables_2d_culling() {
    let mut renderer = RenderComposer::from_asset(RenderPipelineAsset::overlay());
    renderer.surface_size = [1280, 720];

    let mut world = World::new();
    world.spawn((
        Transform::from_xyz(3.0, 4.0, 12.0),
        Camera::new(),
        Projection::perspective(60.0f32.to_radians(), 0.1, 500.0),
        CameraViewport::new(ViewportRect::new(0, 0, 640, 360)).order(2),
        MainCamera,
    ));

    let resolved = renderer.resolve_scene_transforms(&world);
    let views = renderer.collect_world_views(&world, &resolved);
    assert_eq!(views.len(), 1);
    let view = views[0];
    assert_eq!(view.order, 2);
    assert_eq!(view.target_size, [640, 360]);
    assert_eq!(view.view_uniform.camera, [3.0, 4.0, 12.0, 1.0]);
    assert!(view.cull_camera_2d.is_none());
    assert!(view
        .view_uniform
        .view_proj
        .iter()
        .all(|value| value.is_finite()));
}

#[test]
fn tilted_orthographic_view_disables_2d_culling() {
    let mut renderer = RenderComposer::from_asset(RenderPipelineAsset::overlay());
    renderer.surface_size = [800, 600];

    let mut world = World::new();
    world.spawn((
        Transform::from_xyz(0.0, 0.0, 10.0).with_euler_angles(0.35, 0.0, 0.0),
        Camera::new(),
        Projection::orthographic(320.0, 180.0),
        MainCamera,
    ));

    let resolved = renderer.resolve_scene_transforms(&world);
    let views = renderer.collect_world_views(&world, &resolved);
    assert_eq!(views.len(), 1);
    let view = views[0];
    assert!(view.cull_camera_2d.is_none());
    assert!(view
        .view_uniform
        .view_proj
        .iter()
        .all(|value| value.is_finite()));
}

#[cfg(feature = "live2d")]
#[test]
fn live2d_scene_sort_and_layer_visibility_follow_queue_policy() {
    let base = vec![
        Live2DSceneInstance {
            entity: EntityId::new(3, 0),
            model_index: 0,
            transform: Transform::from_xyz(0.0, 0.0, 0.8),
            layer_mask: 0b0001,
            sorting_layer: SortingLayer(1),
            order_in_layer: OrderInLayer(0),
        },
        Live2DSceneInstance {
            entity: EntityId::new(1, 0),
            model_index: 1,
            transform: Transform::from_xyz(0.0, 0.0, 0.2),
            layer_mask: 0b0010,
            sorting_layer: SortingLayer(0),
            order_in_layer: OrderInLayer(5),
        },
        Live2DSceneInstance {
            entity: EntityId::new(2, 0),
            model_index: 2,
            transform: Transform::from_xyz(0.0, 0.0, 0.1),
            layer_mask: 0b0010,
            sorting_layer: SortingLayer(1),
            order_in_layer: OrderInLayer(0),
        },
    ];

    let mut transparent = base.clone();
    sort_live2d_scene_instances(&mut transparent, RenderQueueSort::TransparentScene, None);
    assert_eq!(
        transparent
            .iter()
            .map(|item| (
                item.entity.index(),
                item.sorting_layer.0,
                item.order_in_layer.0
            ))
            .collect::<Vec<_>>(),
        vec![(1, 0, 5), (2, 1, 0), (3, 1, 0)]
    );

    let mut opaque = base.clone();
    sort_live2d_scene_instances(&mut opaque, RenderQueueSort::OpaqueDepthFrontToBack, None);
    assert_eq!(
        opaque
            .iter()
            .map(|item| (item.entity.index(), item.transform.z()))
            .collect::<Vec<_>>(),
        vec![(2, 0.1), (1, 0.2), (3, 0.8)]
    );

    let view = SceneView::new(
        0,
        ViewportRect::new(0, 0, 64, 64),
        [64, 64],
        true,
        0b0010,
        Transform::default(),
        Projection::orthographic(64.0, 64.0),
        Projection::orthographic(64.0, 64.0).view_uniform(Transform::default(), [64, 64]),
        orthographic_cull_camera(Transform::default(), Projection::orthographic(64.0, 64.0)),
    );
    assert!(!live2d_instance_visible_in_view(&base[0], &view));
    assert!(live2d_instance_visible_in_view(&base[1], &view));
    assert!(live2d_instance_visible_in_view(&base[2], &view));
}

#[cfg(feature = "live2d")]
#[test]
fn live2d_perspective_sort_uses_view_relative_depth() {
    let mut instances = vec![
        Live2DSceneInstance {
            entity: EntityId::new(1, 0),
            model_index: 0,
            transform: Transform::from_xyz(0.0, 0.0, 0.0),
            layer_mask: u32::MAX,
            sorting_layer: SortingLayer(0),
            order_in_layer: OrderInLayer(0),
        },
        Live2DSceneInstance {
            entity: EntityId::new(2, 0),
            model_index: 1,
            transform: Transform::from_xyz(0.0, 0.0, 5.0),
            layer_mask: u32::MAX,
            sorting_layer: SortingLayer(0),
            order_in_layer: OrderInLayer(0),
        },
    ];
    let perspective_view = SceneView::new(
        0,
        ViewportRect::new(0, 0, 64, 64),
        [64, 64],
        true,
        u32::MAX,
        Transform::from_xyz(0.0, 0.0, 10.0),
        Projection::perspective(60.0f32.to_radians(), 0.1, 1000.0),
        Projection::perspective(60.0f32.to_radians(), 0.1, 1000.0)
            .view_uniform(Transform::from_xyz(0.0, 0.0, 10.0), [64, 64]),
        None,
    );

    sort_live2d_scene_instances(
        &mut instances,
        RenderQueueSort::TransparentScene,
        Some(&perspective_view),
    );
    assert_eq!(
        instances
            .iter()
            .map(|instance| instance.entity.index())
            .collect::<Vec<_>>(),
        vec![1, 2]
    );

    sort_live2d_scene_instances(
        &mut instances,
        RenderQueueSort::OpaqueDepthFrontToBack,
        Some(&perspective_view),
    );
    assert_eq!(
        instances
            .iter()
            .map(|instance| instance.entity.index())
            .collect::<Vec<_>>(),
        vec![2, 1]
    );
}
