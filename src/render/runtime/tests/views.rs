use super::common::*;

#[test]
fn collect_world_views_uses_camera_projection_viewport_and_layer_mask() {
    let mut renderer = RenderRuntime::from_asset(RenderPipelineAsset::builder().build());
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

    let resolved = renderer.runtime.view_collector.resolve_transforms(&world);
    let views = renderer.runtime.view_collector.collect_world_views(
        &world,
        &resolved,
        renderer.runtime.surface_size,
    );
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
    let mut renderer = RenderRuntime::from_asset(RenderPipelineAsset::builder().build());
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

    let resolved = renderer.runtime.view_collector.resolve_transforms(&world);
    let views = renderer.runtime.view_collector.collect_world_views(
        &world,
        &resolved,
        renderer.runtime.surface_size,
    );

    assert_eq!(views.len(), 1);
    assert_eq!(views[0].order, 5);
    assert_eq!(views[0].viewport, ViewportRect::new(50, 40, 320, 200));
    assert_eq!(views[0].view_uniform.camera, [-4.0, 6.0, 8.0, 1.0]);
}

#[test]
fn collect_world_views_uses_main_camera_for_implicit_view_selection() {
    let mut renderer = RenderRuntime::from_asset(RenderPipelineAsset::builder().build());
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

    let resolved = renderer.runtime.view_collector.resolve_transforms(&world);
    let views = renderer.runtime.view_collector.collect_world_views(
        &world,
        &resolved,
        renderer.runtime.surface_size,
    );

    assert_eq!(views.len(), 1);
    assert_eq!(views[0].viewport, ViewportRect::new(0, 0, 800, 600));
    assert_eq!(views[0].target_size, [800, 600]);
    assert_eq!(views[0].view_uniform.camera, [11.0, 12.0, 13.0, 1.0]);
}

#[test]
fn collect_world_views_uses_fallback_for_missing_projection() {
    let mut renderer = RenderRuntime::from_asset(RenderPipelineAsset::builder().build());
    renderer.runtime.surface_size = [800, 600];

    let mut world = World::new();
    world.spawn((Transform::default(), CameraMarker::new(), MainCamera));

    let resolved = renderer.runtime.view_collector.resolve_transforms(&world);
    let views = renderer.runtime.view_collector.collect_world_views(
        &world,
        &resolved,
        renderer.runtime.surface_size,
    );

    assert_eq!(views.len(), 1);
    assert_eq!(views[0].viewport, ViewportRect::new(0, 0, 800, 600));
    assert_eq!(views[0].target_size, [800, 600]);
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
    let mut renderer = RenderRuntime::from_asset(RenderPipelineAsset::builder().build());
    renderer.runtime.surface_size = [1280, 720];

    let mut world = World::new();
    world.spawn((
        Transform::from_xyz(3.0, 4.0, 12.0),
        CameraMarker::new(),
        Projection::perspective(60.0f32.to_radians(), 0.1, 500.0),
        CameraViewport::new(ViewportRect::new(0, 0, 640, 360)).order(2),
        MainCamera,
    ));

    let resolved = renderer.runtime.view_collector.resolve_transforms(&world);
    let views = renderer.runtime.view_collector.collect_world_views(
        &world,
        &resolved,
        renderer.runtime.surface_size,
    );
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
    let mut renderer = RenderRuntime::from_asset(RenderPipelineAsset::builder().build());
    renderer.runtime.surface_size = [800, 600];

    let mut world = World::new();
    world.spawn((
        Transform::from_xyz(0.0, 0.0, 10.0).with_euler_angles(0.35, 0.0, 0.0),
        CameraMarker::new(),
        Projection::orthographic_fixed(320.0, 180.0),
        MainCamera,
    ));

    let resolved = renderer.runtime.view_collector.resolve_transforms(&world);
    let views = renderer.runtime.view_collector.collect_world_views(
        &world,
        &resolved,
        renderer.runtime.surface_size,
    );
    assert_eq!(views.len(), 1);
    let view = views[0];
    assert!(!view.is_planar_2d);
    assert!(view
        .view_uniform
        .view_proj
        .iter()
        .all(|value| value.is_finite()));
}
