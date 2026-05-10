use super::common::*;

#[test]
fn register_material_automatically_wires_mesh_draw_and_extract() {
    let (device, queue) = create_test_device();
    let mut ctx =
        GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [64, 64]);

    let pipeline = RenderPipelineBuilder::new()
        .register_material::<UnlitMaterial>()
        .add_phase(crate::render::expert::OpaquePhase::new())
        .build();
    let mut renderer = RenderRuntime::from_asset(pipeline);
    renderer.register_material::<UnlitMaterial>(&ctx);

    let mesh_handle = renderer.insert_mesh(crate::render::expert::Mesh::builtin_quad(&ctx));
    let material_handle = renderer.insert_material::<UnlitMaterial>(
        UnlitMaterial::default().color(Color::new(0.3, 0.8, 0.4, 1.0)),
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
    let mut renderer = RenderRuntime::from_asset(
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
    let mut renderer = RenderRuntime::from_asset(pipeline);
    renderer.register_material::<UnlitMaterial>(&ctx);

    let mesh_handle = renderer.insert_mesh(crate::render::expert::Mesh::builtin_quad(&ctx));
    let material_handle = renderer.insert_material::<UnlitMaterial>(
        UnlitMaterial::default().color(Color::new(0.3, 0.8, 0.4, 1.0)),
    );

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
    let mut renderer = RenderRuntime::from_asset(pipeline);
    renderer.register_material::<UnlitMaterial>(&ctx);

    let mesh_handle = renderer.insert_mesh(crate::render::expert::Mesh::builtin_quad(&ctx));
    let green = renderer.insert_material::<UnlitMaterial>(
        UnlitMaterial::default().color(Color::new(0.3, 0.8, 0.4, 1.0)),
    );
    let orange = renderer.insert_material::<UnlitMaterial>(
        UnlitMaterial::default().color(Color::new(0.9, 0.5, 0.2, 1.0)),
    );

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
