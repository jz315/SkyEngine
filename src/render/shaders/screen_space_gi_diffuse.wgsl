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

@group(0) @binding(0)
var<uniform> gi: ScreenSpaceGiUniform;

@group(1) @binding(0)
var radiance_tex: texture_2d<f32>;
@group(1) @binding(1)
var geometry_tex: texture_2d<f32>;

@group(2) @binding(0)
var diffuse_out0: texture_storage_2d<rgba16float, write>;
@group(2) @binding(1)
var diffuse_out1: texture_storage_2d<rgba16float, write>;
@group(2) @binding(2)
var diffuse_out2: texture_storage_2d<rgba16float, write>;
@group(2) @binding(3)
var diffuse_out3: texture_storage_2d<rgba16float, write>;

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

fn luminance(color: vec3<f32>) -> f32 {
    return dot(color, vec3<f32>(0.2126, 0.7152, 0.0722));
}

fn decode_scene_normal(encoded: vec4<f32>) -> vec3<f32> {
    return safe_normalize(encoded.xyz * 2.0 - vec3<f32>(1.0, 1.0, 1.0));
}

fn hemisphere_ambient(view_normal: vec3<f32>) -> vec3<f32> {
    let world_normal = safe_normalize((gi.inverse_view * vec4<f32>(view_normal, 0.0)).xyz);
    let hemi = saturate(world_normal.y * 0.5 + 0.5);
    return mix(gi.ground_color.rgb, gi.sky_color.rgb, hemi);
}

fn hash12(p: vec2<f32>) -> f32 {
    let h = dot(p, vec2<f32>(127.1, 311.7));
    return fract(sin(h) * 43758.5453123);
}

fn rotated_sample_dir(index: i32, rotation: f32) -> vec2<f32> {
    let dir = SAMPLE_DIRS[index];
    let c = cos(rotation);
    let s = sin(rotation);
    return vec2<f32>(
        dir.x * c - dir.y * s,
        dir.x * s + dir.y * c,
    );
}

fn mip_output_size(level: i32) -> vec2<i32> {
    if (level == 0) {
        return vec2<i32>(gi.history_viewport.xy);
    }
    if (level == 1) {
        return vec2<i32>(gi.ultra_viewport.xy);
    }
    if (level == 2) {
        return vec2<i32>(gi.super_viewport.xy);
    }
    return vec2<i32>(gi.hyper_viewport.xy);
}

fn mip_texel(level: i32) -> vec2<f32> {
    let size = mip_output_size(level);
    return vec2<f32>(
        1.0 / max(1.0, f32(size.x)),
        1.0 / max(1.0, f32(size.y)),
    );
}

fn clamp_mip_pixel(level: i32, pixel: vec2<i32>) -> vec2<i32> {
    let size = mip_output_size(level);
    let max_pixel = vec2<i32>(max(size - vec2<i32>(1, 1), vec2<i32>(0, 0)));
    return clamp(pixel, vec2<i32>(0, 0), max_pixel);
}

fn mip_uv_from_pixel(size: vec2<i32>, pixel: vec2<i32>) -> vec2<f32> {
    return (vec2<f32>(pixel) + vec2<f32>(0.5, 0.5)) / vec2<f32>(size);
}

fn mip_pixel_from_uv(level: i32, uv: vec2<f32>) -> vec2<i32> {
    let size = mip_output_size(level);
    let pixel = vec2<i32>(clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0)) * vec2<f32>(size));
    return clamp_mip_pixel(level, pixel);
}

fn geometry_at(level: i32, pixel: vec2<i32>) -> vec4<f32> {
    return textureLoad(geometry_tex, clamp_mip_pixel(level, pixel), level);
}

fn radiance_at(level: i32, pixel: vec2<i32>) -> vec3<f32> {
    return textureLoad(radiance_tex, clamp_mip_pixel(level, pixel), level).rgb;
}

fn depth_at(level: i32, pixel: vec2<i32>) -> f32 {
    return geometry_at(level, pixel).a;
}

fn normal_at(level: i32, pixel: vec2<i32>) -> vec3<f32> {
    return decode_scene_normal(geometry_at(level, pixel));
}

fn depth_at_uv(level: i32, uv: vec2<f32>) -> f32 {
    return depth_at(level, mip_pixel_from_uv(level, uv));
}

fn normal_at_uv(level: i32, uv: vec2<f32>) -> vec3<f32> {
    return normal_at(level, mip_pixel_from_uv(level, uv));
}

fn radiance_at_uv(level: i32, uv: vec2<f32>) -> vec3<f32> {
    return radiance_at(level, mip_pixel_from_uv(level, uv));
}

fn reconstruct_view_position(uv: vec2<f32>, depth: f32) -> vec3<f32> {
    let ndc = vec4<f32>(uv.x * 2.0 - 1.0, (1.0 - uv.y) * 2.0 - 1.0, depth, 1.0);
    let view = gi.inverse_projection * ndc;
    return view.xyz / max(view.w, 0.00001);
}

fn screen_segment_visibility(
    level: i32,
    start_uv: vec2<f32>,
    end_uv: vec2<f32>,
    start_depth: f32,
    end_depth: f32,
) -> f32 {
    let size = vec2<f32>(mip_output_size(level));
    let pixel_dist = length((end_uv - start_uv) * size);
    let steps = clamp(i32(pixel_dist * 0.35), 2, 8);
    let bias = 0.0012 + abs(end_depth - start_depth) * 0.2;

    for (var i: i32 = 1; i < steps; i = i + 1) {
        let t = f32(i) / f32(steps);
        let probe_uv = mix(start_uv, end_uv, t);
        let probe_depth = depth_at_uv(level, probe_uv);
        let expected_depth = mix(start_depth, end_depth, t);
        if (probe_depth + bias < expected_depth) {
            return 0.0;
        }
    }

    return 1.0;
}

fn diffuse_value_for_level(pixel: vec2<i32>, level: i32) -> vec4<f32> {
    let center_depth = depth_at(level, pixel);
    if (center_depth >= 0.99999) {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }

    let size = mip_output_size(level);
    let uv = mip_uv_from_pixel(size, pixel);
    let center_pos = reconstruct_view_position(uv, center_depth);
    let center_normal = normal_at(level, pixel);
    let rotation =
        hash12(uv * vec2<f32>(size) + vec2<f32>(17.0 + f32(level) * 29.0, 31.0)) * 6.28318530718;
    let texel = mip_texel(level);

    var accum = hemisphere_ambient(center_normal) * (0.25 + f32(level) * 0.05);
    var weight_sum = 0.3;
    var ring_count = 3;
    if (level >= 2) {
        ring_count = 2;
    }

    for (var dir_index: i32 = 0; dir_index < 8; dir_index = dir_index + 1) {
        let dir = rotated_sample_dir(dir_index, rotation);
        for (var ring_index: i32 = 0; ring_index < ring_count; ring_index = ring_index + 1) {
            let ring_scale = 0.45 + f32(ring_index) * 0.4 + f32(level) * 0.35;
            let sample_uv = uv + dir * texel * gi.params0.y * ring_scale;
            let sample_depth = depth_at_uv(level, sample_uv);
            if (sample_depth >= 0.99999) {
                continue;
            }

            let sample_pos = reconstruct_view_position(sample_uv, sample_depth);
            let delta = sample_pos - center_pos;
            let dist = length(delta);
            if (dist <= 0.0001) {
                continue;
            }

            let transfer_dir = delta / dist;
            let sample_normal = normal_at_uv(level, sample_uv);
            let facing = saturate(dot(center_normal, transfer_dir))
                * saturate(dot(sample_normal, -transfer_dir));
            if (facing <= 0.0001) {
                continue;
            }

            let normal_similarity = pow(
                saturate(dot(center_normal, sample_normal)),
                gi.params0.w * (0.85 + f32(level) * 0.15),
            );
            let plane_distance = abs(dot(center_normal, delta));
            let thickness_weight = exp(-plane_distance * gi.params0.z * (0.9 + f32(level) * 0.25));
            let distance_weight = 1.0 / (1.0 + dist * dist * gi.params1.x * (0.8 + f32(level) * 0.2));
            let visibility =
                screen_segment_visibility(level, uv, sample_uv, center_depth, sample_depth);
            let weight =
                facing * normal_similarity * thickness_weight * distance_weight * visibility;
            if (weight <= 0.0001) {
                continue;
            }

            let energy_cap = max(2.5, 6.0 - f32(level));
            let sample_radiance = min(radiance_at_uv(level, sample_uv), vec3<f32>(energy_cap));
            let energy = 0.35 + min(1.5, luminance(sample_radiance)) * (0.8 + f32(level) * 0.1);
            accum += sample_radiance * energy * weight;
            weight_sum += weight;
        }
    }

    let indirect = max((accum / max(weight_sum, 0.0001)) * gi.params0.x, vec3<f32>(0.0));
    let variance = clamp(1.0 / max(weight_sum * 0.6, 1.0), 0.02, 1.0);
    return vec4<f32>(indirect, variance);
}

fn store_diffuse(level: i32, pixel: vec2<i32>, value: vec4<f32>) {
    if (level == 0) {
        textureStore(diffuse_out0, pixel, value);
        return;
    }
    if (level == 1) {
        textureStore(diffuse_out1, pixel, value);
        return;
    }
    if (level == 2) {
        textureStore(diffuse_out2, pixel, value);
        return;
    }
    textureStore(diffuse_out3, pixel, value);
}

@compute @workgroup_size(8, 8, 1)
fn cs_build_diffuse_mips(@builtin(global_invocation_id) gid: vec3<u32>) {
    let pixel = vec2<i32>(gid.xy);
    for (var level: i32 = 0; level < 4; level = level + 1) {
        let size = mip_output_size(level);
        if (pixel.x < size.x && pixel.y < size.y) {
            store_diffuse(level, pixel, diffuse_value_for_level(pixel, level));
        }
    }
}
