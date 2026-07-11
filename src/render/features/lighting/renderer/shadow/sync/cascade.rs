use super::*;

pub(crate) fn shadow_debug_mode(debug_view: RenderDebugView) -> f32 {
    match debug_view {
        RenderDebugView::DirectionalShadowCoverage => 1.0,
        RenderDebugView::DirectionalShadowSplitCoverage => 2.0,
        RenderDebugView::DirectionalShadowFade => 3.0,
        RenderDebugView::DirectionalShadowCompareDelta => 4.0,
        RenderDebugView::DirectionalShadowBias => 5.0,
        RenderDebugView::DirectionalShadowPcss => 6.0,
        RenderDebugView::DirectLighting => 7.0,
        RenderDebugView::IndirectLighting => 8.0,
        _ => 0.0,
    }
}

pub(crate) fn shadow_temporal_rotation_seed(views: &[SceneView]) -> f32 {
    let Some(view) = views.iter().find(|view| !view.is_shadow()) else {
        return 0.0;
    };
    if view.temporal.jitter == [0.0, 0.0] && view.temporal.previous_jitter == [0.0, 0.0] {
        0.0
    } else {
        (view.temporal.frame_index % 256) as f32 / 256.0
    }
}

pub(crate) fn shadow_filter_radius(light: DirectionalLight) -> f32 {
    if light.shadow_filter_radius <= 0.0 {
        0.0
    } else {
        light.shadow_filter_radius.max(light.radius).max(0.0)
    }
}

pub(crate) fn select_shadow_light(
    lights: &[DirectionalLight],
    layer_mask: u32,
) -> Option<DirectionalLight> {
    lights
        .iter()
        .copied()
        .filter(|light| light.layer_mask & layer_mask != 0)
        .max_by(|lhs, rhs| lhs.intensity.total_cmp(&rhs.intensity))
}

pub(crate) fn build_shadow_view(
    view: &SceneView,
    light: DirectionalLight,
    binding_index: usize,
    cascade_index: u32,
) -> Option<(SceneView, DirectionalShadowSetup)> {
    let light_direction = Vec3::from_array(light.direction)
        .try_normalized()?
        .to_array();
    let cascade_count = resolved_cascade_count(light);
    let cascade_splits = resolved_cascade_splits(light, view, cascade_count);
    if cascade_index >= cascade_count {
        return None;
    }
    let full_corners = view_frustum_corners_world(view)?;
    let corners =
        cascade_frustum_corners_world(&full_corners, view, &cascade_splits, cascade_index);
    let light_view = light_view_matrix(light_direction);
    let light_view_matrix = Mat4::from_cols_array(light_view);

    // Ported from WickedEngine's MIT-licensed CreateDirLightShadowCams():
    // transform the camera frustum into light space, fit a bounding sphere,
    // and snap that sphere-aligned box to the shadow texel grid.
    let mut center = [0.0; 3];
    let mut light_space_corners = [[0.0; 3]; 8];
    for (index, corner) in corners.iter().enumerate() {
        let light_space = light_view_matrix
            .transform_point3(Vec3::from_array(*corner))
            .to_array();
        light_space_corners[index] = light_space;
        center[0] += light_space[0];
        center[1] += light_space[1];
        center[2] += light_space[2];
    }
    let center_scale = (light_space_corners.len() as f32).recip();
    center[0] *= center_scale;
    center[1] *= center_scale;
    center[2] *= center_scale;

    let mut radius = 0.0f32;
    for corner in &light_space_corners {
        let delta = [
            corner[0] - center[0],
            corner[1] - center[1],
            corner[2] - center[2],
        ];
        radius =
            radius.max((delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2]).sqrt());
    }
    if !radius.is_finite() || radius <= 0.0 {
        return None;
    }

    let mut min_x = center[0] - radius;
    let mut max_x = center[0] + radius;
    let mut min_y = center[1] - radius;
    let mut max_y = center[1] + radius;
    let min_z = center[2] - radius;

    let resolution = light.shadow_resolution_per_cascade.max(1);
    let texel_size_x = ((max_x - min_x) / resolution as f32).max(f32::EPSILON);
    let texel_size_y = ((max_y - min_y) / resolution as f32).max(f32::EPSILON);
    min_x = (min_x / texel_size_x).floor() * texel_size_x;
    max_x = (max_x / texel_size_x).floor() * texel_size_x;
    min_y = (min_y / texel_size_y).floor() * texel_size_y;
    max_y = (max_y / texel_size_y).floor() * texel_size_y;
    center[0] = (min_x + max_x) * 0.5;
    center[1] = (min_y + max_y) * 0.5;

    // Wicked expands the receiver slice for the real projection, then uses an
    // even coarser Z range for caster culling so off-slice casters can still
    // write into the cascade. The shadow shaders clamp clip Z to emulate the
    // depth-clamp part of that contract without requiring DEPTH_CLIP_CONTROL.
    let receiver_depth_extent = (center[2] - min_z).abs() * 4.0;
    let caster_depth_extent = if view.far.is_finite() {
        view.far.clamp(0.0, 2000.0) * 0.5
    } else {
        0.0
    };
    let culling_depth_extent = receiver_depth_extent.max(caster_depth_extent);
    let min_z = center[2] - receiver_depth_extent;
    let max_z = center[2] + receiver_depth_extent;
    let culling_min_z = center[2] - culling_depth_extent;
    let culling_max_z = center[2] + culling_depth_extent;
    let world_extent = [(max_x - min_x).abs(), (max_y - min_y).abs()];
    let texel_world_size = [
        world_extent[0] / resolution as f32,
        world_extent[1] / resolution as f32,
    ];

    let near = -max_z;
    let far = -min_z;
    if !near.is_finite() || !far.is_finite() || (far - near).abs() <= 1e-5 {
        return None;
    }
    let culling_near = -culling_max_z;
    let culling_far = -culling_min_z;
    if !culling_near.is_finite()
        || !culling_far.is_finite()
        || (culling_far - culling_near).abs() <= 1e-5
    {
        return None;
    }

    let projection = Mat4::orthographic_rh(min_x, max_x, min_y, max_y, near, far);
    let culling_projection =
        Mat4::orthographic_rh(min_x, max_x, min_y, max_y, culling_near, culling_far);
    let inverse_view = light_view_matrix.inverse().to_cols_array();
    let camera_position = [inverse_view[12], inverse_view[13], inverse_view[14]];
    let view_proj = (projection * light_view_matrix).to_cols_array();
    let culling_view_proj = (culling_projection * light_view_matrix).to_cols_array();
    let view_uniform = crate::render::view::ViewUniform {
        view_proj: culling_view_proj,
        camera: [
            camera_position[0],
            camera_position[1],
            camera_position[2],
            1.0,
        ],
        viewport: [
            resolution as f32,
            resolution as f32,
            (resolution as f32).recip(),
            (resolution as f32).recip(),
        ],
        view: light_view,
        projection: culling_projection.to_cols_array(),
        inverse_view,
        camera_position: [
            camera_position[0],
            camera_position[1],
            camera_position[2],
            1.0,
        ],
        near_far_time_delta: [culling_near, culling_far, 0.0, 0.0],
    };
    let shadow_view = SceneView::from_parts(
        view.order,
        view.viewport,
        [resolution, resolution],
        false,
        SceneViewKind::DirectionalShadow,
        Some(binding_index),
        view.layer_mask,
        Transform::from_xyz(camera_position[0], camera_position[1], camera_position[2]),
        Projection::orthographic_fixed((max_x - min_x).max(1e-3), (max_y - min_y).max(1e-3)),
        view_uniform,
        false,
    )
    .with_shadow_binding_and_cascade(binding_index, cascade_index);

    Some((
        shadow_view,
        DirectionalShadowSetup {
            binding_index,
            cascade_index,
            light_direction,
            light_view_proj: view_proj,
            resolution,
            world_extent,
            texel_world_size,
            receiver_depth_extent,
            caster_depth_extent: culling_depth_extent,
            bias: light.shadow_bias.max(0.0),
            radius: shadow_filter_radius(light),
            raster_bias: ShadowRasterBias::new(
                light.shadow_depth_bias,
                light.shadow_slope_bias.max(0.0),
                0.0,
            ),
            normal_bias: light.shadow_normal_bias.max(0.0),
            cascade_count,
            cascade_splits,
            cascade_blend: light.cascade_blend.max(0.0),
            sampling_mode: light.shadow_sampling_mode,
            update_policy: light.shadow_update_policy,
        },
    ))
}

pub(crate) fn shadow_atlas_resolution_rcp_with_sampling_mode(
    atlas_layout: ShadowAtlasLayout,
    sampling_mode: ShadowSamplingMode,
) -> [f32; 4] {
    let mut params = atlas_layout.shadow_atlas_resolution_rcp();
    params[3] = sampling_mode.shader_code();
    params
}

pub(crate) fn cascade_update_mask(
    policy: ShadowUpdatePolicy,
    force_update: bool,
    cascade_count: u32,
    previous_signatures: &[u64; MAX_DIRECTIONAL_SHADOW_CASCADES],
    next_signatures: &[u64; MAX_DIRECTIONAL_SHADOW_CASCADES],
) -> u32 {
    let active_mask = if cascade_count >= MAX_DIRECTIONAL_SHADOW_CASCADES as u32 {
        (1u32 << MAX_DIRECTIONAL_SHADOW_CASCADES) - 1
    } else {
        (1u32 << cascade_count.max(1)) - 1
    };
    if force_update || matches!(policy, ShadowUpdatePolicy::EveryFrame) {
        return active_mask;
    }

    let mut mask = 0u32;
    for cascade in 0..cascade_count.min(MAX_DIRECTIONAL_SHADOW_CASCADES as u32) {
        let index = cascade as usize;
        if previous_signatures[index] != next_signatures[index] {
            mask |= 1u32 << cascade;
        }
    }
    mask
}

pub(crate) fn shadow_cascade_signature(
    setup: &DirectionalShadowSetup,
    scene_view: SceneView,
    opaque_phase: &OpaquePhase,
    transparent_phase: &TransparentPhase,
    model_matrices: &[[f32; 16]],
) -> u64 {
    let mut hasher = rustc_hash::FxHasher::default();
    setup.binding_index.hash(&mut hasher);
    setup.cascade_index.hash(&mut hasher);
    setup.resolution.hash(&mut hasher);
    setup.bias.to_bits().hash(&mut hasher);
    setup.radius.to_bits().hash(&mut hasher);
    setup.raster_bias.constant.hash(&mut hasher);
    setup.raster_bias.slope_scale.to_bits().hash(&mut hasher);
    setup.raster_bias.clamp.to_bits().hash(&mut hasher);
    setup.normal_bias.to_bits().hash(&mut hasher);
    setup.light_view_proj.hash_f32_array(&mut hasher);
    setup.receiver_depth_extent.to_bits().hash(&mut hasher);
    setup.caster_depth_extent.to_bits().hash(&mut hasher);
    setup.cascade_count.hash(&mut hasher);
    for split in setup.cascade_splits {
        split.to_bits().hash(&mut hasher);
    }
    setup.cascade_blend.to_bits().hash(&mut hasher);
    setup.sampling_mode.hash(&mut hasher);
    setup.update_policy.hash(&mut hasher);
    scene_view.shadow_cascade().hash(&mut hasher);
    scene_view
        .view_uniform
        .view_proj
        .hash_f32_array(&mut hasher);
    hash_shadow_phase_items(opaque_phase.items(), model_matrices, &mut hasher);
    hash_shadow_phase_items(transparent_phase.items(), model_matrices, &mut hasher);
    hasher.finish()
}

pub(crate) fn hash_shadow_phase_items(
    items: &[crate::render::phase::PhaseItem],
    model_matrices: &[[f32; 16]],
    hasher: &mut impl Hasher,
) {
    items.len().hash(&mut *hasher);
    for item in items {
        item.draw_function_id.hash(&mut *hasher);
        item.entity.hash(&mut *hasher);
        item.batch_key.hash(&mut *hasher);
        let draw = *item.data::<MeshDrawData>();
        draw.hash(&mut *hasher);
        if let Some(model) = model_matrices.get(draw.model_slot() as usize) {
            (*model).hash_f32_array(&mut *hasher);
        }
    }
}

trait HashF32Array {
    fn hash_f32_array(self, hasher: &mut impl Hasher);
}

impl<const N: usize> HashF32Array for [f32; N] {
    fn hash_f32_array(self, hasher: &mut impl Hasher) {
        for value in self {
            value.to_bits().hash(hasher);
        }
    }
}

pub(crate) fn resolved_cascade_count(light: DirectionalLight) -> u32 {
    light
        .cascade_count
        .clamp(1, MAX_DIRECTIONAL_SHADOW_CASCADES as u32)
}

pub(crate) fn resolved_cascade_splits(
    light: DirectionalLight,
    view: &SceneView,
    cascade_count: u32,
) -> [f32; MAX_DIRECTIONAL_SHADOW_CASCADES] {
    let mut splits = [view.far.max(view.near + 1.0); MAX_DIRECTIONAL_SHADOW_CASCADES];
    let near = view.near.max(0.0);
    let far = if view.far.is_finite() && view.far > near {
        view.far
    } else {
        near + 1.0
    };
    let active_count = cascade_count.clamp(1, MAX_DIRECTIONAL_SHADOW_CASCADES as u32) as usize;
    let mut previous = near;
    for (index, split) in splits.iter_mut().take(active_count).enumerate() {
        let fallback = near + (far - near) * ((index + 1) as f32 / active_count as f32);
        let requested = light.cascade_distances[index];
        let candidate = if requested.is_finite() && requested > previous {
            requested
        } else {
            fallback
        };
        *split = candidate.clamp(previous + 1e-4, far);
        previous = *split;
    }
    splits
}

pub(crate) fn cascade_frustum_corners_world(
    full_corners: &[[f32; 3]; 8],
    view: &SceneView,
    cascade_splits: &[f32; MAX_DIRECTIONAL_SHADOW_CASCADES],
    cascade_index: u32,
) -> [[f32; 3]; 8] {
    let near = view.near.max(0.0);
    let far = view.far.max(1e-4);
    let depth_span = (far - near).max(1e-4);
    let near_split = if cascade_index == 0 {
        0.0
    } else {
        (cascade_splits[cascade_index as usize - 1] - near) / depth_span
    }
    .clamp(0.0, 1.0);
    let far_split = (cascade_splits[cascade_index as usize].clamp(near, far) - near) / depth_span;
    let far_split = far_split.clamp(near_split, 1.0);

    let mut corners = [[0.0; 3]; 8];
    for corner in 0..4 {
        let near = full_corners[corner];
        let far = full_corners[corner + 4];
        corners[corner] = lerp_point(near, far, near_split);
        corners[corner + 4] = lerp_point(near, far, far_split);
    }
    corners
}

pub(crate) fn lerp_point(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

pub(crate) fn view_frustum_corners_world(view: &SceneView) -> Option<[[f32; 3]; 8]> {
    // Wicked builds directional shadow cascades from the camera with jitter removed.
    // Keep the shadow projection stable even when the main view is TAA-jittered.
    let inverse_view_proj = Mat4::from_cols_array(view.unjittered_view_proj_matrix).inverse();
    let corners = [
        [-1.0, -1.0, 0.0],
        [1.0, -1.0, 0.0],
        [1.0, 1.0, 0.0],
        [-1.0, 1.0, 0.0],
        [-1.0, -1.0, 1.0],
        [1.0, -1.0, 1.0],
        [1.0, 1.0, 1.0],
        [-1.0, 1.0, 1.0],
    ];

    let mut world = [[0.0; 3]; 8];
    for (index, corner) in corners.into_iter().enumerate() {
        let clip = Vec4::from_array([corner[0], corner[1], corner[2], 1.0]);
        let homogeneous = inverse_view_proj * clip;
        if homogeneous.w().abs() <= 1e-6 {
            return None;
        }
        let inv_w = homogeneous.w().recip();
        world[index] = [
            homogeneous.x() * inv_w,
            homogeneous.y() * inv_w,
            homogeneous.z() * inv_w,
        ];
    }
    Some(world)
}

pub(crate) fn light_view_matrix(light_direction: [f32; 3]) -> [f32; 16] {
    let up_guess = if light_direction[1].abs() > 0.98 {
        [0.0, 0.0, 1.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    Mat4::look_to_rh(
        Vec3::from_array([0.0, 0.0, 0.0]),
        Vec3::from_array(light_direction),
        Vec3::from_array(up_guess),
    )
    .to_cols_array()
}

pub(crate) const IDENTITY_MATRIX: [f32; 16] = [
    1.0, 0.0, 0.0, 0.0, //
    0.0, 1.0, 0.0, 0.0, //
    0.0, 0.0, 1.0, 0.0, //
    0.0, 0.0, 0.0, 1.0,
];
