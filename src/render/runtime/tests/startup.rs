use super::common::*;

#[test]
fn declared_materials_are_available_after_gpu_initialization_before_first_render() {
    let (device, queue) = create_test_device();
    let ctx = GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [64, 64]);
    let mut renderer = RenderRuntime::from_asset(RenderPipelineAsset::modern_3d());

    renderer.prepare_gpu_resources(&ctx);

    let material = renderer.insert_material::<StandardMaterial>(StandardMaterial::default());
    assert!(renderer.material::<StandardMaterial>(material).is_ok());
}

#[test]
fn builder_unlit_pipeline_renders_default_sprite_scene() {
    let (device, queue) = create_test_device();
    let mut ctx =
        GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [64, 64]);
    let mut renderer = RenderRuntime::from_asset(
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
