use crate::asset::{AssetConfig, AssetId, AssetServer, Handle, TextureAsset};
use crate::diagnostics::{
    DiagnosticSeverity, DiagnosticSubsystem, Diagnostics, EngineDiagnosticKind,
};
#[cfg(feature = "live2d")]
use crate::ecs::EntityId;
use crate::ecs::World;
use crate::gpu::GpuContext;
use crate::render::execution::{PreparedFrame, PreparedView};
use crate::render::expert::{
    BoundingSphere, Mesh, MeshDescriptor, MeshIndexData, RenderGraphError, TargetSize,
};
use crate::render::gpu::{read_render_target, RenderTarget, RenderTargetDescriptor};
use crate::render::graph::ImportedTexture;
#[cfg(feature = "live2d")]
use crate::render::live2d::{
    live2d_instance_visible_in_view, sort_live2d_scene_instances, Live2DSceneInstance,
};
use crate::render::pipeline::{
    ComputePass, GraphPass, GraphPassExecuteContext, GraphPassSetupContext, PostFxPass, RenderPass,
    RenderPhase, RenderPhaseExecuteContext, RenderPhaseSetupContext,
};
use crate::render::resources::assets::SharedRenderAssetCache;
use crate::render::view::Projection;
use crate::render::{
    CameraMarker, CameraViewport, Color, ComputePassExecuteContext, ComputePassSetupContext,
    DirectionalLight, MainCamera, MaterialError, PostFxPassExecuteContext, PostFxPassSetupContext,
    RenderComposer, RenderPassExecuteContext, RenderPassSetupContext, RenderPipelineAsset,
    RenderPipelineBuilder, RenderSettings, StandardMaterial, Transform, UnlitMaterial,
    ViewportRect, WgpuMeshRenderer,
};
#[cfg(feature = "live2d")]
use crate::render::{RenderQueueSort, SceneView, SortingLayer};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

fn create_test_device() -> (wgpu::Device, wgpu::Queue) {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        compatible_surface: None,
        force_fallback_adapter: false,
    }))
    .expect("No suitable GPU adapter found for composer tests");

    pollster::block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: Some("composer_test_device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::Performance,
        },
        None,
    ))
    .expect("Failed to create test GPU device")
}

#[test]
fn declared_materials_are_available_after_gpu_initialization_before_first_render() {
    let (device, queue) = create_test_device();
    let ctx = GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [64, 64]);
    let mut renderer = RenderComposer::from_asset(RenderPipelineAsset::modern_3d());

    renderer.initialize_for_gpu(&ctx);

    let material = renderer
        .materials_mut::<StandardMaterial>()
        .insert(StandardMaterial::default());
    assert!(renderer
        .materials::<StandardMaterial>()
        .get(material)
        .is_some());
}

#[test]
fn builder_unlit_pipeline_renders_default_sprite_scene() {
    let (device, queue) = create_test_device();
    let mut ctx =
        GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [64, 64]);
    let mut renderer = RenderComposer::from_asset(
        RenderPipelineAsset::builder()
            .add_feature(crate::render::SpriteFeature::unlit())
            .add_phase(crate::render::TransparentPhase::new())
            .build(),
    );
    let mut world = World::new();
    world.spawn((
        Transform::default(),
        CameraMarker::new(),
        Projection::orthographic_fixed(64.0, 64.0),
        MainCamera,
    ));
    world.spawn((
        Transform::default(),
        crate::render::SpriteRenderer::new(8.0, 8.0),
    ));

    ctx.begin_frame()
        .expect("headless begin_frame should succeed");
    renderer.render_world(&mut ctx, &world);
    ctx.end_frame();

    let stats = renderer.stats();
    assert_eq!(stats.view_count, 1);
    assert!(stats.passes >= 1);
    assert!(stats.step_count >= 1);
}

struct OrderedGraphPass {
    name: &'static str,
    executions: Arc<Mutex<Vec<&'static str>>>,
}

impl GraphPass for OrderedGraphPass {
    fn name(&self) -> &'static str {
        self.name
    }

    fn setup(&mut self, ctx: &mut GraphPassSetupContext<'_, '_>) {
        let target_size = ctx.view().target_size();
        let output = ctx.graph().create_texture(|builder| {
            builder
                .name(self.name)
                .size(TargetSize::Exact(target_size[0], target_size[1]))
                .format(wgpu::TextureFormat::Bgra8Unorm)
                .persistent();
        });
        ctx.graph().add_render_pass(self.name, |setup| {
            setup.write_color(0, output);
        });
    }

    fn execute(
        &mut self,
        ctx: &mut GraphPassExecuteContext<'_, '_>,
    ) -> Result<(), RenderGraphError> {
        if ctx.pass().name == self.name {
            self.executions
                .lock()
                .expect("order log lock")
                .push(self.name);
        }
        Ok(())
    }
}

#[test]
fn custom_graph_pass_execution_order_matches_builder_order() {
    let (device, queue) = create_test_device();
    let mut ctx =
        GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [32, 32]);
    let executions = Arc::new(Mutex::new(Vec::new()));
    let pipeline = RenderPipelineAsset::builder()
        .add_graph_pass(OrderedGraphPass {
            name: "graph_order_a",
            executions: executions.clone(),
        })
        .add_graph_pass(OrderedGraphPass {
            name: "graph_order_b",
            executions: executions.clone(),
        })
        .build();
    assert_eq!(
        pipeline.descriptor().step_names,
        vec![
            crate::render::PipelineStepDescriptor::Graph("graph_order_a"),
            crate::render::PipelineStepDescriptor::Graph("graph_order_b"),
        ]
    );
    let mut renderer = RenderComposer::from_asset(pipeline);
    let mut world = World::new();
    world.spawn((
        Transform::default(),
        CameraMarker::new(),
        Projection::orthographic_fixed(32.0, 32.0),
        MainCamera,
    ));

    ctx.begin_frame()
        .expect("headless begin_frame should succeed");
    renderer.render_world(&mut ctx, &world);
    ctx.end_frame();

    assert_eq!(
        executions.lock().expect("order log lock").as_slice(),
        ["graph_order_a", "graph_order_b"]
    );
}

#[test]
fn sprite_texture_asset_handle_uploads_into_render_cache() {
    let (device, queue) = create_test_device();
    let mut ctx =
        GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [64, 64]);
    let mut renderer = RenderComposer::from_asset(
        RenderPipelineAsset::builder()
            .add_feature(crate::render::SpriteFeature::unlit())
            .add_phase(crate::render::TransparentPhase::new())
            .build(),
    );
    let asset_server = AssetServer::with_empty_manifest(AssetConfig::default());
    let texture = asset_server.insert_runtime(TextureAsset::checkerboard(
        2,
        1,
        [255, 255, 255, 255],
        [32, 32, 32, 255],
    ));
    let mut world = World::new();
    world.insert_resource(asset_server.clone());
    world.insert_resource(SharedRenderAssetCache::default());
    world.spawn((
        Transform::default(),
        CameraMarker::new(),
        Projection::orthographic_fixed(64.0, 64.0),
        MainCamera,
    ));
    world.spawn((
        Transform::default(),
        crate::render::SpriteRenderer::new(8.0, 8.0).texture(texture),
    ));

    ctx.begin_frame()
        .expect("headless begin_frame should succeed");
    renderer.render_world(&mut ctx, &world);
    ctx.end_frame();

    let cache = world
        .get_resource::<SharedRenderAssetCache>()
        .expect("render asset cache should exist");
    assert_eq!(
        cache
            .borrow_mut()
            .texture_readiness(Some(&asset_server), texture),
        crate::render::TextureReadiness::GpuReady
    );
    let stats = renderer.stats();
    assert_eq!(stats.resident_render_assets, 1);
    assert_eq!(stats.uploaded_render_assets, 1);
    assert_eq!(stats.loading_render_assets, 1);
    assert_eq!(stats.fallback_render_assets, 1);
    assert_eq!(stats.missing_render_assets, 0);
    assert_eq!(stats.failed_render_assets, 0);

    ctx.begin_frame()
        .expect("second headless begin_frame should succeed");
    renderer.render_world(&mut ctx, &world);
    ctx.end_frame();

    assert!(cache.borrow_mut().contains_texture(texture));
    assert_eq!(
        cache
            .borrow_mut()
            .texture_readiness(Some(&asset_server), texture),
        crate::render::TextureReadiness::GpuReady
    );
    let stats = renderer.stats();
    assert_eq!(stats.resident_render_assets, 1);
    assert_eq!(stats.uploaded_render_assets, 0);
    assert_eq!(stats.loading_render_assets, 0);

    asset_server.unload(&texture);
    asset_server
        .update()
        .expect("runtime asset unload should update");
    ctx.begin_frame()
        .expect("third headless begin_frame should succeed");
    renderer.render_world(&mut ctx, &world);
    ctx.end_frame();

    assert!(!cache.borrow_mut().contains_texture(texture));
    let stats = renderer.stats();
    assert_eq!(stats.resident_render_assets, 0);
    assert_eq!(stats.missing_render_assets, 1);
}

#[test]
fn texture_asset_gpu_queue_uploads_requested_textures_on_following_frame() {
    let (device, queue) = create_test_device();
    let mut ctx =
        GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [64, 64]);
    let mut renderer = RenderComposer::from_asset(
        RenderPipelineAsset::builder()
            .add_feature(crate::render::SpriteFeature::unlit())
            .add_phase(crate::render::TransparentPhase::new())
            .build(),
    );

    let asset_server = AssetServer::with_empty_manifest(AssetConfig::default());
    let first = asset_server.insert_runtime(TextureAsset::white_pixel());
    let second = asset_server.insert_runtime(TextureAsset::checkerboard(
        2,
        1,
        [255, 255, 255, 255],
        [32, 32, 32, 255],
    ));
    let mut world = World::new();
    world.insert_resource(asset_server.clone());
    world.insert_resource(SharedRenderAssetCache::default());
    world.spawn((
        Transform::default(),
        CameraMarker::new(),
        Projection::orthographic_fixed(64.0, 64.0),
        MainCamera,
    ));
    world.spawn((
        Transform::default(),
        crate::render::SpriteRenderer::new(8.0, 8.0).texture(first),
    ));
    world.spawn((
        Transform::from_xyz(12.0, 0.0, 0.0),
        crate::render::SpriteRenderer::new(8.0, 8.0).texture(second),
    ));

    ctx.begin_frame()
        .expect("headless begin_frame should succeed");
    renderer.render_world(&mut ctx, &world);
    ctx.end_frame();

    let stats = renderer.stats();
    assert_eq!(stats.uploaded_render_assets, 2);
    assert_eq!(stats.resident_render_assets, 2);
    assert_eq!(stats.loading_render_assets, 2);
    assert_eq!(stats.fallback_render_assets, 2);
    let cache = world
        .get_resource::<SharedRenderAssetCache>()
        .expect("render asset cache should exist");
    assert_eq!(
        cache
            .borrow_mut()
            .texture_readiness(Some(&asset_server), first),
        crate::render::TextureReadiness::GpuReady
    );
    assert_eq!(
        cache
            .borrow_mut()
            .texture_readiness(Some(&asset_server), second),
        crate::render::TextureReadiness::GpuReady
    );

    ctx.begin_frame()
        .expect("second headless begin_frame should succeed");
    renderer.render_world(&mut ctx, &world);
    ctx.end_frame();

    let stats = renderer.stats();
    assert_eq!(stats.uploaded_render_assets, 0);
    assert_eq!(stats.resident_render_assets, 2);
    assert_eq!(stats.loading_render_assets, 0);
}

#[test]
fn visible_texture_gpu_requests_are_prepared_before_preloads() {
    let (device, queue) = create_test_device();
    let mut ctx =
        GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [64, 64]);
    let mut renderer = RenderComposer::from_asset(
        RenderPipelineAsset::builder()
            .add_feature(crate::render::SpriteFeature::unlit())
            .add_phase(crate::render::TransparentPhase::new())
            .build(),
    );

    let asset_server = AssetServer::with_empty_manifest(AssetConfig::default());
    let preloads = [
        asset_server.insert_runtime(TextureAsset::checkerboard(
            4,
            4,
            [255, 255, 255, 255],
            [32, 32, 32, 255],
        )),
        asset_server.insert_runtime(TextureAsset::checkerboard(
            4,
            4,
            [255, 255, 255, 255],
            [48, 48, 48, 255],
        )),
        asset_server.insert_runtime(TextureAsset::checkerboard(
            4,
            4,
            [255, 255, 255, 255],
            [64, 64, 64, 255],
        )),
        asset_server.insert_runtime(TextureAsset::checkerboard(
            4,
            4,
            [255, 255, 255, 255],
            [80, 80, 80, 255],
        )),
    ];
    let visible = asset_server.insert_runtime(TextureAsset::white_pixel());
    let mut world = World::new();
    world.insert_resource(asset_server.clone());
    world.insert_resource(SharedRenderAssetCache::default());
    world.spawn((
        Transform::default(),
        CameraMarker::new(),
        Projection::orthographic_fixed(64.0, 64.0),
        MainCamera,
    ));
    world.spawn((
        Transform::default(),
        crate::render::SpriteRenderer::new(8.0, 8.0).texture(visible),
    ));

    let cache = world
        .get_resource::<SharedRenderAssetCache>()
        .expect("render asset cache should exist");
    for preload in preloads {
        assert_eq!(
            cache
                .borrow_mut()
                .request_texture_gpu(&ctx, &asset_server, preload),
            crate::render::TextureReadiness::GpuQueued
        );
    }

    ctx.begin_frame()
        .expect("headless begin_frame should succeed");
    renderer.render_world(&mut ctx, &world);
    ctx.end_frame();

    let stats = renderer.stats();
    assert_eq!(stats.uploaded_render_assets, 4);
    assert_eq!(stats.queued_render_assets, 1);
    assert_eq!(stats.visible_queued_render_assets, 0);
    assert_eq!(stats.fallback_render_assets, 1);
    assert!(cache.borrow_mut().contains_texture(visible));
    let resident_preloads = preloads
        .into_iter()
        .filter(|preload| cache.borrow_mut().contains_texture(*preload))
        .count();
    assert_eq!(resident_preloads, 3);
    assert_eq!(stats.uploaded_render_asset_bytes, 3 * 4 * 4 * 4 + 4);

    ctx.begin_frame()
        .expect("second headless begin_frame should succeed");
    renderer.render_world(&mut ctx, &world);
    ctx.end_frame();

    assert!(cache.borrow_mut().contains_texture(visible));
    for preload in preloads {
        assert!(cache.borrow_mut().contains_texture(preload));
    }
}

#[test]
fn runtime_texture_replace_invalidates_gpu_texture_and_queues_reupload() {
    let (device, queue) = create_test_device();
    let mut ctx =
        GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [64, 64]);
    let mut renderer = RenderComposer::from_asset(
        RenderPipelineAsset::builder()
            .add_feature(crate::render::SpriteFeature::unlit())
            .add_phase(crate::render::TransparentPhase::new())
            .build(),
    );

    let asset_server = AssetServer::with_empty_manifest(AssetConfig::default());
    let texture = asset_server.insert_runtime(TextureAsset::white_pixel());
    let mut world = World::new();
    world.insert_resource(asset_server.clone());
    world.insert_resource(SharedRenderAssetCache::default());
    world.spawn((
        Transform::default(),
        CameraMarker::new(),
        Projection::orthographic_fixed(64.0, 64.0),
        MainCamera,
    ));
    world.spawn((
        Transform::default(),
        crate::render::SpriteRenderer::new(8.0, 8.0).texture(texture),
    ));

    ctx.begin_frame()
        .expect("headless begin_frame should succeed");
    renderer.render_world(&mut ctx, &world);
    ctx.end_frame();
    ctx.begin_frame()
        .expect("second headless begin_frame should succeed");
    renderer.render_world(&mut ctx, &world);
    ctx.end_frame();

    let cache = world
        .get_resource::<SharedRenderAssetCache>()
        .expect("render asset cache should exist");
    assert!(cache.borrow_mut().contains_texture(texture));

    asset_server
        .replace_runtime(
            texture,
            TextureAsset::checkerboard(2, 1, [255, 255, 255, 255], [32, 32, 32, 255]),
        )
        .expect("runtime texture replace should succeed");

    ctx.begin_frame()
        .expect("third headless begin_frame should succeed");
    renderer.render_world(&mut ctx, &world);
    ctx.end_frame();

    assert!(cache.borrow_mut().contains_texture(texture));
    assert_eq!(
        cache
            .borrow_mut()
            .texture_readiness(Some(&asset_server), texture),
        crate::render::TextureReadiness::GpuReady
    );
    let stats = renderer.stats();
    assert_eq!(stats.resident_render_assets, 1);
    assert_eq!(stats.uploaded_render_assets, 1);
    assert_eq!(stats.loading_render_assets, 1);
    assert_eq!(stats.fallback_render_assets, 1);

    ctx.begin_frame()
        .expect("fourth headless begin_frame should succeed");
    renderer.render_world(&mut ctx, &world);
    ctx.end_frame();

    assert!(cache.borrow_mut().contains_texture(texture));
    assert_eq!(renderer.stats().resident_render_assets, 1);
}

#[test]
fn wait_texture_gpu_prepares_queued_texture_immediately() {
    let (device, queue) = create_test_device();
    let ctx = GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [64, 64]);
    let asset_server = AssetServer::with_empty_manifest(AssetConfig::default());
    let texture = asset_server.insert_runtime(TextureAsset::checkerboard(
        2,
        1,
        [255, 255, 255, 255],
        [32, 32, 32, 255],
    ));
    let cache = SharedRenderAssetCache::default();
    assert_eq!(
        cache
            .borrow_mut()
            .request_texture_gpu(&ctx, &asset_server, texture),
        crate::render::TextureReadiness::GpuQueued
    );
    assert!(!cache.borrow_mut().contains_texture(texture));

    assert_eq!(
        cache.borrow_mut().wait_texture_gpu(
            &ctx,
            &asset_server,
            texture,
            std::time::Duration::from_millis(1),
        ),
        crate::render::TextureReadiness::GpuReady
    );
    assert!(cache.borrow_mut().contains_texture(texture));
}

#[test]
fn missing_sprite_texture_asset_is_reported_in_render_stats() {
    let (device, queue) = create_test_device();
    let mut ctx =
        GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [64, 64]);
    let mut renderer = RenderComposer::from_asset(
        RenderPipelineAsset::builder()
            .add_feature(crate::render::SpriteFeature::unlit())
            .add_phase(crate::render::TransparentPhase::new())
            .build(),
    );
    let missing = Handle::<TextureAsset>::new(AssetId::new());
    let mut world = World::new();
    world.insert_resource(Diagnostics::default());
    world.insert_resource(AssetServer::with_empty_manifest(AssetConfig::default()));
    world.insert_resource(SharedRenderAssetCache::default());
    world.spawn((
        Transform::default(),
        CameraMarker::new(),
        Projection::orthographic_fixed(64.0, 64.0),
        MainCamera,
    ));
    world.spawn((
        Transform::default(),
        crate::render::SpriteRenderer::new(8.0, 8.0).texture(missing),
    ));

    ctx.begin_frame()
        .expect("headless begin_frame should succeed");
    renderer.render_world(&mut ctx, &world);
    ctx.end_frame();

    let stats = renderer.stats();
    assert_eq!(stats.resident_render_assets, 0);
    assert_eq!(stats.uploaded_render_assets, 0);
    assert_eq!(stats.loading_render_assets, 0);
    assert_eq!(stats.missing_render_assets, 1);
    assert_eq!(stats.failed_render_assets, 0);

    let diagnostics = world
        .get_resource::<Diagnostics>()
        .expect("diagnostics should exist")
        .entries();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].id.as_str(), "render.asset.texture.missing");
    assert_eq!(diagnostics[0].subsystem, DiagnosticSubsystem::render());
    assert_eq!(diagnostics[0].severity, DiagnosticSeverity::Warning);
    assert_eq!(diagnostics[0].title, "Texture asset is missing");
    assert_eq!(
        diagnostics[0].help.as_deref(),
        Some(
            "Check that the texture is registered in the asset manifest or inserted as a runtime \
             asset before rendering."
        )
    );
    let missing_id = missing.id().to_string();
    assert_eq!(diagnostics[0].field("asset_id"), Some(missing_id.as_str()));
}

#[test]
fn forward_3d_descriptor_includes_directional_shadow_phase() {
    let descriptor = RenderPipelineAsset::forward_3d().descriptor();
    assert!(descriptor.step_names.iter().any(|step| matches!(
        step,
        crate::render::PipelineStepDescriptor::Phase("directional_shadow")
    )));
    assert!(descriptor.step_names.iter().any(|step| matches!(
        step,
        crate::render::PipelineStepDescriptor::Compute("gi_update")
    )));
    assert!(!descriptor.step_names.iter().any(|step| matches!(
        step,
        crate::render::PipelineStepDescriptor::Phase("scene_material_prepass")
    )));
}

#[test]
fn modern_3d_descriptor_uses_wicked_style_pass_order() {
    use crate::render::PipelineStepDescriptor::{Compute, Phase, PostFx};

    let descriptor = RenderPipelineAsset::modern_3d().descriptor();
    let steps = descriptor.step_names;
    let expected = [
        Phase("scene_normal_prepass"),
        Phase("scene_material_prepass"),
        Phase("directional_shadow"),
        Compute("gi_update"),
        Phase("opaque"),
        PostFx("contact_shadows"),
        PostFx("gi_composite"),
        Phase("transparent"),
        PostFx("taa"),
        PostFx("sharpen"),
        PostFx("bloom"),
        PostFx("tonemap"),
        PostFx("debug_view"),
    ];
    assert_eq!(steps.as_slice(), &expected);
}

#[test]
fn forward_3d_enables_shadow_view_for_perspective_directional_light() {
    #[repr(C)]
    #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    struct Vertex {
        position: [f32; 3],
        normal: [f32; 3],
        uv: [f32; 2],
    }

    let (device, queue) = create_test_device();
    let mut ctx =
        GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [96, 96]);
    let mut renderer = RenderComposer::from_asset(RenderPipelineAsset::forward_3d());
    renderer.register_material::<StandardMaterial>(&ctx);

    let vertices = [
        Vertex {
            position: [-1.0, -1.0, 0.0],
            normal: [0.0, 0.0, 1.0],
            uv: [0.0, 1.0],
        },
        Vertex {
            position: [1.0, -1.0, 0.0],
            normal: [0.0, 0.0, 1.0],
            uv: [1.0, 1.0],
        },
        Vertex {
            position: [1.0, 1.0, 0.0],
            normal: [0.0, 0.0, 1.0],
            uv: [1.0, 0.0],
        },
        Vertex {
            position: [-1.0, 1.0, 0.0],
            normal: [0.0, 0.0, 1.0],
            uv: [0.0, 0.0],
        },
    ];
    let indices = [0u16, 1, 2, 0, 2, 3];
    let mesh_handle = renderer.insert_mesh(Mesh::from_raw(
        &ctx,
        MeshDescriptor::new(
            bytemuck::cast_slice(&vertices),
            vertices.len() as u32,
            Mesh::vertex_layout_position_normal_uv(),
            "shadow_quad",
        )
        .with_indices(MeshIndexData::U16(&indices)),
    ));
    let material = renderer
        .materials_mut::<StandardMaterial>()
        .insert(StandardMaterial::default());

    let mut world = World::new();
    world.spawn((
        Transform::default(),
        CameraMarker::new(),
        Projection::perspective(60.0f32.to_radians(), 0.1, 32.0),
        MainCamera,
    ));
    world.spawn((
        Transform::from_xyz(0.0, 0.0, -3.0),
        WgpuMeshRenderer::new(mesh_handle, material),
    ));
    world.spawn((DirectionalLight::new([0.3, -1.0, 0.2]).shadow_filter_radius(0.05),));

    ctx.begin_frame()
        .expect("headless begin_frame should succeed");
    renderer.render_world(&mut ctx, &world);
    ctx.end_frame();

    assert_eq!(renderer.shadows.views.len(), 1);
    assert!(renderer.shadows.views[0].enabled());
    assert!(renderer.shadows.views[0].caster_count() > 0);
    assert!((renderer.shadows.views[0].radius() - 0.05).abs() < 0.0001);
}

#[test]
fn forward_3d_directional_shadow_atlas_writes_depth_for_casters() {
    #[repr(C)]
    #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    struct Vertex {
        position: [f32; 3],
        normal: [f32; 3],
        uv: [f32; 2],
    }

    let (device, queue) = create_test_device();
    let mut ctx =
        GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [96, 96]);
    let mut renderer = RenderComposer::from_asset(RenderPipelineAsset::forward_3d());
    renderer.register_material::<StandardMaterial>(&ctx);

    let vertices = [
        Vertex {
            position: [-0.5, -0.5, 0.5],
            normal: [0.0, 0.0, 1.0],
            uv: [0.0, 1.0],
        },
        Vertex {
            position: [0.5, -0.5, 0.5],
            normal: [0.0, 0.0, 1.0],
            uv: [1.0, 1.0],
        },
        Vertex {
            position: [0.5, 0.5, 0.5],
            normal: [0.0, 0.0, 1.0],
            uv: [1.0, 0.0],
        },
        Vertex {
            position: [-0.5, 0.5, 0.5],
            normal: [0.0, 0.0, 1.0],
            uv: [0.0, 0.0],
        },
        Vertex {
            position: [0.5, -0.5, -0.5],
            normal: [0.0, 0.0, -1.0],
            uv: [0.0, 1.0],
        },
        Vertex {
            position: [-0.5, -0.5, -0.5],
            normal: [0.0, 0.0, -1.0],
            uv: [1.0, 1.0],
        },
        Vertex {
            position: [-0.5, 0.5, -0.5],
            normal: [0.0, 0.0, -1.0],
            uv: [1.0, 0.0],
        },
        Vertex {
            position: [0.5, 0.5, -0.5],
            normal: [0.0, 0.0, -1.0],
            uv: [0.0, 0.0],
        },
        Vertex {
            position: [-1.0, -1.0, 0.0],
            normal: [-1.0, 0.0, 0.0],
            uv: [0.0, 1.0],
        },
        Vertex {
            position: [-0.5, -0.5, 0.5],
            normal: [-1.0, 0.0, 0.0],
            uv: [1.0, 1.0],
        },
        Vertex {
            position: [-0.5, 0.5, 0.5],
            normal: [-1.0, 0.0, 0.0],
            uv: [1.0, 0.0],
        },
        Vertex {
            position: [-0.5, 0.5, -0.5],
            normal: [-1.0, 0.0, 0.0],
            uv: [0.0, 0.0],
        },
        Vertex {
            position: [0.5, -0.5, 0.5],
            normal: [1.0, 0.0, 0.0],
            uv: [0.0, 1.0],
        },
        Vertex {
            position: [0.5, -0.5, -0.5],
            normal: [1.0, 0.0, 0.0],
            uv: [1.0, 1.0],
        },
        Vertex {
            position: [0.5, 0.5, -0.5],
            normal: [1.0, 0.0, 0.0],
            uv: [1.0, 0.0],
        },
        Vertex {
            position: [0.5, 0.5, 0.5],
            normal: [1.0, 0.0, 0.0],
            uv: [0.0, 0.0],
        },
        Vertex {
            position: [-0.5, 0.5, 0.5],
            normal: [0.0, 1.0, 0.0],
            uv: [0.0, 1.0],
        },
        Vertex {
            position: [0.5, 0.5, 0.5],
            normal: [0.0, 1.0, 0.0],
            uv: [1.0, 1.0],
        },
        Vertex {
            position: [0.5, 0.5, -0.5],
            normal: [0.0, 1.0, 0.0],
            uv: [1.0, 0.0],
        },
        Vertex {
            position: [-0.5, 0.5, -0.5],
            normal: [0.0, 1.0, 0.0],
            uv: [0.0, 0.0],
        },
        Vertex {
            position: [-0.5, -0.5, -0.5],
            normal: [0.0, -1.0, 0.0],
            uv: [0.0, 1.0],
        },
        Vertex {
            position: [0.5, -0.5, -0.5],
            normal: [0.0, -1.0, 0.0],
            uv: [1.0, 1.0],
        },
        Vertex {
            position: [0.5, -0.5, 0.5],
            normal: [0.0, -1.0, 0.0],
            uv: [1.0, 0.0],
        },
        Vertex {
            position: [-0.5, -0.5, 0.5],
            normal: [0.0, -1.0, 0.0],
            uv: [0.0, 0.0],
        },
    ];
    let indices: [u16; 36] = [
        0, 1, 2, 0, 2, 3, 4, 5, 6, 4, 6, 7, 8, 9, 10, 8, 10, 11, 12, 13, 14, 12, 14, 15, 16, 17,
        18, 16, 18, 19, 20, 21, 22, 20, 22, 23,
    ];
    let mesh_handle = renderer.insert_mesh(Mesh::from_raw(
        &ctx,
        MeshDescriptor::new(
            bytemuck::cast_slice(&vertices),
            vertices.len() as u32,
            Mesh::vertex_layout_position_normal_uv(),
            "shadow_depth_readback_cube",
        )
        .with_indices(MeshIndexData::U16(&indices))
        .with_bounding_sphere(BoundingSphere::new(
            [0.0, 0.0, 0.0],
            (0.5f32 * 0.5 + 0.5 * 0.5 + 0.5 * 0.5).sqrt(),
        )),
    ));
    let material = renderer
        .materials_mut::<StandardMaterial>()
        .insert(StandardMaterial::default());

    let mut world = World::new();
    world.spawn((
        Transform::default(),
        CameraMarker::new(),
        Projection::perspective(60.0f32.to_radians(), 0.1, 32.0),
        MainCamera,
    ));
    world.spawn((
        Transform::from_xyz(0.0, 0.0, -3.0),
        WgpuMeshRenderer::new(mesh_handle, material),
    ));
    world.spawn((DirectionalLight::new([0.3, -1.0, 0.2]).shadow_map_size(64),));

    ctx.begin_frame()
        .expect("headless begin_frame should succeed");
    renderer.render_world(&mut ctx, &world);
    ctx.end_frame();

    let stats = renderer.stats();
    assert_eq!(stats.shadow_cascade_count, 1);
    assert_eq!(stats.shadow_caster_count, 1);
    assert_eq!(stats.shadow_draw_calls, 1);

    let shadow_view = renderer
        .shadows
        .views
        .first()
        .expect("directional shadow binding should exist");
    assert!(shadow_view.enabled());
    let readback =
        read_render_target(&ctx, shadow_view.target()).expect("shadow atlas readback should work");
    let mut min_depth = f32::INFINITY;
    let mut max_depth = f32::NEG_INFINITY;
    let mut below_clear_depth_count = 0usize;
    let mut written_depth_count = 0usize;
    for bytes in readback.data().chunks_exact(4) {
        let depth = f32::from_le_bytes(bytes.try_into().unwrap());
        if !depth.is_finite() {
            continue;
        }
        min_depth = min_depth.min(depth);
        max_depth = max_depth.max(depth);
        if depth < 1.0 {
            below_clear_depth_count += 1;
        }
        if depth < 0.999 {
            written_depth_count += 1;
        }
    }

    assert!(
        written_depth_count > 0,
        "shadow atlas should contain caster depth values below the clear depth; min_depth={min_depth}, max_depth={max_depth}, below_clear_depth_count={below_clear_depth_count}"
    );
}

#[test]
fn directional_shadow_atlas_draws_caster_between_light_and_near_cascade() {
    #[repr(C)]
    #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    struct Vertex {
        position: [f32; 3],
        normal: [f32; 3],
        uv: [f32; 2],
    }

    let (device, queue) = create_test_device();
    let mut ctx =
        GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [96, 96]);
    let mut renderer = RenderComposer::from_asset(
        RenderPipelineAsset::builder()
            .register_material::<StandardMaterial>()
            .add_phase(crate::render::lighting::shadow::DirectionalShadowPhase::new())
            .add_phase(crate::render::OpaquePhase::new())
            .build(),
    );
    renderer.register_material::<StandardMaterial>(&ctx);

    let vertices = [
        Vertex {
            position: [-0.5, -0.5, 0.5],
            normal: [0.0, 0.0, 1.0],
            uv: [0.0, 1.0],
        },
        Vertex {
            position: [0.5, -0.5, 0.5],
            normal: [0.0, 0.0, 1.0],
            uv: [1.0, 1.0],
        },
        Vertex {
            position: [0.5, 0.5, 0.5],
            normal: [0.0, 0.0, 1.0],
            uv: [1.0, 0.0],
        },
        Vertex {
            position: [-0.5, 0.5, 0.5],
            normal: [0.0, 0.0, 1.0],
            uv: [0.0, 0.0],
        },
        Vertex {
            position: [0.5, -0.5, -0.5],
            normal: [0.0, 0.0, -1.0],
            uv: [0.0, 1.0],
        },
        Vertex {
            position: [-0.5, -0.5, -0.5],
            normal: [0.0, 0.0, -1.0],
            uv: [1.0, 1.0],
        },
        Vertex {
            position: [-0.5, 0.5, -0.5],
            normal: [0.0, 0.0, -1.0],
            uv: [1.0, 0.0],
        },
        Vertex {
            position: [0.5, 0.5, -0.5],
            normal: [0.0, 0.0, -1.0],
            uv: [0.0, 0.0],
        },
    ];
    let indices: [u16; 12] = [0, 1, 2, 0, 2, 3, 4, 5, 6, 4, 6, 7];
    let mesh_handle = renderer.insert_mesh(Mesh::from_raw(
        &ctx,
        MeshDescriptor::new(
            bytemuck::cast_slice(&vertices),
            vertices.len() as u32,
            Mesh::vertex_layout_position_normal_uv(),
            "near_cascade_light_ray_caster",
        )
        .with_indices(MeshIndexData::U16(&indices))
        .with_bounding_sphere(BoundingSphere::new([0.0, 0.0, 0.0], 0.87)),
    ));
    let material = renderer
        .materials_mut::<StandardMaterial>()
        .insert(StandardMaterial::default());

    let mut world = World::new();
    world.insert_resource(RenderSettings {
        clear_color: Color::BLACK,
        ambient_color: Color::BLACK,
        global_illumination: crate::render::GlobalIllumination::Off,
        ..RenderSettings::default()
    });
    world.spawn((
        Transform::default(),
        CameraMarker::new(),
        Projection::perspective(60.0f32.to_radians(), 0.1, 120.0),
        MainCamera,
    ));
    world.spawn((
        Transform::from_xyz(-2.8, 8.0, -3.5),
        WgpuMeshRenderer::new(mesh_handle, material).shadow_lod_cascades(1),
    ));
    world.spawn((DirectionalLight::new([0.35, -1.0, -0.25])
        .cascade_count(4)
        .cascade_distances([8.0, 24.0, 60.0, 120.0])
        .shadow_map_size(128)
        .shadow_bias(0.0)
        .shadow_depth_bias(0)
        .shadow_slope_bias(0.0)
        .shadow_normal_bias(0.0)
        .shadow_filter_radius(0.0),));

    ctx.begin_frame()
        .expect("headless begin_frame should succeed");
    renderer.render_world(&mut ctx, &world);
    ctx.end_frame();

    let stats = renderer.stats();
    assert_eq!(stats.shadow_cascade_count, 4);
    assert_eq!(stats.shadow_caster_count_by_cascade[0], 1);
    assert_eq!(stats.shadow_draw_calls_by_cascade[0], 1);
    let shadow_view = renderer
        .shadows
        .views
        .first()
        .expect("directional shadow binding should exist");
    let readback =
        read_render_target(&ctx, shadow_view.target()).expect("shadow atlas readback should work");
    let cascade_width = readback.width() / stats.shadow_cascade_count.max(1) as u32;
    let mut written_first_cascade = 0usize;
    for y in 0..readback.height() {
        for x in 0..cascade_width {
            let index = ((y * readback.width() + x) * readback.bytes_per_pixel()) as usize;
            let depth = f32::from_le_bytes(readback.data()[index..index + 4].try_into().unwrap());
            if depth < 0.999 {
                written_first_cascade += 1;
            }
        }
    }
    assert!(
        written_first_cascade > 0,
        "first cascade should contain depth from the light-ray caster"
    );
}

#[derive(Clone)]
struct CaptureCurrentColorPass {
    target: Arc<RenderTarget>,
}

impl CaptureCurrentColorPass {
    fn import_target(&self) -> ImportedTexture {
        ImportedTexture {
            texture: Arc::new(self.target.texture().clone()),
            view: Arc::new(self.target.view().clone()),
            size: [self.target.width(), self.target.height()],
            format: self.target.format(),
            usage: self.target.usage(),
            sample_count: self.target.sample_count(),
            mip_level_count: self.target.mip_level_count(),
            array_layer_count: self.target.array_layer_count(),
        }
    }
}

impl PostFxPass for CaptureCurrentColorPass {
    fn name(&self) -> &'static str {
        "capture_current_color"
    }

    fn setup(&mut self, ctx: &mut PostFxPassSetupContext<'_, '_>) {
        let Some(current) = ctx.state().current_color() else {
            return;
        };
        let capture = ctx.graph().create_texture(|builder| {
            builder
                .name("captured_current_color")
                .import_external(self.import_target());
        });
        ctx.graph().add_copy_pass(self.name(), |setup| {
            setup.texture_to_texture(current.handle(), capture);
        });
        ctx.state().set_current_color(capture, current.format());
    }
}

#[test]
fn standard_material_directional_shadow_darkens_final_color() {
    #[repr(C)]
    #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    struct Vertex {
        position: [f32; 3],
        normal: [f32; 3],
        uv: [f32; 2],
    }

    fn plane_mesh(ctx: &GpuContext, label: &'static str) -> Mesh {
        let vertices = [
            Vertex {
                position: [-1.0, -1.0, 0.0],
                normal: [0.0, 0.0, 1.0],
                uv: [0.0, 1.0],
            },
            Vertex {
                position: [1.0, -1.0, 0.0],
                normal: [0.0, 0.0, 1.0],
                uv: [1.0, 1.0],
            },
            Vertex {
                position: [1.0, 1.0, 0.0],
                normal: [0.0, 0.0, 1.0],
                uv: [1.0, 0.0],
            },
            Vertex {
                position: [-1.0, 1.0, 0.0],
                normal: [0.0, 0.0, 1.0],
                uv: [0.0, 0.0],
            },
        ];
        Mesh::from_raw(
            ctx,
            MeshDescriptor::new(
                bytemuck::cast_slice(&vertices),
                vertices.len() as u32,
                Mesh::vertex_layout_position_normal_uv(),
                label,
            )
            .with_indices(MeshIndexData::U16(&[0, 1, 2, 0, 2, 3]))
            .with_bounding_sphere(BoundingSphere::new([0.0, 0.0, 0.0], (2.0f32).sqrt())),
        )
    }

    fn box_mesh(ctx: &GpuContext, label: &'static str) -> Mesh {
        let vertices = [
            Vertex {
                position: [-0.5, -0.5, 0.5],
                normal: [0.0, 0.0, 1.0],
                uv: [0.0, 1.0],
            },
            Vertex {
                position: [0.5, -0.5, 0.5],
                normal: [0.0, 0.0, 1.0],
                uv: [1.0, 1.0],
            },
            Vertex {
                position: [0.5, 0.5, 0.5],
                normal: [0.0, 0.0, 1.0],
                uv: [1.0, 0.0],
            },
            Vertex {
                position: [-0.5, 0.5, 0.5],
                normal: [0.0, 0.0, 1.0],
                uv: [0.0, 0.0],
            },
            Vertex {
                position: [0.5, -0.5, -0.5],
                normal: [0.0, 0.0, -1.0],
                uv: [0.0, 1.0],
            },
            Vertex {
                position: [-0.5, -0.5, -0.5],
                normal: [0.0, 0.0, -1.0],
                uv: [1.0, 1.0],
            },
            Vertex {
                position: [-0.5, 0.5, -0.5],
                normal: [0.0, 0.0, -1.0],
                uv: [1.0, 0.0],
            },
            Vertex {
                position: [0.5, 0.5, -0.5],
                normal: [0.0, 0.0, -1.0],
                uv: [0.0, 0.0],
            },
            Vertex {
                position: [-0.5, -0.5, -0.5],
                normal: [-1.0, 0.0, 0.0],
                uv: [0.0, 1.0],
            },
            Vertex {
                position: [-0.5, -0.5, 0.5],
                normal: [-1.0, 0.0, 0.0],
                uv: [1.0, 1.0],
            },
            Vertex {
                position: [-0.5, 0.5, 0.5],
                normal: [-1.0, 0.0, 0.0],
                uv: [1.0, 0.0],
            },
            Vertex {
                position: [-0.5, 0.5, -0.5],
                normal: [-1.0, 0.0, 0.0],
                uv: [0.0, 0.0],
            },
            Vertex {
                position: [0.5, -0.5, 0.5],
                normal: [1.0, 0.0, 0.0],
                uv: [0.0, 1.0],
            },
            Vertex {
                position: [0.5, -0.5, -0.5],
                normal: [1.0, 0.0, 0.0],
                uv: [1.0, 1.0],
            },
            Vertex {
                position: [0.5, 0.5, -0.5],
                normal: [1.0, 0.0, 0.0],
                uv: [1.0, 0.0],
            },
            Vertex {
                position: [0.5, 0.5, 0.5],
                normal: [1.0, 0.0, 0.0],
                uv: [0.0, 0.0],
            },
            Vertex {
                position: [-0.5, 0.5, 0.5],
                normal: [0.0, 1.0, 0.0],
                uv: [0.0, 1.0],
            },
            Vertex {
                position: [0.5, 0.5, 0.5],
                normal: [0.0, 1.0, 0.0],
                uv: [1.0, 1.0],
            },
            Vertex {
                position: [0.5, 0.5, -0.5],
                normal: [0.0, 1.0, 0.0],
                uv: [1.0, 0.0],
            },
            Vertex {
                position: [-0.5, 0.5, -0.5],
                normal: [0.0, 1.0, 0.0],
                uv: [0.0, 0.0],
            },
            Vertex {
                position: [-0.5, -0.5, -0.5],
                normal: [0.0, -1.0, 0.0],
                uv: [0.0, 1.0],
            },
            Vertex {
                position: [0.5, -0.5, -0.5],
                normal: [0.0, -1.0, 0.0],
                uv: [1.0, 1.0],
            },
            Vertex {
                position: [0.5, -0.5, 0.5],
                normal: [0.0, -1.0, 0.0],
                uv: [1.0, 0.0],
            },
            Vertex {
                position: [-0.5, -0.5, 0.5],
                normal: [0.0, -1.0, 0.0],
                uv: [0.0, 0.0],
            },
        ];
        let indices: [u16; 36] = [
            0, 1, 2, 0, 2, 3, 4, 5, 6, 4, 6, 7, 8, 9, 10, 8, 10, 11, 12, 13, 14, 12, 14, 15, 16,
            17, 18, 16, 18, 19, 20, 21, 22, 20, 22, 23,
        ];
        Mesh::from_raw(
            ctx,
            MeshDescriptor::new(
                bytemuck::cast_slice(&vertices),
                vertices.len() as u32,
                Mesh::vertex_layout_position_normal_uv(),
                label,
            )
            .with_indices(MeshIndexData::U16(&indices))
            .with_bounding_sphere(BoundingSphere::new([0.0, 0.0, 0.0], (0.75f32).sqrt())),
        )
    }

    fn render_scene(
        receive_shadows: bool,
    ) -> (Vec<[f32; 3]>, u32, u32, crate::render::view::RenderStats) {
        let (device, queue) = create_test_device();
        let mut ctx =
            GpuContext::new_headless(device, queue, wgpu::TextureFormat::Rgba8Unorm, [96, 96]);
        let capture = Arc::new(RenderTarget::from_descriptor(
            &ctx,
            RenderTargetDescriptor::new(96, 96, wgpu::TextureFormat::Rgba8Unorm)
                .label("standard_material_shadow_capture"),
        ));
        let mut renderer = RenderComposer::from_asset(
            RenderPipelineAsset::builder()
                .register_material::<StandardMaterial>()
                .add_phase(crate::render::lighting::shadow::DirectionalShadowPhase::new())
                .add_compute(crate::render::GiUpdateCompute::default())
                .add_phase(crate::render::OpaquePhase::new())
                .add_postfx(CaptureCurrentColorPass {
                    target: capture.clone(),
                })
                .build(),
        );
        renderer.register_material::<StandardMaterial>(&ctx);

        let plane = renderer.insert_mesh(plane_mesh(&ctx, "shadow_receiver_plane"));
        let cube = renderer.insert_mesh(box_mesh(&ctx, "shadow_caster_cube"));
        let receiver_material =
            renderer
                .materials_mut::<StandardMaterial>()
                .insert(StandardMaterial {
                    albedo: Color::WHITE,
                    roughness: 0.8,
                    receive_shadows,
                    ..StandardMaterial::default()
                });
        let caster_material =
            renderer
                .materials_mut::<StandardMaterial>()
                .insert(StandardMaterial {
                    albedo: Color::BLACK,
                    roughness: 1.0,
                    receive_shadows: false,
                    ..StandardMaterial::default()
                });

        let mut world = World::new();
        world.insert_resource(RenderSettings {
            clear_color: Color::BLACK,
            ambient_color: Color::BLACK,
            global_illumination: crate::render::GlobalIllumination::Off,
            bloom: crate::render::BloomSettings {
                enabled: false,
                ..Default::default()
            },
            tonemap: crate::render::ToneMapSettings {
                enabled: false,
                ..Default::default()
            },
            temporal_aa: crate::render::TemporalAntiAliasingSettings {
                enabled: false,
                ..Default::default()
            },
            vignette: crate::render::VignetteSettings {
                enabled: false,
                ..Default::default()
            },
            contact_shadows: crate::render::ContactShadowsSettings {
                enabled: false,
                ..Default::default()
            },
            ..RenderSettings::default()
        });
        world.spawn((
            Transform::default(),
            CameraMarker::new(),
            Projection::perspective(60.0f32.to_radians(), 0.1, 32.0),
            MainCamera,
        ));
        world.spawn((
            Transform::from_xyz(0.0, 0.0, -5.0).with_scale3(2.4, 2.4, 1.0),
            WgpuMeshRenderer::new(plane, receiver_material).casts_shadows(false),
        ));
        world.spawn((
            Transform::from_xyz(-0.65, 0.0, -4.0).with_scale3(0.7, 0.7, 0.7),
            WgpuMeshRenderer::new(cube, caster_material),
        ));
        world.spawn((DirectionalLight::new([0.65, 0.0, -1.0])
            .intensity(8.0)
            .color(Color::WHITE)
            .shadow_map_size(256)
            .shadow_bias(0.0)
            .shadow_depth_bias(0)
            .shadow_slope_bias(0.0)
            .shadow_normal_bias(0.0)
            .shadow_filter_radius(0.0),));

        ctx.begin_frame()
            .expect("headless begin_frame should succeed");
        renderer.render_world(&mut ctx, &world);
        ctx.end_frame();

        let readback =
            read_render_target(&ctx, &capture).expect("captured color readback should work");
        assert_eq!(readback.format(), wgpu::TextureFormat::Rgba8Unorm);
        let pixels = readback
            .data()
            .chunks_exact(readback.bytes_per_pixel() as usize)
            .map(|pixel| {
                [
                    pixel[0] as f32 / 255.0,
                    pixel[1] as f32 / 255.0,
                    pixel[2] as f32 / 255.0,
                ]
            })
            .collect();
        (
            pixels,
            readback.width(),
            readback.height(),
            renderer.stats(),
        )
    }

    let (shadowed, width, height, shadowed_stats) = render_scene(true);
    let (unshadowed, unshadowed_width, unshadowed_height, unshadowed_stats) = render_scene(false);
    assert_eq!([width, height], [unshadowed_width, unshadowed_height]);
    let center_index = (48 * width + 48) as usize;
    let shadowed_center = shadowed[center_index];
    let unshadowed_center = unshadowed[center_index];
    let center_delta = (unshadowed_center[0] + unshadowed_center[1] + unshadowed_center[2])
        - (shadowed_center[0] + shadowed_center[1] + shadowed_center[2]);
    let mut max_delta = f32::NEG_INFINITY;
    let mut max_delta_pixel = [0u32; 2];
    let mut max_shadowed = [0.0; 3];
    let mut max_unshadowed = [0.0; 3];
    for y in 16..(height - 16) {
        for x in 16..(width - 16) {
            let index = (y * width + x) as usize;
            let shadowed_luma = shadowed[index][0] + shadowed[index][1] + shadowed[index][2];
            let unshadowed_luma =
                unshadowed[index][0] + unshadowed[index][1] + unshadowed[index][2];
            let delta = unshadowed_luma - shadowed_luma;
            if delta > max_delta {
                max_delta = delta;
                max_delta_pixel = [x, y];
                max_shadowed = shadowed[index];
                max_unshadowed = unshadowed[index];
            }
        }
    }

    assert_eq!(shadowed_stats.shadow_caster_count, 1);
    assert_eq!(shadowed_stats.shadow_draw_calls, 1);
    assert_eq!(unshadowed_stats.shadow_caster_count, 1);
    assert_eq!(unshadowed_stats.shadow_draw_calls, 1);
    assert!(
        max_delta > 0.25,
        "expected receive_shadows=true to darken at least one receiver pixel; center_delta={center_delta}, max_delta={max_delta} at {max_delta_pixel:?}, shadowed={max_shadowed:?}, unshadowed={max_unshadowed:?}"
    );
}

#[test]
fn forward_3d_enables_shadow_view_for_orthographic_directional_light() {
    #[repr(C)]
    #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    struct Vertex {
        position: [f32; 3],
        normal: [f32; 3],
        uv: [f32; 2],
    }

    let (device, queue) = create_test_device();
    let mut ctx =
        GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [96, 96]);
    let mut renderer = RenderComposer::from_asset(RenderPipelineAsset::forward_3d());
    renderer.register_material::<StandardMaterial>(&ctx);

    let vertices = [
        Vertex {
            position: [-1.0, -1.0, 0.0],
            normal: [0.0, 0.0, 1.0],
            uv: [0.0, 1.0],
        },
        Vertex {
            position: [1.0, -1.0, 0.0],
            normal: [0.0, 0.0, 1.0],
            uv: [1.0, 1.0],
        },
        Vertex {
            position: [1.0, 1.0, 0.0],
            normal: [0.0, 0.0, 1.0],
            uv: [1.0, 0.0],
        },
        Vertex {
            position: [-1.0, 1.0, 0.0],
            normal: [0.0, 0.0, 1.0],
            uv: [0.0, 0.0],
        },
    ];
    let indices = [0u16, 1, 2, 0, 2, 3];
    let mesh_handle = renderer.insert_mesh(Mesh::from_raw(
        &ctx,
        MeshDescriptor::new(
            bytemuck::cast_slice(&vertices),
            vertices.len() as u32,
            Mesh::vertex_layout_position_normal_uv(),
            "shadow_quad_ortho",
        )
        .with_indices(MeshIndexData::U16(&indices)),
    ));
    let material = renderer
        .materials_mut::<StandardMaterial>()
        .insert(StandardMaterial::default());

    let mut world = World::new();
    world.spawn((
        Transform::from_xyz(0.0, 0.0, 8.0),
        CameraMarker::new(),
        Projection::orthographic_fixed(12.0, 12.0),
        MainCamera,
    ));
    world.spawn((
        Transform::from_xyz(0.0, 0.0, 0.0).with_scale(4.0, 4.0),
        WgpuMeshRenderer::new(mesh_handle, material),
    ));
    world.spawn((DirectionalLight::new([0.3, -1.0, 0.2]),));

    ctx.begin_frame()
        .expect("headless begin_frame should succeed");
    renderer.render_world(&mut ctx, &world);
    ctx.end_frame();

    assert_eq!(renderer.shadows.views.len(), 1);
    assert!(renderer.shadows.views[0].enabled());
    assert!(renderer.shadows.views[0].caster_count() > 0);
}

#[test]
fn forward_3d_shadow_stats_track_lod_mask_per_cascade() {
    #[repr(C)]
    #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    struct Vertex {
        position: [f32; 3],
        normal: [f32; 3],
        uv: [f32; 2],
    }

    let (device, queue) = create_test_device();
    let mut ctx =
        GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [96, 96]);
    let mut renderer = RenderComposer::from_asset(RenderPipelineAsset::forward_3d());
    renderer.register_material::<StandardMaterial>(&ctx);

    let vertices = [
        Vertex {
            position: [-1.0, -1.0, 0.0],
            normal: [0.0, 0.0, 1.0],
            uv: [0.0, 1.0],
        },
        Vertex {
            position: [1.0, -1.0, 0.0],
            normal: [0.0, 0.0, 1.0],
            uv: [1.0, 1.0],
        },
        Vertex {
            position: [1.0, 1.0, 0.0],
            normal: [0.0, 0.0, 1.0],
            uv: [1.0, 0.0],
        },
        Vertex {
            position: [-1.0, 1.0, 0.0],
            normal: [0.0, 0.0, 1.0],
            uv: [0.0, 0.0],
        },
    ];
    let indices = [0u16, 1, 2, 0, 2, 3];
    let mesh_handle = renderer.insert_mesh(Mesh::from_raw(
        &ctx,
        MeshDescriptor::new(
            bytemuck::cast_slice(&vertices),
            vertices.len() as u32,
            Mesh::vertex_layout_position_normal_uv(),
            "shadow_lod_stats_quad",
        )
        .with_indices(MeshIndexData::U16(&indices)),
    ));
    let material = renderer
        .materials_mut::<StandardMaterial>()
        .insert(StandardMaterial::default());

    let mut world = World::new();
    world.spawn((
        Transform::default(),
        CameraMarker::new(),
        Projection::perspective(60.0f32.to_radians(), 0.1, 32.0),
        MainCamera,
    ));
    world.spawn((
        Transform::from_xyz(0.0, 0.0, -3.0),
        WgpuMeshRenderer::new(mesh_handle, material).shadow_lod_cascades(1),
    ));
    world.spawn((DirectionalLight::new([0.3, -1.0, 0.2])
        .cascade_count(2)
        .cascade_distances([8.0, 32.0, 0.0, 0.0])
        .shadow_map_size(64),));

    ctx.begin_frame()
        .expect("headless begin_frame should succeed");
    renderer.render_world(&mut ctx, &world);
    ctx.end_frame();

    let stats = renderer.stats();
    assert_eq!(stats.shadow_cascade_count, 2);
    assert_eq!(stats.shadow_caster_count_by_cascade[0], 1);
    assert_eq!(stats.shadow_caster_count_by_cascade[1], 0);
    assert_eq!(stats.shadow_caster_count, 1);
    assert_eq!(stats.shadow_draw_calls_by_cascade[0], 1);
    assert_eq!(stats.shadow_draw_calls_by_cascade[1], 0);
}

const TEST_HOLOGRAM_SHADER: &str = r#"
struct ViewUniform {
    view_proj: mat4x4<f32>,
    camera: vec4<f32>,
    viewport: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> camera: ViewUniform;

struct HologramUniform {
    tint: vec4<f32>,
    params: vec4<f32>,
};

@group(1) @binding(0)
var<uniform> material: HologramUniform;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(8) model_col0: vec4<f32>,
    @location(9) model_col1: vec4<f32>,
    @location(10) model_col2: vec4<f32>,
    @location(11) model_col3: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) uv: vec2<f32>,
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    let model = mat4x4<f32>(
        input.model_col0,
        input.model_col1,
        input.model_col2,
        input.model_col3,
    );
    let world_position = model * vec4<f32>(input.position, 1.0);
    output.clip_position = camera.view_proj * world_position;
    output.world_position = world_position.xyz;
    output.uv = input.uv;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let scan = 0.55 + 0.45 * sin(input.world_position.y * material.params.y + input.uv.x * 12.0);
    let edge = pow(1.0 - abs(input.uv.y * 2.0 - 1.0), 2.0);
    let glow = material.params.x * (0.35 + scan * 0.65 + edge * 0.8);
    let alpha = material.tint.a * (0.25 + scan * 0.55 + edge * 0.2);
    return vec4<f32>(material.tint.rgb * glow, alpha);
}
"#;

const TEST_SCENE_PREPASS_SHADER: &str = r#"
struct ViewUniform {
    view_proj: mat4x4<f32>,
    camera: vec4<f32>,
    viewport: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> camera: ViewUniform;

struct MaterialUniform {
    color: vec4<f32>,
};

@group(1) @binding(0)
var<uniform> material: MaterialUniform;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(8) model_col0: vec4<f32>,
    @location(9) model_col1: vec4<f32>,
    @location(10) model_col2: vec4<f32>,
    @location(11) model_col3: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    let model = mat4x4<f32>(
        input.model_col0,
        input.model_col1,
        input.model_col2,
        input.model_col3,
    );
    output.clip_position = camera.view_proj * (model * vec4<f32>(input.position, 1.0));
    output.uv = input.uv;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(material.color.rgb * vec3<f32>(0.6 + input.uv.x * 0.4), material.color.a);
}
"#;

const TEST_SCENE_PREPASS_GBUFFER_SHADER: &str = r#"
struct ViewUniform {
    view_proj: mat4x4<f32>,
    camera: vec4<f32>,
    viewport: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> camera: ViewUniform;

struct MaterialUniform {
    color: vec4<f32>,
};

@group(1) @binding(0)
var<uniform> material: MaterialUniform;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(8) model_col0: vec4<f32>,
    @location(9) model_col1: vec4<f32>,
    @location(10) model_col2: vec4<f32>,
    @location(11) model_col3: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

struct FragmentOutput {
    @location(0) albedo: vec4<f32>,
    @location(1) material: vec4<f32>,
    @location(2) emissive: vec4<f32>,
    @location(3) encoded_normal: vec4<f32>,
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    let model = mat4x4<f32>(
        input.model_col0,
        input.model_col1,
        input.model_col2,
        input.model_col3,
    );
    output.clip_position = camera.view_proj * (model * vec4<f32>(input.position, 1.0));
    output.uv = input.uv;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> FragmentOutput {
    let tint = vec3<f32>(input.uv.x, input.uv.y, 1.0 - input.uv.x * 0.5);
    var output: FragmentOutput;
    output.albedo = vec4<f32>(material.color.rgb * tint, material.color.a);
    output.material = vec4<f32>(0.15, 0.75, 0.25, material.color.a);
    output.emissive = vec4<f32>(material.color.rgb * 0.05, material.color.a);
    output.encoded_normal = vec4<f32>(0.5, 0.5, 1.0, 1.0);
    return output;
}
"#;

#[derive(Clone)]
struct TestHologramMaterial {
    tint: Color,
    intensity: f32,
    stripe_scale: f32,
}

impl crate::render::Material for TestHologramMaterial {
    type Data = TestHologramMaterial;

    fn interface() -> crate::render::resources::material::MaterialInterface {
        crate::render::resources::material::MaterialInterface::builder("test_hologram")
            .shader(crate::render::resources::material::MaterialShaderSet::wgsl(
                TEST_HOLOGRAM_SHADER,
            ))
            .vertex(crate::render::expert::Mesh::vertex_layout_position_uv())
            .binding(
                crate::render::resources::material::MaterialBinding::uniform(
                    0,
                    std::num::NonZeroU64::new(32).expect("hologram uniform has non-zero size"),
                ),
            )
            .main_pass(crate::render::resources::material::MainPassMode::Transparent)
            .render_state(crate::render::MaterialRenderState::transparent())
            .build()
    }

    fn prepare(
        data: &Self::Data,
        ctx: &mut crate::render::resources::material::MaterialPrepareContext<'_>,
    ) -> Result<crate::render::resources::material::PreparedMaterial, MaterialError> {
        #[repr(C)]
        #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
        struct HologramUniform {
            tint: [f32; 4],
            params: [f32; 4],
        }

        let uniform = HologramUniform {
            tint: data.tint.to_array(),
            params: [data.intensity, data.stripe_scale, 0.0, 0.0],
        };
        ctx.bindings()
            .uniform(0, "test_hologram_material_uniform", &uniform)
            .build()
    }

    fn render_state(_data: &Self::Data) -> crate::render::MaterialRenderState {
        crate::render::MaterialRenderState::transparent()
    }
}

#[derive(Clone)]
struct TestScenePrepassMaterial {
    color: Color,
}

impl crate::render::Material for TestScenePrepassMaterial {
    type Data = TestScenePrepassMaterial;

    fn interface() -> crate::render::resources::material::MaterialInterface {
        crate::render::resources::material::MaterialInterface::builder("test_scene_prepass")
            .shader(crate::render::resources::material::MaterialShaderSet::wgsl(
                TEST_SCENE_PREPASS_SHADER,
            ))
            .vertex(crate::render::expert::Mesh::vertex_layout_position_uv())
            .binding(
                crate::render::resources::material::MaterialBinding::uniform(
                    0,
                    std::num::NonZeroU64::new(16)
                        .expect("scene prepass material uniform has non-zero size"),
                ),
            )
            .render_state(crate::render::MaterialRenderState::opaque())
            .passes(crate::render::resources::material::MaterialPassSet {
                main: crate::render::resources::material::MainPassMode::Opaque,
                prepass: Some(
                    crate::render::resources::material::MaterialPrepassMode::SceneMaterial,
                ),
                shadow: crate::render::resources::material::ShadowPassMode::None,
            })
            .build()
    }

    fn prepare(
        data: &Self::Data,
        ctx: &mut crate::render::resources::material::MaterialPrepareContext<'_>,
    ) -> Result<crate::render::resources::material::PreparedMaterial, MaterialError> {
        #[repr(C)]
        #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
        struct MaterialUniform {
            color: [f32; 4],
        }

        ctx.bindings()
            .uniform(
                0,
                "test_scene_prepass_material_uniform",
                &MaterialUniform {
                    color: data.color.to_array(),
                },
            )
            .build()
    }

    fn render_state(_data: &Self::Data) -> crate::render::MaterialRenderState {
        crate::render::MaterialRenderState::opaque()
    }

    fn scene_prepass_shader_source(_data: &Self::Data) -> Option<crate::render::ShaderSource> {
        Some(crate::render::ShaderSource::wgsl(
            TEST_SCENE_PREPASS_GBUFFER_SHADER,
        ))
    }

    fn scene_prepass_vertex_layout(_data: &Self::Data) -> crate::render::expert::VertexLayout {
        crate::render::expert::Mesh::vertex_layout_position_uv()
    }
}

#[test]
fn custom_material_registration_renders_mesh_without_engine_changes() {
    #[repr(C)]
    #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    struct Vertex {
        position: [f32; 3],
        uv: [f32; 2],
    }

    let (device, queue) = create_test_device();
    let mut ctx =
        GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [96, 96]);
    let pipeline = RenderPipelineBuilder::new()
        .register_material::<TestHologramMaterial>()
        .add_phase(crate::render::TransparentPhase::new())
        .build();
    let mut renderer = RenderComposer::from_asset(pipeline);
    renderer.register_material::<TestHologramMaterial>(&ctx);

    let vertices = [
        Vertex {
            position: [-0.9, -1.1, 0.3],
            uv: [0.0, 1.0],
        },
        Vertex {
            position: [1.1, -0.7, -0.2],
            uv: [1.0, 1.0],
        },
        Vertex {
            position: [0.7, 1.0, 0.1],
            uv: [1.0, 0.0],
        },
        Vertex {
            position: [-1.0, 0.6, -0.3],
            uv: [0.0, 0.0],
        },
    ];
    let indices = [0u16, 1, 2, 0, 2, 3];
    let mesh_handle = renderer.insert_mesh(Mesh::from_raw(
        &ctx,
        MeshDescriptor::new(
            bytemuck::cast_slice(&vertices),
            vertices.len() as u32,
            Mesh::vertex_layout_position_uv(),
            "custom_material_mesh",
        )
        .with_indices(MeshIndexData::U16(&indices)),
    ));
    let material_handle =
        renderer
            .materials_mut::<TestHologramMaterial>()
            .insert(TestHologramMaterial {
                tint: Color::new(0.2, 0.9, 1.0, 0.72),
                intensity: 1.35,
                stripe_scale: 14.0,
            });

    let mut world = World::new();
    world.spawn((
        Transform::from_xyz(0.0, 0.0, 6.0),
        CameraMarker::new(),
        Projection::perspective(55.0f32.to_radians(), 0.1, 32.0),
        MainCamera,
    ));
    world.spawn((
        Transform::from_xyz(0.0, 0.0, 0.0).with_euler_angles(0.35, 0.0, 0.2),
        WgpuMeshRenderer::new(mesh_handle, material_handle),
    ));

    ctx.begin_frame()
        .expect("headless begin_frame should succeed");
    renderer.render_world(&mut ctx, &world);
    ctx.end_frame();

    let stats = renderer.stats();
    assert_eq!(stats.view_count, 1);
    assert_eq!(stats.draw_calls, 1);
    assert!(stats.passes >= 1);
}

#[test]
fn custom_material_scene_prepass_runs_in_opaque_3d_pipeline() {
    #[repr(C)]
    #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    struct Vertex {
        position: [f32; 3],
        uv: [f32; 2],
    }

    let (device, queue) = create_test_device();
    let mut ctx =
        GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [96, 96]);
    let pipeline = RenderPipelineBuilder::new()
        .register_material::<TestScenePrepassMaterial>()
        .add_phase(crate::render::SceneNormalPrepass::default())
        .add_phase(crate::render::SceneMaterialPrepass::default())
        .add_phase(crate::render::expert::OpaquePhase::new())
        .build();
    let mut renderer = RenderComposer::from_asset(pipeline);
    renderer.register_material::<TestScenePrepassMaterial>(&ctx);

    let vertices = [
        Vertex {
            position: [-0.8, -0.8, 0.0],
            uv: [0.0, 1.0],
        },
        Vertex {
            position: [0.8, -0.8, 0.0],
            uv: [1.0, 1.0],
        },
        Vertex {
            position: [0.8, 0.8, 0.0],
            uv: [1.0, 0.0],
        },
        Vertex {
            position: [-0.8, 0.8, 0.0],
            uv: [0.0, 0.0],
        },
    ];
    let indices = [0u16, 1, 2, 0, 2, 3];
    let mesh_handle = renderer.insert_mesh(Mesh::from_raw(
        &ctx,
        MeshDescriptor::new(
            bytemuck::cast_slice(&vertices),
            vertices.len() as u32,
            Mesh::vertex_layout_position_uv(),
            "custom_scene_prepass_mesh",
        )
        .with_indices(MeshIndexData::U16(&indices)),
    ));
    let material_handle =
        renderer
            .materials_mut::<TestScenePrepassMaterial>()
            .insert(TestScenePrepassMaterial {
                color: Color::new(0.9, 0.4, 0.2, 1.0),
            });

    let mut world = World::new();
    world.spawn((
        Transform::from_xyz(0.0, 0.0, 4.0),
        CameraMarker::new(),
        Projection::perspective(55.0f32.to_radians(), 0.1, 32.0),
        MainCamera,
    ));
    world.spawn((
        Transform::default(),
        WgpuMeshRenderer::new(mesh_handle, material_handle),
    ));

    ctx.begin_frame()
        .expect("headless begin_frame should succeed");
    renderer.render_world(&mut ctx, &world);
    ctx.end_frame();

    let stats = renderer.stats();
    assert_eq!(stats.view_count, 1);
    assert!(stats.passes >= 3);
    assert_eq!(stats.draw_calls, 1);
}

#[test]
fn forward_3d_ddgi_executes_with_standard_material_geometry() {
    #[repr(C)]
    #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    struct Vertex {
        position: [f32; 3],
        normal: [f32; 3],
        uv: [f32; 2],
    }

    let (device, queue) = create_test_device();
    let mut ctx =
        GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [96, 96]);
    let mut renderer = RenderComposer::from_asset(RenderPipelineAsset::forward_3d());
    renderer.register_material::<StandardMaterial>(&ctx);

    let vertices = [
        Vertex {
            position: [-0.9, -0.9, 0.0],
            normal: [0.0, 0.0, 1.0],
            uv: [0.0, 1.0],
        },
        Vertex {
            position: [0.9, -0.9, 0.0],
            normal: [0.0, 0.0, 1.0],
            uv: [1.0, 1.0],
        },
        Vertex {
            position: [0.9, 0.9, 0.0],
            normal: [0.0, 0.0, 1.0],
            uv: [1.0, 0.0],
        },
        Vertex {
            position: [-0.9, 0.9, 0.0],
            normal: [0.0, 0.0, 1.0],
            uv: [0.0, 0.0],
        },
    ];
    let indices = [0u16, 1, 2, 0, 2, 3];
    let mesh_handle = renderer.insert_mesh(Mesh::from_raw(
        &ctx,
        MeshDescriptor::new(
            bytemuck::cast_slice(&vertices),
            vertices.len() as u32,
            Mesh::vertex_layout_position_normal_uv(),
            "forward_3d_global_illumination_mesh",
        )
        .with_indices(MeshIndexData::U16(&indices)),
    ));
    let material_handle = renderer
        .materials_mut::<StandardMaterial>()
        .insert(StandardMaterial {
            albedo: Color::new(0.82, 0.48, 0.26, 1.0),
            emissive: Color::new(0.08, 0.03, 0.01, 1.0),
            ..StandardMaterial::default()
        });

    let mut world = World::new();
    world.insert_resource(RenderSettings {
        global_illumination: crate::render::gi::providers::ddgi::global_illumination(
            crate::render::gi::providers::ddgi::DdgiSettings {
                volume: crate::render::gi::providers::ddgi::DdgiVolumeSettings {
                    origin: [-6.0, -4.0, -6.0],
                    spacing: 3.0,
                    counts: [6, 4, 6],
                    scroll_with_main_camera: false,
                },
                rays_per_probe: 8,
                probes_per_frame: 8,
                irradiance_resolution: 4,
                visibility_resolution: 4,
                max_ray_distance: 16.0,
                ..Default::default()
            },
        ),
        bloom: crate::render::BloomSettings {
            enabled: false,
            ..Default::default()
        },
        tonemap: crate::render::ToneMapSettings {
            enabled: false,
            ..Default::default()
        },
        temporal_aa: crate::render::TemporalAntiAliasingSettings {
            enabled: true,
            ..Default::default()
        },
        vignette: crate::render::VignetteSettings {
            enabled: false,
            ..Default::default()
        },
        ..RenderSettings::default()
    });
    world.spawn((
        Transform::from_xyz(0.0, 0.0, 4.0),
        CameraMarker::new(),
        Projection::perspective(55.0f32.to_radians(), 0.1, 32.0),
        MainCamera,
    ));
    world.spawn((
        Transform::default(),
        WgpuMeshRenderer::new(mesh_handle, material_handle),
    ));
    world.spawn((
        Transform::from_xyz(0.0, 0.0, 2.0),
        DirectionalLight::new([0.0, 0.0, -1.0])
            .intensity(0.9)
            .color(Color::new(1.0, 0.96, 0.9, 1.0)),
    ));

    ctx.begin_frame()
        .expect("headless begin_frame should succeed");
    renderer.render_world(&mut ctx, &world);
    ctx.end_frame();

    let stats = renderer.stats();
    assert_eq!(stats.view_count, 2);
    assert!(stats.passes >= 5);
    assert!(stats.draw_calls >= 1);
}

#[test]
fn modern_3d_ssgi_executes_with_standard_material_geometry() {
    #[repr(C)]
    #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    struct Vertex {
        position: [f32; 3],
        normal: [f32; 3],
        uv: [f32; 2],
    }

    let (device, queue) = create_test_device();
    let mut ctx =
        GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [96, 96]);
    let mut renderer = RenderComposer::from_asset(RenderPipelineAsset::modern_3d());
    renderer.register_material::<StandardMaterial>(&ctx);

    let vertices = [
        Vertex {
            position: [-0.9, -0.9, 0.0],
            normal: [0.0, 0.0, 1.0],
            uv: [0.0, 1.0],
        },
        Vertex {
            position: [0.9, -0.9, 0.0],
            normal: [0.0, 0.0, 1.0],
            uv: [1.0, 1.0],
        },
        Vertex {
            position: [0.9, 0.9, 0.0],
            normal: [0.0, 0.0, 1.0],
            uv: [1.0, 0.0],
        },
        Vertex {
            position: [-0.9, 0.9, 0.0],
            normal: [0.0, 0.0, 1.0],
            uv: [0.0, 0.0],
        },
    ];
    let indices = [0u16, 1, 2, 0, 2, 3];
    let mesh_handle = renderer.insert_mesh(Mesh::from_raw(
        &ctx,
        MeshDescriptor::new(
            bytemuck::cast_slice(&vertices),
            vertices.len() as u32,
            Mesh::vertex_layout_position_normal_uv(),
            "modern_3d_ssgi_mesh",
        )
        .with_indices(MeshIndexData::U16(&indices)),
    ));
    let material_handle = renderer
        .materials_mut::<StandardMaterial>()
        .insert(StandardMaterial {
            albedo: Color::new(0.72, 0.56, 0.42, 1.0),
            roughness: 0.92,
            emissive: Color::new(0.12, 0.08, 0.04, 1.0),
            ..StandardMaterial::default()
        });

    let mut world = World::new();
    world.insert_resource(RenderSettings {
        global_illumination: crate::render::gi::providers::ssgi::global_illumination(
            crate::render::gi::providers::ssgi::SsgiSettings {
                intensity: 1.0,
                radius_pixels: 8.0,
                depth_rejection: 8.0,
                normal_power: 64.0,
            },
        ),
        bloom: crate::render::BloomSettings {
            enabled: false,
            ..Default::default()
        },
        tonemap: crate::render::ToneMapSettings {
            enabled: false,
            ..Default::default()
        },
        vignette: crate::render::VignetteSettings {
            enabled: false,
            ..Default::default()
        },
        ..RenderSettings::default()
    });
    world.spawn((
        Transform::from_xyz(0.0, 0.0, 4.0),
        CameraMarker::new(),
        Projection::perspective(55.0f32.to_radians(), 0.1, 32.0),
        MainCamera,
    ));
    world.spawn((
        Transform::default(),
        WgpuMeshRenderer::new(mesh_handle, material_handle),
    ));

    ctx.begin_frame()
        .expect("headless begin_frame should succeed");
    renderer.render_world(&mut ctx, &world);
    ctx.end_frame();

    let stats = renderer.stats();
    assert_eq!(stats.view_count, 1);
    assert!(stats.passes >= 5);
    assert!(stats.draw_calls >= 2);
}

#[derive(Clone)]
struct CountingComputePass {
    setup_calls: Arc<AtomicUsize>,
    execute_calls: Arc<AtomicUsize>,
}

impl ComputePass for CountingComputePass {
    fn name(&self) -> &'static str {
        "counting_compute"
    }

    fn setup(&mut self, ctx: &mut ComputePassSetupContext<'_, '_>) {
        self.setup_calls.fetch_add(1, Ordering::Relaxed);
        let current = ctx
            .state()
            .current_color()
            .expect("scene color should exist before custom compute");
        ctx.graph().add_compute_pass(self.name(), |setup| {
            setup.readwrite(current.handle());
        });
    }

    fn execute(
        &mut self,
        ctx: &mut ComputePassExecuteContext<'_, '_>,
    ) -> Result<(), RenderGraphError> {
        self.execute_calls.fetch_add(1, Ordering::Relaxed);
        assert!(ctx.scene_view().is_some());
        Ok(())
    }
}

#[derive(Clone)]
struct CountingPostFxPass {
    setup_calls: Arc<AtomicUsize>,
    execute_calls: Arc<AtomicUsize>,
}

impl PostFxPass for CountingPostFxPass {
    fn name(&self) -> &'static str {
        "counting_postfx"
    }

    fn setup(&mut self, ctx: &mut PostFxPassSetupContext<'_, '_>) {
        self.setup_calls.fetch_add(1, Ordering::Relaxed);
        let current = ctx
            .state()
            .current_color()
            .expect("scene color should exist before custom postfx");
        ctx.graph().add_render_pass(self.name(), |setup| {
            setup.read(current.handle());
            setup.write_color_loaded(0, current.handle());
        });
    }

    fn execute(
        &mut self,
        ctx: &mut PostFxPassExecuteContext<'_, '_>,
    ) -> Result<(), RenderGraphError> {
        self.execute_calls.fetch_add(1, Ordering::Relaxed);
        assert!(ctx.scene_view().is_some());
        Ok(())
    }
}

#[derive(Clone)]
struct CountingFinalizePass {
    setup_calls: Arc<AtomicUsize>,
    execute_calls: Arc<AtomicUsize>,
}

impl RenderPass for CountingFinalizePass {
    fn name(&self) -> &'static str {
        "counting_finalize"
    }

    fn setup(&mut self, ctx: &mut RenderPassSetupContext<'_, '_, '_>) {
        self.setup_calls.fetch_add(1, Ordering::Relaxed);
        let first_view = ctx
            .state()
            .completed_views()
            .first()
            .expect("finalize pass should see at least one prepared view");
        let input = first_view
            .slots()
            .current_color()
            .expect("view should expose a current color slot");
        let size = first_view.target_size();
        let sink = ctx.graph().create_texture(|builder| {
            builder
                .name("counting_finalize_sink")
                .size(TargetSize::Exact(size[0], size[1]))
                .format(input.format())
                .persistent();
        });
        ctx.graph().add_render_pass(self.name(), |setup| {
            setup.read(input.handle());
            setup.write_color(0, sink);
        });
        let _ = ctx.state().set_current_color(sink, input.format());
    }

    fn execute(
        &mut self,
        ctx: &mut RenderPassExecuteContext<'_, '_>,
    ) -> Result<(), RenderGraphError> {
        self.execute_calls.fetch_add(1, Ordering::Relaxed);
        assert_eq!(ctx.completed_views().len(), 1);
        assert_eq!(ctx.pass().name.as_ref(), self.name());
        Ok(())
    }
}

#[derive(Clone)]
struct CountingPhase {
    setup_calls: Arc<AtomicUsize>,
    execute_calls: Arc<AtomicUsize>,
}

impl RenderPhase for CountingPhase {
    fn name(&self) -> &'static str {
        "counting_phase"
    }

    fn setup(&mut self, ctx: &mut RenderPhaseSetupContext<'_, '_>) {
        self.setup_calls.fetch_add(1, Ordering::Relaxed);
        let current = ctx
            .state()
            .current_color()
            .expect("scene color should exist before custom phase");
        ctx.graph().add_render_pass(self.name(), |setup| {
            setup.write_color_loaded(0, current.handle());
        });
    }

    fn execute(
        &mut self,
        ctx: &mut RenderPhaseExecuteContext<'_, '_, '_>,
    ) -> Result<(), RenderGraphError> {
        self.execute_calls.fetch_add(1, Ordering::Relaxed);
        assert!(ctx.scene_view().is_some());

        let (
            gpu,
            pass,
            resources,
            _execution,
            _draw_functions,
            _material_registry,
            _mesh_registry,
            _fallback,
        ) = ctx.split();
        let output_handle =
            crate::render::execution::pass_first_write_texture(pass, self.name(), "output");
        let output = crate::render::execution::require_render_target(
            resources,
            output_handle,
            self.name(),
            "output",
        );
        let color_attachments = [Some(wgpu::RenderPassColorAttachment {
            view: output.view(),
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Load,
                store: wgpu::StoreOp::Store,
            },
        })];
        let mut frame = gpu.frame();
        let _render_pass = frame.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(self.name()),
            color_attachments: &color_attachments,
            depth_stencil_attachment: None,
            ..Default::default()
        });
        Ok(())
    }
}

#[derive(Clone)]
struct ViewFilteredPhase {
    enabled_order: i32,
    setup_calls: Arc<AtomicUsize>,
    execute_calls: Arc<AtomicUsize>,
}

impl RenderPhase for ViewFilteredPhase {
    fn name(&self) -> &'static str {
        "view_filtered_phase"
    }

    fn is_enabled(&self, _frame: &PreparedFrame<'_>, view: &PreparedView<'_>) -> bool {
        view.order() == self.enabled_order
    }

    fn setup(&mut self, ctx: &mut RenderPhaseSetupContext<'_, '_>) {
        assert_eq!(ctx.view().order(), self.enabled_order);
        self.setup_calls.fetch_add(1, Ordering::Relaxed);
        let current = ctx
            .state()
            .current_color()
            .expect("scene color should exist before filtered phase");
        ctx.graph().add_render_pass(self.name(), |setup| {
            setup.write_color_loaded(0, current.handle());
        });
    }

    fn execute(
        &mut self,
        ctx: &mut RenderPhaseExecuteContext<'_, '_, '_>,
    ) -> Result<(), RenderGraphError> {
        assert_eq!(ctx.view().order(), self.enabled_order);
        self.execute_calls.fetch_add(1, Ordering::Relaxed);
        let (gpu, pass, resources, _execution, _, _, _, _) = ctx.split();
        let output_handle =
            crate::render::execution::pass_first_write_texture(pass, self.name(), "output");
        let output = crate::render::execution::require_render_target(
            resources,
            output_handle,
            self.name(),
            "output",
        );
        let color_attachments = [Some(wgpu::RenderPassColorAttachment {
            view: output.view(),
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Load,
                store: wgpu::StoreOp::Store,
            },
        })];
        let mut frame = gpu.frame();
        let _render_pass = frame.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(self.name()),
            color_attachments: &color_attachments,
            depth_stencil_attachment: None,
            ..Default::default()
        });
        Ok(())
    }
}

#[derive(Clone)]
struct ViewFilteredPostFxPass {
    enabled_order: i32,
    setup_calls: Arc<AtomicUsize>,
    execute_calls: Arc<AtomicUsize>,
}

impl PostFxPass for ViewFilteredPostFxPass {
    fn name(&self) -> &'static str {
        "view_filtered_postfx"
    }

    fn is_enabled(&self, _frame: &PreparedFrame<'_>, view: &PreparedView<'_>) -> bool {
        view.order() == self.enabled_order
    }

    fn setup(&mut self, ctx: &mut PostFxPassSetupContext<'_, '_>) {
        assert_eq!(ctx.view().order(), self.enabled_order);
        self.setup_calls.fetch_add(1, Ordering::Relaxed);
        let current = ctx
            .state()
            .current_color()
            .expect("scene color should exist before filtered postfx");
        ctx.graph().add_render_pass(self.name(), |setup| {
            setup.read(current.handle());
            setup.write_color_loaded(0, current.handle());
        });
    }

    fn execute(
        &mut self,
        ctx: &mut PostFxPassExecuteContext<'_, '_>,
    ) -> Result<(), RenderGraphError> {
        assert_eq!(ctx.view().order(), self.enabled_order);
        self.execute_calls.fetch_add(1, Ordering::Relaxed);
        let (gpu, pass, resources, _execution) = ctx.split();
        let output_handle =
            crate::render::execution::pass_first_write_texture(pass, self.name(), "output");
        let output = crate::render::execution::require_render_target(
            resources,
            output_handle,
            self.name(),
            "output",
        );
        let color_attachments = [Some(wgpu::RenderPassColorAttachment {
            view: output.view(),
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Load,
                store: wgpu::StoreOp::Store,
            },
        })];
        let mut frame = gpu.frame();
        let _render_pass = frame.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(self.name()),
            color_attachments: &color_attachments,
            depth_stencil_attachment: None,
            ..Default::default()
        });
        Ok(())
    }
}

#[test]
fn custom_pipeline_steps_receive_setup_and_execute_contexts() {
    let (device, queue) = create_test_device();
    let mut ctx =
        GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [64, 64]);

    let compute_setup = Arc::new(AtomicUsize::new(0));
    let compute_execute = Arc::new(AtomicUsize::new(0));
    let postfx_setup = Arc::new(AtomicUsize::new(0));
    let postfx_execute = Arc::new(AtomicUsize::new(0));
    let finalize_setup = Arc::new(AtomicUsize::new(0));
    let finalize_execute = Arc::new(AtomicUsize::new(0));

    let pipeline = RenderPipelineBuilder::new()
        .add_compute(CountingComputePass {
            setup_calls: compute_setup.clone(),
            execute_calls: compute_execute.clone(),
        })
        .add_postfx(CountingPostFxPass {
            setup_calls: postfx_setup.clone(),
            execute_calls: postfx_execute.clone(),
        })
        .add_pass(CountingFinalizePass {
            setup_calls: finalize_setup.clone(),
            execute_calls: finalize_execute.clone(),
        })
        .build();
    let mut renderer = RenderComposer::from_asset(pipeline);
    let world = World::new();

    ctx.begin_frame()
        .expect("headless begin_frame should succeed");
    renderer.render_world(&mut ctx, &world);
    ctx.end_frame();

    assert_eq!(compute_setup.load(Ordering::Relaxed), 1);
    assert_eq!(compute_execute.load(Ordering::Relaxed), 1);
    assert_eq!(postfx_setup.load(Ordering::Relaxed), 1);
    assert_eq!(postfx_execute.load(Ordering::Relaxed), 1);
    assert_eq!(finalize_setup.load(Ordering::Relaxed), 1);
    assert_eq!(finalize_execute.load(Ordering::Relaxed), 1);
}

#[test]
fn custom_phase_steps_receive_setup_and_execute_contexts() {
    let (device, queue) = create_test_device();
    let mut ctx =
        GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [64, 64]);

    let phase_setup = Arc::new(AtomicUsize::new(0));
    let phase_execute = Arc::new(AtomicUsize::new(0));

    let pipeline = RenderPipelineBuilder::new()
        .add_phase(CountingPhase {
            setup_calls: phase_setup.clone(),
            execute_calls: phase_execute.clone(),
        })
        .build();
    let mut renderer = RenderComposer::from_asset(pipeline);
    let world = World::new();

    ctx.begin_frame()
        .expect("headless begin_frame should succeed");
    renderer.render_world(&mut ctx, &world);
    ctx.end_frame();

    assert_eq!(phase_setup.load(Ordering::Relaxed), 1);
    assert_eq!(phase_execute.load(Ordering::Relaxed), 1);
}

#[test]
fn view_specific_phase_and_postfx_skip_disabled_views() {
    let (device, queue) = create_test_device();
    let mut ctx =
        GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [64, 64]);

    let phase_setup = Arc::new(AtomicUsize::new(0));
    let phase_execute = Arc::new(AtomicUsize::new(0));
    let postfx_setup = Arc::new(AtomicUsize::new(0));
    let postfx_execute = Arc::new(AtomicUsize::new(0));

    let pipeline = RenderPipelineBuilder::new()
        .add_phase(ViewFilteredPhase {
            enabled_order: 1,
            setup_calls: phase_setup.clone(),
            execute_calls: phase_execute.clone(),
        })
        .add_postfx(ViewFilteredPostFxPass {
            enabled_order: 1,
            setup_calls: postfx_setup.clone(),
            execute_calls: postfx_execute.clone(),
        })
        .build();
    let mut renderer = RenderComposer::from_asset(pipeline);

    let mut world = World::new();
    world.spawn((
        Transform::default(),
        CameraMarker::new(),
        Projection::orthographic_fixed(32.0, 32.0),
        CameraViewport::new(ViewportRect::new(0, 0, 32, 32)).order(10),
    ));
    world.spawn((
        Transform::default(),
        CameraMarker::new(),
        Projection::orthographic_fixed(32.0, 32.0),
        CameraViewport::new(ViewportRect::new(32, 0, 32, 32)).order(20),
    ));

    ctx.begin_frame()
        .expect("headless begin_frame should succeed");
    renderer.render_world(&mut ctx, &world);
    ctx.end_frame();

    assert_eq!(renderer.stats().view_count, 2);
    assert_eq!(phase_setup.load(Ordering::Relaxed), 1);
    assert_eq!(phase_execute.load(Ordering::Relaxed), 1);
    assert_eq!(postfx_setup.load(Ordering::Relaxed), 1);
    assert_eq!(postfx_execute.load(Ordering::Relaxed), 1);
}

#[test]
fn register_material_automatically_wires_mesh_draw_and_extract() {
    let (device, queue) = create_test_device();
    let mut ctx =
        GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [64, 64]);

    let pipeline = RenderPipelineBuilder::new()
        .register_material::<UnlitMaterial>()
        .add_phase(crate::render::expert::OpaquePhase::new())
        .build();
    let mut renderer = RenderComposer::from_asset(pipeline);
    renderer.register_material::<UnlitMaterial>(&ctx);

    let mesh_handle = renderer.insert_mesh(crate::render::expert::Mesh::builtin_quad(&ctx));
    let material_handle = renderer
        .materials_mut::<UnlitMaterial>()
        .insert(UnlitMaterial::default().color(Color::new(0.3, 0.8, 0.4, 1.0)));

    let mut world = World::new();
    world.spawn((
        Transform::default(),
        CameraMarker::new(),
        Projection::orthographic_fixed(64.0, 64.0),
        MainCamera,
    ));
    world.spawn((
        Transform::default(),
        WgpuMeshRenderer::new(mesh_handle, material_handle),
    ));

    assert_eq!(renderer.plan.extractors.len(), 1);

    ctx.begin_frame()
        .expect("headless begin_frame should succeed");
    renderer.render_world(&mut ctx, &world);
    ctx.end_frame();

    assert_eq!(renderer.stats().view_count, 1);
    assert!(renderer.stats().passes >= 2);
}

#[test]
fn transparent_sprite_phase_reports_single_draw_call_for_same_texture_batch() {
    let (device, queue) = create_test_device();
    let mut ctx =
        GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [64, 64]);
    let mut renderer = RenderComposer::from_asset(
        RenderPipelineAsset::builder()
            .add_feature(crate::render::SpriteFeature::unlit())
            .add_phase(crate::render::TransparentPhase::new())
            .build(),
    );

    let asset_server = AssetServer::with_empty_manifest(AssetConfig::default());
    let white = asset_server.insert_runtime(TextureAsset::white_pixel());

    let mut world = World::new();
    world.insert_resource(asset_server);
    world.spawn((
        Transform::default(),
        CameraMarker::new(),
        Projection::orthographic_fixed(64.0, 64.0),
        MainCamera,
    ));
    world.spawn((
        Transform::from_xyz(-8.0, 0.0, 0.0),
        crate::render::SpriteRenderer::new(16.0, 16.0)
            .color(Color::RED)
            .texture(white.clone()),
    ));
    world.spawn((
        Transform::from_xyz(8.0, 0.0, 0.0),
        crate::render::SpriteRenderer::new(16.0, 16.0)
            .color(Color::GREEN)
            .texture(white.clone()),
    ));

    ctx.begin_frame()
        .expect("headless begin_frame should succeed");
    renderer.render_world(&mut ctx, &world);
    ctx.end_frame();

    assert_eq!(renderer.stats().draw_calls, 1);
}

#[test]
fn opaque_mesh_phase_batches_same_mesh_and_material_instances() {
    let (device, queue) = create_test_device();
    let mut ctx =
        GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [64, 64]);

    let pipeline = RenderPipelineBuilder::new()
        .register_material::<UnlitMaterial>()
        .add_phase(crate::render::expert::OpaquePhase::new())
        .build();
    let mut renderer = RenderComposer::from_asset(pipeline);
    renderer.register_material::<UnlitMaterial>(&ctx);

    let mesh_handle = renderer.insert_mesh(crate::render::expert::Mesh::builtin_quad(&ctx));
    let material_handle = renderer
        .materials_mut::<UnlitMaterial>()
        .insert(UnlitMaterial::default().color(Color::new(0.3, 0.8, 0.4, 1.0)));

    let mut world = World::new();
    world.spawn((
        Transform::default(),
        CameraMarker::new(),
        Projection::orthographic_fixed(64.0, 64.0),
        MainCamera,
    ));
    world.spawn((
        Transform::from_xyz(-8.0, 0.0, 0.2).with_scale(16.0, 16.0),
        WgpuMeshRenderer::new(mesh_handle, material_handle),
    ));
    world.spawn((
        Transform::from_xyz(8.0, 0.0, 0.2).with_scale(16.0, 16.0),
        WgpuMeshRenderer::new(mesh_handle, material_handle),
    ));

    ctx.begin_frame()
        .expect("headless begin_frame should succeed");
    renderer.render_world(&mut ctx, &world);
    ctx.end_frame();

    assert_eq!(renderer.stats().draw_calls, 1);
}

#[test]
fn opaque_mesh_phase_keeps_separate_draws_for_different_material_instances() {
    let (device, queue) = create_test_device();
    let mut ctx =
        GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [64, 64]);

    let pipeline = RenderPipelineBuilder::new()
        .register_material::<UnlitMaterial>()
        .add_phase(crate::render::expert::OpaquePhase::new())
        .build();
    let mut renderer = RenderComposer::from_asset(pipeline);
    renderer.register_material::<UnlitMaterial>(&ctx);

    let mesh_handle = renderer.insert_mesh(crate::render::expert::Mesh::builtin_quad(&ctx));
    let green = renderer
        .materials_mut::<UnlitMaterial>()
        .insert(UnlitMaterial::default().color(Color::new(0.3, 0.8, 0.4, 1.0)));
    let orange = renderer
        .materials_mut::<UnlitMaterial>()
        .insert(UnlitMaterial::default().color(Color::new(0.9, 0.5, 0.2, 1.0)));

    let mut world = World::new();
    world.spawn((
        Transform::default(),
        CameraMarker::new(),
        Projection::orthographic_fixed(64.0, 64.0),
        MainCamera,
    ));
    world.spawn((
        Transform::from_xyz(-8.0, 0.0, 0.2).with_scale(16.0, 16.0),
        WgpuMeshRenderer::new(mesh_handle, green),
    ));
    world.spawn((
        Transform::from_xyz(8.0, 0.0, 0.2).with_scale(16.0, 16.0),
        WgpuMeshRenderer::new(mesh_handle, orange),
    ));

    ctx.begin_frame()
        .expect("headless begin_frame should succeed");
    renderer.render_world(&mut ctx, &world);
    ctx.end_frame();

    assert_eq!(renderer.stats().draw_calls, 2);
}

#[test]
fn collect_world_views_uses_camera_projection_viewport_and_layer_mask() {
    let mut renderer = RenderComposer::from_asset(RenderPipelineAsset::builder().build());
    renderer.runtime.surface_size = [800, 600];

    let mut world = World::new();
    world.spawn((
        Transform::from_xyz(12.0, -4.0, 8.0),
        CameraMarker::new(),
        Projection::orthographic_fixed(320.0, 180.0),
        CameraViewport::new(ViewportRect::new(100, 50, 400, 300))
            .order(7)
            .layer_mask(0b0011),
        MainCamera,
    ));
    world.spawn((
        Transform::default(),
        CameraMarker::new().enabled(false),
        Projection::orthographic_fixed(64.0, 64.0),
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
    assert!(view.is_planar_2d);
}

#[test]
fn collect_world_views_prefers_explicit_viewports_over_implicit_main_camera() {
    let mut renderer = RenderComposer::from_asset(RenderPipelineAsset::builder().build());
    renderer.runtime.surface_size = [800, 600];

    let mut world = World::new();
    world.spawn((
        Transform::from_xyz(10.0, 20.0, 30.0),
        CameraMarker::new(),
        Projection::orthographic_fixed(320.0, 180.0),
        MainCamera,
    ));
    world.spawn((
        Transform::from_xyz(-4.0, 6.0, 8.0),
        CameraMarker::new(),
        Projection::orthographic_fixed(160.0, 90.0),
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
    let mut renderer = RenderComposer::from_asset(RenderPipelineAsset::builder().build());
    renderer.runtime.surface_size = [800, 600];

    let mut world = World::new();
    world.spawn((
        Transform::from_xyz(1.0, 2.0, 3.0),
        CameraMarker::new(),
        Projection::orthographic_fixed(320.0, 180.0),
    ));
    world.spawn((
        Transform::from_xyz(11.0, 12.0, 13.0),
        CameraMarker::new(),
        Projection::orthographic_fixed(640.0, 360.0),
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
fn collect_world_views_reports_missing_projection_diagnostic() {
    let mut renderer = RenderComposer::from_asset(RenderPipelineAsset::builder().build());
    renderer.runtime.surface_size = [800, 600];

    let mut world = World::new();
    world.insert_resource(Diagnostics::default());
    let camera = world.spawn((Transform::default(), CameraMarker::new(), MainCamera));

    let resolved = renderer.resolve_scene_transforms(&world);
    let views = renderer.collect_world_views(&world, &resolved);

    assert_eq!(views.len(), 1);
    let diagnostics = world
        .get_resource::<Diagnostics>()
        .expect("diagnostics should exist")
        .entries();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(
        diagnostics[0].id.as_str(),
        EngineDiagnosticKind::CAMERA_MISSING_PROJECTION
    );
    assert_eq!(diagnostics[0].subsystem, DiagnosticSubsystem::render());
    assert_eq!(diagnostics[0].severity, DiagnosticSeverity::Warning);
    assert_eq!(diagnostics[0].entity, Some(camera));
    assert_eq!(diagnostics[0].title, "Camera is missing a Projection");
    assert_eq!(
        diagnostics[0].help.as_deref(),
        Some(
            "Add Projection::orthographic(height) for stable world-unit sizing, or \
             Projection::orthographic_fixed(width, height) for a fixed logical view."
        )
    );
}

#[test]
fn projection_view_uniforms_stay_finite_for_orthographic_and_perspective() {
    for projection in [
        Projection::orthographic_fixed(1280.0, 720.0),
        Projection::perspective(60.0f32.to_radians(), 0.1, 1000.0),
    ] {
        let uniform = projection.view_uniform(Transform::from_xyz(3.0, 4.0, 5.0), [1280, 720]);
        assert!(uniform.view_proj.iter().all(|value| value.is_finite()));
        assert!(uniform.camera.iter().all(|value| value.is_finite()));
        assert!(uniform.viewport.iter().all(|value| value.is_finite()));
    }
}

#[test]
fn perspective_screen_to_world_intersects_the_world_z_plane() {
    let projection = Projection::perspective(60.0f32.to_radians(), 0.1, 1000.0);
    let transform = Transform::from_xyz(10.0, 20.0, 10.0);

    let center =
        projection.screen_to_world(transform, [800.0, 600.0].into(), [400.0, 300.0].into());
    assert!((center[0] - 10.0).abs() <= 0.001);
    assert!((center[1] - 20.0).abs() <= 0.001);

    let top_left = projection.screen_to_world(transform, [800.0, 600.0].into(), [0.0, 0.0].into());
    assert!(top_left[0] < transform.x());
    assert!(top_left[1] > transform.y());
    assert!(top_left.to_array().iter().all(|value| value.is_finite()));
}

#[test]
fn orthographic_screen_to_world_respects_camera_rotation() {
    let projection = Projection::orthographic_fixed(100.0, 50.0);
    let transform = Transform::from_xy(10.0, 20.0).with_rotation(std::f32::consts::FRAC_PI_2);

    let center = projection.screen_to_world(transform, [200.0, 100.0].into(), [100.0, 50.0].into());
    assert!((center[0] - 10.0).abs() <= 0.001);
    assert!((center[1] - 20.0).abs() <= 0.001);

    let right_edge =
        projection.screen_to_world(transform, [200.0, 100.0].into(), [200.0, 50.0].into());
    assert!((right_edge[0] - 10.0).abs() <= 0.001);
    assert!((right_edge[1] - 70.0).abs() <= 0.001);
}

#[test]
fn perspective_view_extraction_keeps_camera_depth_and_disables_2d_culling() {
    let mut renderer = RenderComposer::from_asset(RenderPipelineAsset::builder().build());
    renderer.runtime.surface_size = [1280, 720];

    let mut world = World::new();
    world.spawn((
        Transform::from_xyz(3.0, 4.0, 12.0),
        CameraMarker::new(),
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
    assert!(!view.is_planar_2d);
    assert!(view
        .view_uniform
        .view_proj
        .iter()
        .all(|value| value.is_finite()));
}

#[test]
fn tilted_orthographic_view_disables_2d_culling() {
    let mut renderer = RenderComposer::from_asset(RenderPipelineAsset::builder().build());
    renderer.runtime.surface_size = [800, 600];

    let mut world = World::new();
    world.spawn((
        Transform::from_xyz(0.0, 0.0, 10.0).with_euler_angles(0.35, 0.0, 0.0),
        CameraMarker::new(),
        Projection::orthographic_fixed(320.0, 180.0),
        MainCamera,
    ));

    let resolved = renderer.resolve_scene_transforms(&world);
    let views = renderer.collect_world_views(&world, &resolved);
    assert_eq!(views.len(), 1);
    let view = views[0];
    assert!(!view.is_planar_2d);
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
            sorting_layer: SortingLayer(10),
        },
        Live2DSceneInstance {
            entity: EntityId::new(1, 0),
            model_index: 1,
            transform: Transform::from_xyz(0.0, 0.0, 0.2),
            layer_mask: 0b0010,
            sorting_layer: SortingLayer(0),
        },
        Live2DSceneInstance {
            entity: EntityId::new(2, 0),
            model_index: 2,
            transform: Transform::from_xyz(0.0, 0.0, 0.1),
            layer_mask: 0b0010,
            sorting_layer: SortingLayer(10),
        },
    ];

    let mut transparent = base.clone();
    sort_live2d_scene_instances(&mut transparent, RenderQueueSort::TransparentScene, None);
    assert_eq!(
        transparent
            .iter()
            .map(|item| (item.entity.index(), item.sorting_layer.0))
            .collect::<Vec<_>>(),
        vec![(1, 0), (2, 10), (3, 10)]
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

    let projection = Projection::orthographic_fixed(64.0, 64.0);
    let view = SceneView::new(
        0,
        ViewportRect::new(0, 0, 64, 64),
        [64, 64],
        true,
        0b0010,
        Transform::default(),
        projection,
        projection.view_uniform(Transform::default(), [64, 64]),
        true,
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
        },
        Live2DSceneInstance {
            entity: EntityId::new(2, 0),
            model_index: 1,
            transform: Transform::from_xyz(0.0, 0.0, 5.0),
            layer_mask: u32::MAX,
            sorting_layer: SortingLayer(0),
        },
    ];
    let projection = Projection::perspective(60.0f32.to_radians(), 0.1, 1000.0);
    let perspective_view = SceneView::new(
        0,
        ViewportRect::new(0, 0, 64, 64),
        [64, 64],
        true,
        u32::MAX,
        Transform::from_xyz(0.0, 0.0, 10.0),
        projection,
        projection.view_uniform(Transform::from_xyz(0.0, 0.0, 10.0), [64, 64]),
        false,
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
