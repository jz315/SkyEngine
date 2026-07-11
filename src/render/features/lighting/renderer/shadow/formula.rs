const SHADOW_FILTER_RADIUS_SCALE: f32 = 8.0;
const SHADOW_FILTER_RADIUS_BIAS: f32 = 2.0;
const SHADOW_PCSS_MAX_FILTER_TEXELS: f32 = 36.0;
const SHADOW_PCSS_PENUMBRA_SCALE: f32 = 200.0;
const SHADOW_PCSS_MAX_PENUMBRA_SCALE: f32 = 4.0;
const SHADOW_PCSS_BLOCKER_GRID_RADIUS: i32 = 2;
const SHADOW_PCSS_BLOCKER_GRID_STEP_SCALE: f32 = 4.0;
const SHADOW_PCSS_BLOCKER_DEPTH_BIAS_TEXELS: f32 = 6.0;
const SHADOW_CASCADE_MAX: u32 = 4;
const SHADOW_RECEIVER_NORMAL_TEXEL_BIAS: f32 = 0.25;
const SHADOW_COMPARE_TEXEL_BIAS_BASE: f32 = 0.25;
const SHADOW_COMPARE_TEXEL_BIAS_SLOPE: f32 = 0.75;
const SHADOW_MATERIAL_COMPARE_BIAS_SCALE: f32 = 0.0;

#[derive(Clone, Copy, Debug, PartialEq)]
struct PcssBlocker {
    average_gap: f32,
    count: f32,
}

#[derive(Clone, Copy, Debug)]
struct BiasInputs {
    compare_bias: f32,
    texel_world_size: f32,
    depth_range: f32,
    normal_bias_world: f32,
    light_direction: [f32; 3],
}

fn cascade_index(view_depth: f32, cascade_count: u32, cascade_splits: [f32; 4]) -> u32 {
    let cascade_count = cascade_count.clamp(1, SHADOW_CASCADE_MAX);
    let mut cascade = 0;
    for i in 0..SHADOW_CASCADE_MAX {
        if i < cascade_count && view_depth > cascade_splits[i as usize] {
            cascade = (i + 1).min(cascade_count - 1);
        }
    }
    cascade
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn normalize_or(value: [f32; 3], fallback: [f32; 3]) -> [f32; 3] {
    let len_sq = dot(value, value);
    if len_sq <= 0.000001 {
        return fallback;
    }
    let inv_len = len_sq.sqrt().recip();
    [value[0] * inv_len, value[1] * inv_len, value[2] * inv_len]
}

fn receiver_ndotl(normal: [f32; 3], light_direction: [f32; 3]) -> f32 {
    let normal = normalize_or(normal, [0.0, 1.0, 0.0]);
    let light_to_receiver = normalize_or(
        [
            -light_direction[0],
            -light_direction[1],
            -light_direction[2],
        ],
        [0.0, 1.0, 0.0],
    );
    dot(normal, light_to_receiver).clamp(0.0, 1.0)
}

fn receiver_bias_world(normal: [f32; 3], inputs: BiasInputs) -> f32 {
    let texel_world_size = inputs.texel_world_size.max(0.000001);
    let ndotl = receiver_ndotl(normal, inputs.light_direction);
    let texel_bias = texel_world_size * SHADOW_RECEIVER_NORMAL_TEXEL_BIAS * (1.0 - ndotl);
    inputs.normal_bias_world.max(0.0).max(texel_bias)
}

fn compare_depth(depth: f32, normal: [f32; 3], inputs: BiasInputs) -> f32 {
    (depth - compare_bias_depth(normal, inputs)).clamp(0.0, 1.0)
}

fn compare_bias_depth(normal: [f32; 3], inputs: BiasInputs) -> f32 {
    let texel_world_size = inputs.texel_world_size.max(0.000001);
    let depth_range = inputs.depth_range.max(texel_world_size);
    let ndotl = receiver_ndotl(normal, inputs.light_direction);
    let texel_depth_bias = (texel_world_size / depth_range)
        * (SHADOW_COMPARE_TEXEL_BIAS_BASE + SHADOW_COMPARE_TEXEL_BIAS_SLOPE * (1.0 - ndotl));
    let normal_depth_bias = receiver_bias_world(normal, inputs) / depth_range;
    (inputs.compare_bias * SHADOW_MATERIAL_COMPARE_BIAS_SCALE)
        .max(texel_depth_bias)
        .max(normal_depth_bias)
}

fn pcss_blocker_depth_bias(texel_world_size: f32, depth_range: f32) -> f32 {
    let texel_world_size = texel_world_size.max(0.000001);
    let depth_range = depth_range.max(texel_world_size);
    (texel_world_size / depth_range) * SHADOW_PCSS_BLOCKER_DEPTH_BIAS_TEXELS
}

fn cascade_edge_fade(light_ndc: [f32; 3], blend_width: f32) -> f32 {
    if blend_width <= 0.0 {
        return 0.0;
    }
    let width = blend_width.clamp(0.001, 0.5);
    let fade_start = 1.0 - width;
    let shadow_box = [light_ndc[0], light_ndc[1], light_ndc[2] * 2.0 - 1.0];
    shadow_box
        .into_iter()
        .map(|value| ((value.abs() - fade_start) / width).clamp(0.0, 1.0))
        .fold(0.0, f32::max)
}

fn cascade_split_blend_width(
    cascade_before_split: u32,
    cascade_splits: [f32; 4],
    blend_fraction: f32,
) -> f32 {
    if blend_fraction <= 0.0 {
        return 0.0;
    }

    let far_split = cascade_splits[cascade_before_split as usize];
    let near_split = if cascade_before_split == 0 {
        0.0
    } else {
        cascade_splits[cascade_before_split as usize - 1]
    };
    let cascade_depth = (far_split - near_split).max(0.0001);
    cascade_depth * blend_fraction.clamp(0.0, 0.5)
}

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    let t = ((value - edge0) / (edge1 - edge0).max(0.0001)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn cascade_far_split_fade(
    view_depth: f32,
    cascade_index: u32,
    cascade_count: u32,
    cascade_splits: [f32; 4],
    blend_fraction: f32,
) -> f32 {
    if cascade_index + 1 >= cascade_count.clamp(1, SHADOW_CASCADE_MAX) {
        return 0.0;
    }

    let blend_depth = cascade_split_blend_width(cascade_index, cascade_splits, blend_fraction);
    if blend_depth <= 0.0001 {
        return 0.0;
    }

    let split = cascade_splits[cascade_index as usize];
    let half_width = blend_depth * 0.5;
    smoothstep(split - half_width, split + half_width, view_depth)
}

fn cascade_near_split_fade(
    view_depth: f32,
    cascade_index: u32,
    cascade_splits: [f32; 4],
    blend_fraction: f32,
) -> f32 {
    if cascade_index == 0 {
        return 1.0;
    }

    let cascade_before_split = cascade_index - 1;
    let blend_depth =
        cascade_split_blend_width(cascade_before_split, cascade_splits, blend_fraction);
    if blend_depth <= 0.0001 {
        return 1.0;
    }

    let split = cascade_splits[cascade_before_split as usize];
    let half_width = blend_depth * 0.5;
    smoothstep(split - half_width, split + half_width, view_depth)
}

fn wicked_cascade_edge_fade(light_ndc: [f32; 3]) -> f32 {
    let shadow_box = [light_ndc[0], light_ndc[1], light_ndc[2] * 2.0 - 1.0];
    shadow_box
        .into_iter()
        .map(|value| (value.abs().clamp(0.0, 1.0) - 0.8).clamp(0.0, 1.0) * 5.0)
        .fold(0.0, f32::max)
}

fn filter_radius_texels(light_radius: f32) -> f32 {
    if light_radius <= 0.0 {
        0.0
    } else {
        (light_radius * SHADOW_FILTER_RADIUS_SCALE + SHADOW_FILTER_RADIUS_BIAS)
            .clamp(SHADOW_FILTER_RADIUS_BIAS, SHADOW_PCSS_MAX_FILTER_TEXELS)
    }
}

fn wicked_filter_radius_texels(light_radius: f32) -> f32 {
    filter_radius_texels(light_radius)
}

fn pcss_penumbra_scale(average_gap: f32) -> f32 {
    (average_gap.max(0.0) * SHADOW_PCSS_PENUMBRA_SCALE).clamp(0.0, SHADOW_PCSS_MAX_PENUMBRA_SCALE)
}

fn pcss_filter_texels(base_filter_texels: f32, average_gap: f32) -> f32 {
    let penumbra = pcss_penumbra_scale(average_gap);
    (base_filter_texels * penumbra).clamp(
        SHADOW_FILTER_RADIUS_BIAS,
        (base_filter_texels * SHADOW_PCSS_MAX_PENUMBRA_SCALE).min(SHADOW_PCSS_MAX_FILTER_TEXELS),
    )
}

fn receiver_plane_compare_depth(
    center_uv: [f32; 2],
    sample_uv: [f32; 2],
    compare_depth: f32,
    depth_gradient: [f32; 2],
) -> f32 {
    let plane_bias = depth_gradient[0] * (sample_uv[0] - center_uv[0])
        + depth_gradient[1] * (sample_uv[1] - center_uv[1]);
    (compare_depth + plane_bias.clamp(-0.01, 0.01)).clamp(0.0, 1.0)
}

fn find_blocker(
    uv: [f32; 2],
    compare_depth: f32,
    depth_gradient: [f32; 2],
    spread: [f32; 2],
    uv_min: [f32; 2],
    uv_max: [f32; 2],
    atlas_rcp: [f32; 2],
    blocker_depth_bias: f32,
    mut sample_depth: impl FnMut([f32; 2]) -> f32,
) -> PcssBlocker {
    let mut average_receiver_gap = 0.0_f32;
    let mut blocker_count = 0.0_f32;

    for x in -SHADOW_PCSS_BLOCKER_GRID_RADIUS..=SHADOW_PCSS_BLOCKER_GRID_RADIUS {
        for y in -SHADOW_PCSS_BLOCKER_GRID_RADIUS..=SHADOW_PCSS_BLOCKER_GRID_RADIUS {
            let base_offset = [
                x as f32 * SHADOW_PCSS_BLOCKER_GRID_STEP_SCALE * spread[0],
                y as f32 * SHADOW_PCSS_BLOCKER_GRID_STEP_SCALE * spread[1],
            ];
            for tap in 0..4 {
                let tap_offset = [
                    if tap & 1 != 0 { 0.5 } else { -0.5 } * atlas_rcp[0],
                    if tap & 2 != 0 { 0.5 } else { -0.5 } * atlas_rcp[1],
                ];
                let sample_uv = [
                    (uv[0] + base_offset[0] + tap_offset[0]).clamp(uv_min[0], uv_max[0]),
                    (uv[1] + base_offset[1] + tap_offset[1]).clamp(uv_min[1], uv_max[1]),
                ];
                let sample_compare_depth =
                    receiver_plane_compare_depth(uv, sample_uv, compare_depth, depth_gradient);
                let depth = sample_depth(sample_uv);
                let receiver_gap = sample_compare_depth - depth;
                if receiver_gap > blocker_depth_bias {
                    average_receiver_gap += receiver_gap - blocker_depth_bias;
                    blocker_count += 1.0;
                }
            }
        }
    }

    PcssBlocker {
        average_gap: average_receiver_gap / blocker_count.max(1.0),
        count: blocker_count,
    }
}

fn assert_close(actual: f32, expected: f32, epsilon: f32) {
    assert!(
        (actual - expected).abs() <= epsilon,
        "expected {actual} to be within {epsilon} of {expected}"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cascade_edge_fade_matches_wicked_when_blend_width_is_twenty_percent() {
        let cases = [
            ([0.0, 0.0, 0.5], 0.0),
            ([0.79, 0.0, 0.5], 0.0),
            ([0.9, 0.0, 0.5], 0.5),
            ([1.0, 0.0, 0.5], 1.0),
            ([0.0, 0.0, 0.95], 0.5),
        ];

        for (light_ndc, expected) in cases {
            assert_close(cascade_edge_fade(light_ndc, 0.2), expected, 0.0001);
            assert_close(
                cascade_edge_fade(light_ndc, 0.2),
                wicked_cascade_edge_fade(light_ndc),
                0.0001,
            );
        }
    }

    #[test]
    fn cascade_split_fade_uses_centered_overlap_around_depth_boundary() {
        let splits = [5.0, 10.0, 20.0, 40.0];

        assert_close(cascade_far_split_fade(4.49, 0, 4, splits, 0.2), 0.0, 0.0001);
        assert_close(cascade_far_split_fade(5.0, 0, 4, splits, 0.2), 0.5, 0.0001);
        assert_close(cascade_near_split_fade(5.0, 1, splits, 0.2), 0.5, 0.0001);
        assert_close(cascade_near_split_fade(5.51, 1, splits, 0.2), 1.0, 0.0001);
        assert_close(cascade_far_split_fade(9.5, 1, 4, splits, 0.2), 0.0, 0.0001);
        assert_close(cascade_far_split_fade(10.0, 1, 4, splits, 0.2), 0.5, 0.0001);
        assert_close(cascade_far_split_fade(39.0, 3, 4, splits, 0.2), 0.0, 0.0001);
        assert_close(cascade_far_split_fade(5.0, 0, 4, splits, 0.0), 0.0, 0.0001);
    }

    #[test]
    fn cascade_index_advances_only_after_split_boundary() {
        let splits = [5.5, 13.0, 30.0, 80.0];

        assert_eq!(cascade_index(0.1, 4, splits), 0);
        assert_eq!(cascade_index(5.5, 4, splits), 0);
        assert_eq!(cascade_index(5.5001, 4, splits), 1);
        assert_eq!(cascade_index(13.0, 4, splits), 1);
        assert_eq!(cascade_index(13.0001, 4, splits), 2);
        assert_eq!(cascade_index(30.0001, 4, splits), 3);
        assert_eq!(cascade_index(120.0, 4, splits), 3);
    }

    #[test]
    fn cascade_index_clamps_to_available_cascade_count() {
        let splits = [8.0, 24.0, 60.0, 120.0];

        assert_eq!(cascade_index(0.0, 0, splits), 0);
        assert_eq!(cascade_index(100.0, 1, splits), 0);
        assert_eq!(cascade_index(100.0, 2, splits), 1);
        assert_eq!(cascade_index(100.0, 3, splits), 2);
        assert_eq!(cascade_index(100.0, 99, splits), 3);
    }

    #[test]
    fn filter_radius_uses_wicked_radius_remap_and_caps_to_atlas_sane_limit() {
        let cases = [
            (-1.0, 0.0),
            (0.0, 0.0),
            (0.055, 2.44),
            (1.0, 10.0),
            (16.0, 36.0),
        ];

        for (light_radius, expected) in cases {
            assert_close(filter_radius_texels(light_radius), expected, 0.0001);
            assert_close(
                filter_radius_texels(light_radius),
                wicked_filter_radius_texels(light_radius),
                0.0001,
            );
        }
    }

    #[test]
    fn wicked_fixture_table_documents_shadow_sampling_contract() {
        let radius_fixtures = [
            (0.0, 0.0),
            (0.055, 2.44),
            (0.25, 4.0),
            (1.0, 10.0),
            (4.25, 36.0),
            (9.0, 36.0),
        ];
        for (radius, expected_texels) in radius_fixtures {
            assert_close(filter_radius_texels(radius), expected_texels, 0.0001);
            assert_close(wicked_filter_radius_texels(radius), expected_texels, 0.0001);
        }

        let penumbra_fixtures = [
            (-0.01, 0.0),
            (0.0, 0.0),
            (0.00125, 0.25),
            (0.005, 1.0),
            (0.015, 3.0),
            (0.02, 4.0),
            (0.25, 4.0),
        ];
        for (receiver_blocker_gap, expected_penumbra_scale) in penumbra_fixtures {
            assert_close(
                pcss_penumbra_scale(receiver_blocker_gap),
                expected_penumbra_scale,
                0.0001,
            );
        }

        let cascade_fixtures = [
            (0.0, 0),
            (5.5, 0),
            (5.5001, 1),
            (13.0, 1),
            (13.0001, 2),
            (30.0, 2),
            (30.0001, 3),
            (80.0, 3),
            (160.0, 3),
        ];
        for (view_depth, expected_cascade) in cascade_fixtures {
            assert_eq!(
                cascade_index(view_depth, 4, [5.5, 13.0, 30.0, 80.0]),
                expected_cascade
            );
        }

        let edge_fade_fixtures = [
            ([0.0, 0.0, 0.5], 0.0),
            ([0.8, 0.0, 0.5], 0.0),
            ([0.85, 0.0, 0.5], 0.25),
            ([0.9, 0.0, 0.5], 0.5),
            ([0.95, 0.0, 0.5], 0.75),
            ([1.0, 0.0, 0.5], 1.0),
            ([0.0, 0.0, 0.9], 0.0),
            ([0.0, 0.0, 0.95], 0.5),
            ([0.0, 0.0, 1.0], 1.0),
        ];
        for (light_ndc, expected_fade) in edge_fade_fixtures {
            assert_close(cascade_edge_fade(light_ndc, 0.2), expected_fade, 0.0001);
            assert_close(wicked_cascade_edge_fade(light_ndc), expected_fade, 0.0001);
        }
    }

    #[test]
    fn pcss_penumbra_matches_wicked_gap_times_two_hundred_rule() {
        let base = filter_radius_texels(0.055);

        assert_close(pcss_penumbra_scale(-0.1), 0.0, 0.0001);
        assert_close(pcss_penumbra_scale(0.005), 1.0, 0.0001);
        assert_close(pcss_penumbra_scale(0.02), 4.0, 0.0001);
        assert_close(pcss_penumbra_scale(0.2), 4.0, 0.0001);
        assert_close(pcss_filter_texels(base, 0.0), 2.0, 0.0001);
        assert_close(pcss_filter_texels(base, 0.005), base, 0.0001);
        assert_close(pcss_filter_texels(base, 0.2), base * 4.0, 0.0001);
    }

    #[test]
    fn receiver_plane_compare_depth_clamps_bias_and_output() {
        let center = [0.5, 0.5];
        assert_close(
            receiver_plane_compare_depth(center, [0.6, 0.5], 0.4, [0.05, 0.0]),
            0.405,
            0.0001,
        );
        assert_close(
            receiver_plane_compare_depth(center, [1.0, 0.5], 0.4, [10.0, 0.0]),
            0.41,
            0.0001,
        );
        assert_close(
            receiver_plane_compare_depth(center, [0.0, 0.5], 0.005, [10.0, 0.0]),
            0.0,
            0.0001,
        );
    }

    #[test]
    fn compare_depth_applies_receiver_bias_and_clamps_to_shadow_range() {
        let inputs = BiasInputs {
            compare_bias: 0.01,
            texel_world_size: 0.05,
            depth_range: 10.0,
            normal_bias_world: 0.002,
            light_direction: [0.0, -1.0, 0.0],
        };

        assert_close(
            compare_depth(0.42, [0.0, 1.0, 0.0], inputs),
            0.41875,
            0.0001,
        );
        assert_close(compare_depth(0.42, [1.0, 0.0, 0.0], inputs), 0.415, 0.0001);
        assert_close(compare_depth(0.001, [0.0, 1.0, 0.0], inputs), 0.0, 0.0001);
    }

    #[test]
    fn receiver_bias_uses_explicit_normal_bias_or_angle_scaled_texel_bias() {
        let inputs = BiasInputs {
            compare_bias: 0.0,
            texel_world_size: 0.04,
            depth_range: 10.0,
            normal_bias_world: 0.002,
            light_direction: [0.0, -1.0, 0.0],
        };

        assert_close(receiver_bias_world([0.0, 1.0, 0.0], inputs), 0.002, 0.0001);
        assert_close(receiver_bias_world([1.0, 0.0, 0.0], inputs), 0.01, 0.0001);
    }

    #[test]
    fn compare_bias_depth_tracks_texel_pressure_and_normal_bias() {
        let inputs = BiasInputs {
            compare_bias: 0.5,
            texel_world_size: 0.04,
            depth_range: 10.0,
            normal_bias_world: 0.002,
            light_direction: [0.0, -1.0, 0.0],
        };

        assert_close(compare_bias_depth([0.0, 1.0, 0.0], inputs), 0.001, 0.0001);
        assert_close(compare_bias_depth([1.0, 0.0, 0.0], inputs), 0.004, 0.0001);
    }

    #[test]
    fn blocker_search_reports_no_blockers_when_depth_is_behind_receiver() {
        let blocker = find_blocker(
            [0.5, 0.5],
            0.4,
            [0.0, 0.0],
            [0.01, 0.01],
            [0.0, 0.0],
            [1.0, 1.0],
            [1.0 / 2048.0, 1.0 / 2048.0],
            0.0,
            |_| 0.8,
        );

        assert_eq!(
            blocker,
            PcssBlocker {
                average_gap: 0.0,
                count: 0.0
            }
        );
    }

    #[test]
    fn blocker_search_averages_receiver_gap_for_full_occlusion() {
        let blocker = find_blocker(
            [0.5, 0.5],
            0.4,
            [0.0, 0.0],
            [0.01, 0.01],
            [0.0, 0.0],
            [1.0, 1.0],
            [1.0 / 2048.0, 1.0 / 2048.0],
            0.0,
            |_| 0.35,
        );

        assert_close(blocker.count, 100.0, 0.0001);
        assert_close(blocker.average_gap, 0.05, 0.0001);
    }

    #[test]
    fn pcss_blocker_depth_bias_scales_with_cascade_texel_depth() {
        assert_close(pcss_blocker_depth_bias(0.006, 48.0), 0.00075, 0.000001);
        assert_close(pcss_blocker_depth_bias(0.012, 96.0), 0.00075, 0.000001);
    }

    #[test]
    fn blocker_search_ignores_receiver_self_gaps_below_blocker_bias() {
        let blocker = find_blocker(
            [0.5, 0.5],
            0.4,
            [0.0, 0.0],
            [0.01, 0.01],
            [0.0, 0.0],
            [1.0, 1.0],
            [1.0 / 2048.0, 1.0 / 2048.0],
            0.01,
            |_| 0.395,
        );

        assert_eq!(
            blocker,
            PcssBlocker {
                average_gap: 0.0,
                count: 0.0
            }
        );
    }

    #[test]
    fn blocker_search_keeps_true_blockers_after_blocker_bias() {
        let blocker = find_blocker(
            [0.5, 0.5],
            0.4,
            [0.0, 0.0],
            [0.01, 0.01],
            [0.0, 0.0],
            [1.0, 1.0],
            [1.0 / 2048.0, 1.0 / 2048.0],
            0.01,
            |_| 0.35,
        );

        assert_close(blocker.count, 100.0, 0.0001);
        assert_close(blocker.average_gap, 0.04, 0.0001);
    }
}
