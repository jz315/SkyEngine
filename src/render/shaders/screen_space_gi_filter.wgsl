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
@group(3) @binding(3)
var output_tex: texture_storage_2d<rgba16float, write>;

struct ColorStats {
    mean: vec3<f32>,
    stddev: vec3<f32>,
    luma_variance: f32,
};

struct HistorySample {
    color: vec4<f32>,
    validity: f32,
};

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

fn luminance(color: vec3<f32>) -> f32 {
    return dot(color, vec3<f32>(0.2126, 0.7152, 0.0722));
}

fn primary_at_uv(uv: vec2<f32>) -> vec4<f32> {
    return textureSampleLevel(
        primary_tex,
        primary_sampler,
        clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0)),
        0.0,
    );
}

fn secondary_at_uv(uv: vec2<f32>) -> vec4<f32> {
    return textureSampleLevel(
        secondary_tex,
        primary_sampler,
        clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0)),
        0.0,
    );
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

fn scene_albedo_at_uv(uv: vec2<f32>) -> vec3<f32> {
    return textureLoad(albedo_tex, scene_pixel_from_uv(uv), 0).rgb;
}

fn scene_material_at_uv(uv: vec2<f32>) -> vec4<f32> {
    return textureLoad(material_tex, scene_pixel_from_uv(uv), 0);
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

fn reconstruct_world_position(uv: vec2<f32>, depth: f32) -> vec3<f32> {
    let view = reconstruct_view_position(uv, depth);
    let world = gi.inverse_view * vec4<f32>(view, 1.0);
    return world.xyz / max(world.w, 0.00001);
}

fn current_color_stats(uv: vec2<f32>) -> ColorStats {
    let texel = gi.history_viewport.zw;
    var mean = vec3<f32>(0.0, 0.0, 0.0);
    var mean2 = vec3<f32>(0.0, 0.0, 0.0);
    var mean_luma = 0.0;
    var mean_luma2 = 0.0;
    var weight_sum = 0.0;

    for (var y: i32 = -1; y <= 1; y = y + 1) {
        for (var x: i32 = -1; x <= 1; x = x + 1) {
            let sample_uv = clamp(
                uv + vec2<f32>(f32(x), f32(y)) * texel,
                vec2<f32>(0.0),
                vec2<f32>(1.0),
            );
            let sample_color = primary_at_uv(sample_uv).rgb;
            let weight = exp(-0.65 * f32(x * x + y * y));
            let sample_luma = luminance(sample_color);
            mean += sample_color * weight;
            mean2 += sample_color * sample_color * weight;
            mean_luma += sample_luma * weight;
            mean_luma2 += sample_luma * sample_luma * weight;
            weight_sum += weight;
        }
    }

    mean /= max(weight_sum, 0.0001);
    mean2 /= max(weight_sum, 0.0001);
    mean_luma /= max(weight_sum, 0.0001);
    mean_luma2 /= max(weight_sum, 0.0001);
    let variance = max(mean2 - mean * mean, vec3<f32>(0.0, 0.0, 0.0));
    let luma_variance = max(mean_luma2 - mean_luma * mean_luma, 0.0);
    return ColorStats(mean, sqrt(variance), luma_variance);
}

fn history_depth_validity(reprojected_depth: f32, history_depth: f32) -> f32 {
    let depth_delta = abs(reprojected_depth - history_depth);
    return exp(-depth_delta * gi.params1.w * 24.0);
}

fn sample_history(
    prev_uv: vec2<f32>,
    reprojected_depth: f32,
    current_color: vec3<f32>,
) -> HistorySample {
    let texel = gi.history_viewport.zw;
    var best = aux_at_uv(prev_uv);
    var best_validity = history_depth_validity(reprojected_depth, aux2_at_uv(prev_uv).a);

    for (var y: i32 = -1; y <= 1; y = y + 1) {
        for (var x: i32 = -1; x <= 1; x = x + 1) {
            let sample_uv = clamp(
                prev_uv + vec2<f32>(f32(x), f32(y)) * texel,
                vec2<f32>(0.0),
                vec2<f32>(1.0),
            );
            let sample_color = aux_at_uv(sample_uv);
            let sample_geometry = aux2_at_uv(sample_uv);
            let validity = history_depth_validity(reprojected_depth, sample_geometry.a);
            if (validity > best_validity) {
                best = sample_color;
                best_validity = validity;
            }
        }
    }

    let luma_delta = abs(luminance(best.rgb) - luminance(current_color));
    let color_validity = exp(-luma_delta * 2.5);
    return HistorySample(best, best_validity * color_validity);
}

fn resolve_depth_at_uv(uv: vec2<f32>) -> f32 {
    return secondary_at_uv(uv).a;
}

fn resolve_normal_at_uv(uv: vec2<f32>) -> vec3<f32> {
    return decode_scene_normal(secondary_at_uv(uv));
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
    return textureLoad(primary_tex, clamped, 0);
}

fn surface_similarity(center_uv: vec2<f32>, sample_uv: vec2<f32>) -> f32 {
    let center_albedo = scene_albedo_at_uv(center_uv);
    let sample_albedo = scene_albedo_at_uv(sample_uv);
    let center_material = scene_material_at_uv(center_uv);
    let sample_material = scene_material_at_uv(sample_uv);
    let albedo_delta = length(sample_albedo - center_albedo);
    let material_delta =
        abs(sample_material.x - center_material.x)
        + abs(sample_material.y - center_material.y) * 0.75
        + abs(sample_material.z - center_material.z) * 1.25;
    return exp(-albedo_delta * 3.5 - material_delta * 4.5);
}

fn compress_radiance(color: vec3<f32>) -> vec3<f32> {
    return color / (1.0 + luminance(color));
}

fn decompress_radiance(color: vec3<f32>) -> vec3<f32> {
    return color / max(1.0 - luminance(color), 0.0001);
}

fn spatial_filter_low_res_gi(uv: vec2<f32>) -> vec4<f32> {
    let center = primary_at_uv(uv);
    let center_depth = resolve_depth_at_uv(uv);
    if (center_depth >= 0.99999) {
        return center;
    }

    let center_normal = resolve_normal_at_uv(uv);
    let texel = gi.history_viewport.zw * max(gi.params2.x, 1.0);
    var accum = compress_radiance(center.rgb);
    var accum_variance = center.a;
    var weight_sum = 1.0;

    for (var y: i32 = -1; y <= 1; y = y + 1) {
        for (var x: i32 = -1; x <= 1; x = x + 1) {
            if (x == 0 && y == 0) {
                continue;
            }

            let sample_uv = clamp(
                uv + vec2<f32>(f32(x), f32(y)) * texel,
                vec2<f32>(0.0),
                vec2<f32>(1.0),
            );
            let sample_depth = resolve_depth_at_uv(sample_uv);
            if (sample_depth >= 0.99999) {
                continue;
            }

            let sample_normal = resolve_normal_at_uv(sample_uv);
            let kernel_weight = exp(-0.7 * f32(x * x + y * y));
            let depth_weight = exp(-abs(sample_depth - center_depth) * gi.params2.y * 24.0);
            let normal_weight = pow(
                saturate(dot(center_normal, sample_normal)),
                gi.params2.z,
            ) + 0.001;
            let sample = primary_at_uv(sample_uv);
            let variance_weight = 1.0 / (1.0 + sample.a * 12.0);
            let surface_weight = surface_similarity(uv, sample_uv);
            let weight =
                kernel_weight * depth_weight * normal_weight * variance_weight * surface_weight;
            accum += compress_radiance(sample.rgb) * weight;
            accum_variance += sample.a * weight;
            weight_sum += weight;
        }
    }

    return vec4<f32>(
        max(decompress_radiance(accum / weight_sum), vec3<f32>(0.0)),
        accum_variance / weight_sum,
    );
}

fn pixel_to_uv(pixel: vec2<i32>) -> vec2<f32> {
    return (vec2<f32>(pixel) + vec2<f32>(0.5, 0.5)) * gi.history_viewport.zw;
}

@compute @workgroup_size(8, 8, 1)
fn cs_temporal(@builtin(global_invocation_id) gid: vec3<u32>) {
    let pixel = vec2<i32>(gid.xy);
    let size = vec2<i32>(gi.history_viewport.xy);
    if (pixel.x >= size.x || pixel.y >= size.y) {
        return;
    }

    let uv = pixel_to_uv(pixel);
    let current = primary_at_uv(uv);
    let center_depth = depth_at_uv(uv);
    var result = vec4<f32>(current.rgb, clamp(current.a, 0.001, 1.0));

    if (center_depth < 0.99999 && gi.params2.w >= 0.5) {
        let world_position = reconstruct_world_position(uv, center_depth);
        let prev_clip = gi.prev_view_proj * vec4<f32>(world_position, 1.0);
        if (abs(prev_clip.w) > 0.00001) {
            let prev_ndc = prev_clip.xyz / prev_clip.w;
            let prev_uv_camera = vec2<f32>(prev_ndc.x * 0.5 + 0.5, prev_ndc.y * -0.5 + 0.5);
            let velocity = secondary_at_uv(uv);
            let prev_uv_motion = uv + velocity.xy;

            let stats = current_color_stats(uv);
            let current_variance = clamp(stats.luma_variance * 1.5 + current.a, 0.001, 1.0);

            var history = HistorySample(vec4<f32>(current.rgb, current_variance), 0.0);
            if (in_bounds(prev_uv_camera) && prev_ndc.z > 0.0 && prev_ndc.z < 1.0) {
                history = sample_history(prev_uv_camera, prev_ndc.z, stats.mean);
            }
            if (in_bounds(prev_uv_motion) && velocity.z > 0.0 && velocity.z < 1.0) {
                let motion_history = sample_history(prev_uv_motion, velocity.z, stats.mean);
                if (motion_history.validity > history.validity) {
                    history = motion_history;
                }
            }

            let clamp_min = stats.mean - stats.stddev * 0.9;
            let clamp_max = stats.mean + stats.stddev * 0.9;
            let clamped_history = clamp(history.color.rgb, clamp_min, clamp_max);
            let motion_pixels = length(velocity.xy * gi.viewport.xy);
            let motion_rejection = exp(-motion_pixels * 0.12);
            let history_rejection = smoothstep(0.18, 0.55, history.validity);
            let blend = gi.params1.z * history.validity * motion_rejection * history_rejection;
            let color = mix(current.rgb, clamped_history, saturate(blend));
            let variance = mix(
                select(1.0, current_variance, history.validity > 0.4),
                clamp(history.color.a, 0.001, 1.0),
                saturate(blend * 0.65),
            );
            result = vec4<f32>(color, clamp(variance, 0.001, 1.0));
        } else {
            result = vec4<f32>(current.rgb, center_depth);
        }
    }

    textureStore(output_tex, pixel, result);
}

@compute @workgroup_size(8, 8, 1)
fn cs_spatial(@builtin(global_invocation_id) gid: vec3<u32>) {
    let pixel = vec2<i32>(gid.xy);
    let size = vec2<i32>(gi.history_viewport.xy);
    if (pixel.x >= size.x || pixel.y >= size.y) {
        return;
    }

    textureStore(output_tex, pixel, spatial_filter_low_res_gi(pixel_to_uv(pixel)));
}
