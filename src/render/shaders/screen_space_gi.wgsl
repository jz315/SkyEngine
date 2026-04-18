@group(0) @binding(0)
var depth_tex: texture_depth_2d;
@group(0) @binding(1)
var normal_tex: texture_2d<f32>;
@group(0) @binding(2)
var albedo_tex: texture_2d<f32>;
@group(0) @binding(3)
var material_tex: texture_2d<f32>;

struct ScreenSpaceGiUniform {
    inverse_projection: mat4x4<f32>,
    inverse_view: mat4x4<f32>,
    prev_view_proj: mat4x4<f32>,
    viewport: vec4<f32>,
    history_viewport: vec4<f32>,
    ultra_viewport: vec4<f32>,
    super_viewport: vec4<f32>,
    hyper_viewport: vec4<f32>,
    sky_color: vec4<f32>,
    ground_color: vec4<f32>,
    params0: vec4<f32>,
    params1: vec4<f32>,
    params2: vec4<f32>,
};

@group(1) @binding(0)
var<uniform> gi: ScreenSpaceGiUniform;

@group(2) @binding(0)
var primary_tex: texture_2d<f32>;
@group(2) @binding(1)
var secondary_tex: texture_2d<f32>;
@group(2) @binding(2)
var primary_sampler: sampler;

@group(3) @binding(0)
var aux_tex: texture_2d<f32>;
@group(3) @binding(1)
var aux2_tex: texture_2d<f32>;
@group(3) @binding(2)
var aux_sampler: sampler;

const SAMPLE_DIRS: array<vec2<f32>, 8> = array<vec2<f32>, 8>(
    vec2<f32>(1.0, 0.0),
    vec2<f32>(-1.0, 0.0),
    vec2<f32>(0.0, 1.0),
    vec2<f32>(0.0, -1.0),
    vec2<f32>(0.70710677, 0.70710677),
    vec2<f32>(-0.70710677, 0.70710677),
    vec2<f32>(0.70710677, -0.70710677),
    vec2<f32>(-0.70710677, -0.70710677)
);

fn saturate(v: f32) -> f32 {
    return clamp(v, 0.0, 1.0);
}

fn safe_normalize(v: vec3<f32>) -> vec3<f32> {
    let len_sq = dot(v, v);
    if (len_sq <= 0.000001) {
        return vec3<f32>(0.0, 0.0, 1.0);
    }
    return v * inverseSqrt(len_sq);
}

fn in_bounds(uv: vec2<f32>) -> bool {
    return all(uv >= vec2<f32>(0.0)) && all(uv <= vec2<f32>(1.0));
}

fn primary_at_uv(uv: vec2<f32>) -> vec4<f32> {
    return textureSampleLevel(primary_tex, primary_sampler, clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0);
}

fn aux_at_uv(uv: vec2<f32>) -> vec4<f32> {
    return textureSampleLevel(
        aux_tex,
        aux_sampler,
        clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0)),
        0.0,
    );
}

fn aux2_at_uv(uv: vec2<f32>) -> vec4<f32> {
    return textureSampleLevel(
        aux2_tex,
        aux_sampler,
        clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0)),
        0.0,
    );
}

fn scene_pixel_from_uv(uv: vec2<f32>) -> vec2<i32> {
    let clamped = clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0));
    let pixel = vec2<i32>(clamped * gi.viewport.xy);
    let max_pixel = vec2<i32>(max(gi.viewport.xy - vec2<f32>(1.0), vec2<f32>(0.0)));
    return clamp(pixel, vec2<i32>(0), max_pixel);
}

fn decode_scene_normal(encoded: vec4<f32>) -> vec3<f32> {
    return safe_normalize(encoded.xyz * 2.0 - vec3<f32>(1.0, 1.0, 1.0));
}

fn scene_normal_at_uv(uv: vec2<f32>) -> vec3<f32> {
    let pixel = scene_pixel_from_uv(uv);
    return decode_scene_normal(textureLoad(normal_tex, pixel, 0));
}

fn composite_normal_at_uv(uv: vec2<f32>) -> vec3<f32> {
    return decode_scene_normal(aux_at_uv(uv));
}

fn composite_depth_at_uv(uv: vec2<f32>) -> f32 {
    return aux_at_uv(uv).a;
}

fn depth_at_uv(uv: vec2<f32>) -> f32 {
    let clamped = clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0));
    let pixel = vec2<i32>(clamped * gi.viewport.xy);
    let max_pixel = vec2<i32>(max(gi.viewport.xy - vec2<f32>(1.0), vec2<f32>(0.0)));
    return textureLoad(depth_tex, clamp(pixel, vec2<i32>(0), max_pixel), 0);
}

fn reconstruct_view_position(uv: vec2<f32>, depth: f32) -> vec3<f32> {
    let ndc = vec4<f32>(uv.x * 2.0 - 1.0, (1.0 - uv.y) * 2.0 - 1.0, depth, 1.0);
    let view = gi.inverse_projection * ndc;
    return view.xyz / max(view.w, 0.00001);
}

fn reconstruct_view_normal(uv: vec2<f32>, center_pos: vec3<f32>, center_depth: f32) -> vec3<f32> {
    let texel = gi.viewport.zw;
    let uv_r = clamp(uv + vec2<f32>(texel.x, 0.0), vec2<f32>(0.0), vec2<f32>(1.0));
    let uv_l = clamp(uv - vec2<f32>(texel.x, 0.0), vec2<f32>(0.0), vec2<f32>(1.0));
    let uv_u = clamp(uv - vec2<f32>(0.0, texel.y), vec2<f32>(0.0), vec2<f32>(1.0));
    let uv_d = clamp(uv + vec2<f32>(0.0, texel.y), vec2<f32>(0.0), vec2<f32>(1.0));

    let pos_r = reconstruct_view_position(uv_r, depth_at_uv(uv_r));
    let pos_l = reconstruct_view_position(uv_l, depth_at_uv(uv_l));
    let pos_u = reconstruct_view_position(uv_u, depth_at_uv(uv_u));
    let pos_d = reconstruct_view_position(uv_d, depth_at_uv(uv_d));

    let dx_r = pos_r - center_pos;
    let dx_l = center_pos - pos_l;
    let dy_u = pos_u - center_pos;
    let dy_d = center_pos - pos_d;

    var dx = dx_l;
    if (length(dx_r) < length(dx_l)) {
        dx = dx_r;
    }
    var dy = dy_d;
    if (length(dy_u) < length(dy_d)) {
        dy = dy_u;
    }

    let normal = safe_normalize(cross(dx, dy));
    if (center_depth >= 0.99999) {
        return vec3<f32>(0.0, 0.0, 1.0);
    }
    return normal;
}

fn luminance(color: vec3<f32>) -> f32 {
    return dot(color, vec3<f32>(0.2126, 0.7152, 0.0722));
}

fn ambient_occlusion(
    uv: vec2<f32>,
    center_pos: vec3<f32>,
    center_normal: vec3<f32>,
    center_depth: f32,
) -> f32 {
    var occlusion = 0.0;
    var weight_sum = 0.0;

    for (var dir_index: i32 = 0; dir_index < 8; dir_index = dir_index + 1) {
        let dir = SAMPLE_DIRS[dir_index];
        for (var ring_index: i32 = 0; ring_index < 2; ring_index = ring_index + 1) {
            let ring_scale = 1.25 + f32(ring_index) * 1.5;
            let sample_uv = uv + dir * gi.viewport.zw * ring_scale * 3.0;
            if (!in_bounds(sample_uv)) {
                continue;
            }

            let sample_depth = depth_at_uv(sample_uv);
            if (sample_depth >= 0.99999) {
                continue;
            }

            let sample_pos = reconstruct_view_position(sample_uv, sample_depth);
            let delta = sample_pos - center_pos;
            let dist_sq = max(dot(delta, delta), 0.0001);
            let delta_dir = delta * inverseSqrt(dist_sq);
            let horizon = 1.0 - saturate(dot(center_normal, delta_dir));
            let plane_distance = abs(dot(center_normal, delta));
            let range_weight = 1.0 / (1.0 + dist_sq * 18.0);
            let thickness_weight = exp(-plane_distance * 12.0);
            let depth_weight = exp(-abs(sample_depth - center_depth) * 80.0);
            let weight = range_weight * thickness_weight * depth_weight;

            occlusion += horizon * weight;
            weight_sum += weight;
        }
    }

    let ao = 1.0 - saturate((occlusion / max(weight_sum, 0.0001)) * 0.85);
    return ao;
}

fn history_max_pixel() -> vec2<i32> {
    return vec2<i32>(max(gi.history_viewport.xy - vec2<f32>(1.0), vec2<f32>(0.0)));
}

fn history_pixel_from_uv(uv: vec2<f32>) -> vec2<i32> {
    let pixel = vec2<i32>(uv * gi.history_viewport.xy);
    return clamp(pixel, vec2<i32>(0), history_max_pixel());
}

fn history_uv_from_pixel(pixel: vec2<i32>) -> vec2<f32> {
    let clamped = clamp(pixel, vec2<i32>(0), history_max_pixel());
    return (vec2<f32>(clamped) + vec2<f32>(0.5)) * gi.history_viewport.zw;
}

fn history_load(pixel: vec2<i32>) -> vec4<f32> {
    let clamped = clamp(pixel, vec2<i32>(0), history_max_pixel());
    return textureLoad(secondary_tex, clamped, 0);
}

fn upsample_low_res_gi(
    uv: vec2<f32>,
    center_pos: vec3<f32>,
    center_normal: vec3<f32>,
) -> vec3<f32> {
    let base_pixel = history_pixel_from_uv(uv);
    let history_pos = uv * gi.history_viewport.xy;
    let kernel_range = clamp(i32(round(gi.params2.x)), 1, 2);
    var accum = vec3<f32>(0.0, 0.0, 0.0);
    var weight_sum = 0.0;
    var best_color = history_load(base_pixel).rgb;
    var best_weight = -1.0;

    for (var y: i32 = -kernel_range; y <= kernel_range; y = y + 1) {
        for (var x: i32 = -kernel_range; x <= kernel_range; x = x + 1) {
            let sample_pixel = base_pixel + vec2<i32>(x, y);
            let sample_uv = history_uv_from_pixel(sample_pixel);
            let sample_depth = composite_depth_at_uv(sample_uv);
            if (sample_depth >= 0.99999) {
                continue;
            }

            let sample_pos = reconstruct_view_position(sample_uv, sample_depth);
            let sample_normal = composite_normal_at_uv(sample_uv);
            let sample_gi = history_load(sample_pixel).rgb;
            let sample_center = vec2<f32>(sample_pixel) + vec2<f32>(0.5);
            let kernel_delta = sample_center - history_pos;
            let kernel_weight = exp(-dot(kernel_delta, kernel_delta) * 0.8);
            let depth_delta = abs(sample_pos.z - center_pos.z);
            let plane_delta = abs(dot(center_normal, sample_pos - center_pos));
            let depth_weight = exp(-depth_delta * gi.params2.y);
            let plane_weight = exp(-plane_delta * gi.params2.y * 1.8);
            let normal_weight = pow(
                saturate(dot(center_normal, sample_normal)),
                gi.params2.z,
            ) + 0.001;
            let weight = kernel_weight * depth_weight * plane_weight * normal_weight;

            if (weight > best_weight) {
                best_weight = weight;
                best_color = sample_gi;
            }

            accum += sample_gi * weight;
            weight_sum += weight;
        }
    }

    if (weight_sum <= 0.0001) {
        return best_color;
    }
    return accum / weight_sum;
}

@fragment
fn fs_composite(in: FullscreenOutput) -> @location(0) vec4<f32> {
    let uv = in.uv;
    let scene = primary_at_uv(uv);
    let center_depth = depth_at_uv(uv);
    if (center_depth >= 0.99999) {
        return scene;
    }

    let center_pos = reconstruct_view_position(uv, center_depth);
    let center_normal = scene_normal_at_uv(uv);
    let filtered_gi = upsample_low_res_gi(uv, center_pos, center_normal);
    let ao = ambient_occlusion(uv, center_pos, center_normal, center_depth);
    let scene_luma = luminance(scene.rgb);
    let shadow_lift = 0.35 + 0.65 / (1.0 + scene_luma * 1.25);
    let gi_visibility = 1.0 - smoothstep(0.85, 2.0, scene_luma);
    let receiver_albedo = aux2_at_uv(uv).rgb;
    let scene_with_ao = scene.rgb * mix(0.55, 1.0, ao);
    let gi_irradiance = filtered_gi * shadow_lift * gi_visibility * mix(0.45, 1.0, ao);
    let gi_term = gi_irradiance * receiver_albedo;
    return vec4<f32>(scene_with_ao + gi_term, scene.a);
}
