use super::common::*;

#[test]
fn sprite_texture_asset_handle_uploads_into_render_cache() {
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
    let mut renderer = RenderRuntime::from_asset(
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
    let mut renderer = RenderRuntime::from_asset(
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
    let mut renderer = RenderRuntime::from_asset(
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
    let mut renderer = RenderRuntime::from_asset(
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
