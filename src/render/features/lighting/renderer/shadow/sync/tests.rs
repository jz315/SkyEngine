use super::*;
use crate::ecs::World;
use crate::render::view::ProjectionViewUniformExt;
use crate::render::view::ViewportRect;

fn project_point(view_proj: [f32; 16], point: [f32; 3]) -> [f32; 3] {
    let clip =
        Mat4::from_cols_array(view_proj) * Vec4::from_array([point[0], point[1], point[2], 1.0]);
    let inv_w = clip.w().recip();
    [clip.x() * inv_w, clip.y() * inv_w, clip.z() * inv_w]
}

fn assert_in_range(label: &str, value: f32, min: f32, max: f32, tolerance: f32) {
    assert!(
        value >= min - tolerance && value <= max + tolerance,
        "{label}: {value} not in [{min}, {max}]"
    );
}

#[test]
fn directional_shadow_setup_resolves_fixed_cascade_contract() {
    let projection = Projection::perspective(60.0_f32.to_radians(), 0.1, 100.0);
    let main_view = SceneView::new(
        0,
        ViewportRect::from_surface_size([128, 128]),
        [128, 128],
        false,
        u32::MAX,
        Transform::default(),
        projection,
        projection.view_uniform(Transform::default(), [128, 128]),
        false,
    );
    let mut world = World::new();
    world.spawn((DirectionalLight::new([0.3, -1.0, 0.2])
        .cascade_count(3)
        .cascade_distances([12.0, 36.0, 90.0, 0.0])
        .cascade_blend(0.15)
        .shadow_resolution_per_cascade(512)
        .pcss_shadows(),));

    let mut views = vec![main_view];
    let setups = append_directional_shadow_views(&world, &mut views);

    assert_eq!(views.len(), 4);
    assert_eq!(setups.len(), 3);
    assert_eq!(setups[0].cascade_count, 3);
    assert_eq!(setups[0].cascade_index, 0);
    assert_eq!(setups[1].cascade_index, 1);
    assert_eq!(setups[2].cascade_index, 2);
    assert_eq!(views[1].shadow_cascade(), 0);
    assert_eq!(views[2].shadow_cascade(), 1);
    assert_eq!(views[3].shadow_cascade(), 2);
    assert_eq!(setups[0].cascade_splits, [12.0, 36.0, 90.0, 100.0]);
    assert_eq!(setups[0].cascade_blend, 0.15);
    assert_eq!(setups[0].resolution, 512);
    assert_eq!(setups[0].sampling_mode, ShadowSamplingMode::Pcss);
}

#[test]
fn demo_directional_shadow_views_cover_their_receiver_cascades() {
    let camera_position = [0.0, 3.573_966, 10.239_987];
    let camera_rotation =
        crate::math::Quat::from_xyzw_array([-0.089_878_55, 0.0, 0.0, 0.995_952_7]);
    let camera_transform =
        Transform::from_xyz(camera_position[0], camera_position[1], camera_position[2])
            .with_rotation_quat(camera_rotation);
    let projection = Projection::perspective(55.0_f32.to_radians(), 0.1, 80.0);
    let main_view = SceneView::new(
        0,
        ViewportRect::from_surface_size([1280, 720]),
        [1280, 720],
        false,
        u32::MAX,
        camera_transform,
        projection,
        projection.view_uniform(camera_transform, [1280, 720]),
        false,
    );
    let mut world = World::new();
    world.spawn((DirectionalLight::new([0.58, -1.0, 0.34])
        .cascade_count(4)
        .cascade_distances([5.5, 13.0, 30.0, 80.0])
        .shadow_resolution_per_cascade(2048)
        .radius(0.055)
        .shadow_bias(0.0008)
        .shadow_depth_bias(3)
        .shadow_slope_bias(1.8)
        .shadow_normal_bias(0.002)
        .pcss_shadows(),));

    let mut views = vec![main_view];
    let setups = append_directional_shadow_views(&world, &mut views);
    assert_eq!(setups.len(), 4);
    assert_eq!(views.len(), 5);

    let full_corners = view_frustum_corners_world(&main_view)
        .expect("demo main view should have invertible view projection");
    for cascade_index in 0..4 {
        let corners = cascade_frustum_corners_world(
            &full_corners,
            &main_view,
            &setups[0].cascade_splits,
            cascade_index,
        );
        let shadow_view = views
            .iter()
            .find(|view| view.is_shadow() && view.shadow_cascade() == cascade_index)
            .copied()
            .expect("shadow view should exist for cascade");
        for (corner_index, corner) in corners.iter().copied().enumerate() {
            let ndc = project_point(shadow_view.view_uniform.view_proj, corner);
            assert_in_range(
                &format!("cascade {cascade_index} corner {corner_index} x"),
                ndc[0],
                -1.0,
                1.0,
                0.002,
            );
            assert_in_range(
                &format!("cascade {cascade_index} corner {corner_index} y"),
                ndc[1],
                -1.0,
                1.0,
                0.002,
            );
            assert_in_range(
                &format!("cascade {cascade_index} corner {corner_index} z"),
                ndc[2],
                0.0,
                1.0,
                0.002,
            );
            assert!(
                shadow_view.frustum().intersects_sphere(corner, 0.01),
                "cascade {cascade_index} corner {corner_index} should pass shadow frustum culling"
            );
        }
    }
}

#[test]
fn cascade_frustum_splits_are_relative_to_camera_near_plane() {
    let projection = Projection::perspective(60.0_f32.to_radians(), 1.0, 11.0);
    let main_view = SceneView::new(
        0,
        ViewportRect::from_surface_size([128, 128]),
        [128, 128],
        false,
        u32::MAX,
        Transform::default(),
        projection,
        projection.view_uniform(Transform::default(), [128, 128]),
        false,
    );
    let full_corners = view_frustum_corners_world(&main_view)
        .expect("test view should have invertible view projection");
    let cascade_splits = [1.0, 6.0, 11.0, 11.0];

    let first = cascade_frustum_corners_world(&full_corners, &main_view, &cascade_splits, 0);
    let second = cascade_frustum_corners_world(&full_corners, &main_view, &cascade_splits, 1);

    for corner in 0..4 {
        assert_eq!(
            first[corner + 4],
            full_corners[corner],
            "a split at the camera near plane should not advance into the frustum"
        );
        assert_eq!(
            second[corner], full_corners[corner],
            "the next cascade should start exactly at the previous near-plane split"
        );
    }
}

#[test]
fn directional_shadow_corners_ignore_taa_jittered_view_projection() {
    let projection = Projection::perspective(60.0_f32.to_radians(), 0.1, 100.0);
    let transform = Transform::from_xyz(0.0, 1.0, 8.0);
    let mut main_view = SceneView::new(
        0,
        ViewportRect::from_surface_size([128, 128]),
        [128, 128],
        false,
        u32::MAX,
        transform,
        projection,
        projection.view_uniform(transform, [128, 128]),
        false,
    );
    let unjittered_corners =
        view_frustum_corners_world(&main_view).expect("main view should produce corners");
    let mut jittered_uniform = main_view.view_uniform;
    jittered_uniform.view_proj[12] += 0.25;
    jittered_uniform.view_proj[13] -= 0.25;
    main_view.set_jittered_view_uniform(jittered_uniform);

    assert_ne!(
        main_view.view_uniform.view_proj,
        main_view.unjittered_view_proj_matrix
    );
    assert_eq!(
        view_frustum_corners_world(&main_view).expect("jittered main view should produce corners"),
        unjittered_corners
    );
}

#[test]
fn directional_shadow_view_keeps_light_ray_casters_outside_receiver_slice() {
    let projection = Projection::perspective(60.0_f32.to_radians(), 0.1, 120.0);
    let main_view = SceneView::new(
        0,
        ViewportRect::from_surface_size([256, 256]),
        [256, 256],
        false,
        u32::MAX,
        Transform::default(),
        projection,
        projection.view_uniform(Transform::default(), [256, 256]),
        false,
    );
    let light = DirectionalLight::new([0.35, -1.0, -0.25])
        .cascade_count(4)
        .cascade_distances([8.0, 24.0, 60.0, 120.0])
        .shadow_resolution_per_cascade(512);

    let full_corners = view_frustum_corners_world(&main_view)
        .expect("test view should have invertible view projection");
    let cascade_splits = resolved_cascade_splits(light, &main_view, resolved_cascade_count(light));
    let corners = cascade_frustum_corners_world(&full_corners, &main_view, &cascade_splits, 0);
    let mut center_world = [0.0; 3];
    let light_view = Mat4::from_cols_array(light_view_matrix(
        Vec3::from_array(light.direction)
            .try_normalized()
            .expect("test light direction should normalize")
            .to_array(),
    ));
    let mut center_light_z = 0.0;
    let mut min_light_z = f32::INFINITY;
    for corner in corners {
        center_world[0] += corner[0];
        center_world[1] += corner[1];
        center_world[2] += corner[2];
        let light_space = light_view
            .transform_point3(Vec3::from_array(corner))
            .to_array();
        center_light_z += light_space[2];
        min_light_z = min_light_z.min(light_space[2]);
    }
    for axis in &mut center_world {
        *axis *= 0.125;
    }
    center_light_z *= 0.125;
    let previous_receiver_depth_extent = (center_light_z - min_light_z).abs() * 4.0;

    let (shadow_view, setup) =
        build_shadow_view(&main_view, light, 0, 0).expect("cascade 0 should build");
    assert!(
        setup.caster_depth_extent > setup.receiver_depth_extent + 8.0,
        "caster culling range should be wider than the tight receiver slice"
    );
    assert!(
        setup.receiver_depth_extent >= previous_receiver_depth_extent - 0.001,
        "sampling projection should keep the receiver slice depth range"
    );

    let light_direction = Vec3::from_array(light.direction)
        .try_normalized()
        .expect("test light direction should normalize");
    let caster_offset = (previous_receiver_depth_extent + 4.0).min(setup.caster_depth_extent - 1.0);
    let caster_center = Vec3::from_array(center_world) - light_direction * caster_offset;
    assert!(
        shadow_view
            .frustum()
            .intersects_sphere(caster_center.to_array(), 0.5),
        "casters between the light and the receiver slice must survive shadow-view culling"
    );
}

#[test]
fn packed_shadow_atlas_mul_add_matches_wicked_cascade_mapping() {
    let layout = ShadowAtlasLayout::directional_packed(512, 4, 1.0);
    let mul_add = layout.shadow_atlas_mul_add();
    let atlas_rcp = layout.shadow_atlas_resolution_rcp();

    assert_eq!(mul_add, [0.25, 1.0, 0.0, 0.0]);
    assert_eq!(atlas_rcp, [1.0 / 2048.0, 1.0 / 512.0, 1.0, 0.0]);
    assert_eq!(
        shadow_atlas_resolution_rcp_with_sampling_mode(layout, ShadowSamplingMode::DitheredPcf),
        [1.0 / 2048.0, 1.0 / 512.0, 1.0, 1.0]
    );

    for cascade in 0..4 {
        let left = ((0.0 + cascade as f32) * mul_add[0]) + mul_add[2];
        let right = ((1.0 + cascade as f32) * mul_add[0]) + mul_add[2];
        assert_eq!(left, cascade as f32 * 0.25);
        assert_eq!(right, (cascade + 1) as f32 * 0.25);
    }
}

#[test]
fn static_shadow_update_policy_only_marks_changed_cascades() {
    let previous = [11, 22, 33, 44];
    let same = previous;
    let changed = [11, 99, 33, 88];

    assert_eq!(
        cascade_update_mask(
            ShadowUpdatePolicy::StaticWhenUnchanged,
            false,
            4,
            &previous,
            &same
        ),
        0
    );
    assert_eq!(
        cascade_update_mask(
            ShadowUpdatePolicy::StaticWhenUnchanged,
            false,
            4,
            &previous,
            &changed
        ),
        0b1010
    );
    assert_eq!(
        cascade_update_mask(ShadowUpdatePolicy::EveryFrame, false, 3, &previous, &same),
        0b0111
    );
}
