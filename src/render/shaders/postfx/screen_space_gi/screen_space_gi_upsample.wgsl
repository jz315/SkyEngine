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
var low_geometry_tex: texture_2d<f32>;
@group(1) @binding(1)
var low_diffuse_tex: texture_2d<f32>;
@group(1) @binding(2)
var high_geometry_tex: texture_2d<f32>;
@group(1) @binding(3)
var high_diffuse_tex: texture_2d<f32>;

@group(2) @binding(0)
var output_tex: texture_storage_2d<rgba16float, write>;

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

fn decode_scene_normal(encoded: vec4<f32>) -> vec3<f32> {
    return safe_normalize(encoded.xyz * 2.0 - vec3<f32>(1.0, 1.0, 1.0));
}

fn texture_size(tex: texture_2d<f32>) -> vec2<i32> {
    return vec2<i32>(textureDimensions(tex, 0));
}

fn clamp_pixel(size: vec2<i32>, pixel: vec2<i32>) -> vec2<i32> {
    let max_pixel = vec2<i32>(max(size - vec2<i32>(1, 1), vec2<i32>(0, 0)));
    return clamp(pixel, vec2<i32>(0, 0), max_pixel);
}

fn uv_from_pixel(size: vec2<i32>, pixel: vec2<i32>) -> vec2<f32> {
    return (vec2<f32>(pixel) + vec2<f32>(0.5, 0.5)) / vec2<f32>(size);
}

fn reconstruct_view_position(uv: vec2<f32>, depth: f32) -> vec3<f32> {
    let ndc = vec4<f32>(uv.x * 2.0 - 1.0, (1.0 - uv.y) * 2.0 - 1.0, depth, 1.0);
    let view = gi.inverse_projection * ndc;
    return view.xyz / max(view.w, 0.00001);
}

fn upsample_pixel(pixel: vec2<i32>, range: i32, spread: i32, contribution: f32) -> vec4<f32> {
    let high_size = texture_size(high_geometry_tex);
    let low_size = texture_size(low_geometry_tex);
    let high_pixel = clamp_pixel(high_size, pixel);
    let high_geometry = textureLoad(high_geometry_tex, high_pixel, 0);
    let high_base = textureLoad(high_diffuse_tex, high_pixel, 0);
    let high_depth = high_geometry.a;

    if (high_depth >= 0.99999) {
        return high_base;
    }

    let high_uv = uv_from_pixel(high_size, high_pixel);
    let high_normal = decode_scene_normal(high_geometry);
    let high_pos = reconstruct_view_position(high_uv, high_depth);
    let low_center = clamp_pixel(low_size, vec2<i32>(high_uv * vec2<f32>(low_size)));

    var accum = high_base.rgb;
    var variance_accum = high_base.a;
    var weight_sum = 1.0;

    for (var y: i32 = -range; y <= range; y = y + 1) {
        for (var x: i32 = -range; x <= range; x = x + 1) {
            let low_pixel = clamp_pixel(low_size, low_center + vec2<i32>(x * spread, y * spread));
            let low_geometry = textureLoad(low_geometry_tex, low_pixel, 0);
            let low_depth = low_geometry.a;
            if (low_depth >= 0.99999) {
                continue;
            }

            let low_uv = uv_from_pixel(low_size, low_pixel);
            let low_pos = reconstruct_view_position(low_uv, low_depth);
            let low_normal = decode_scene_normal(low_geometry);
            let low_diffuse = textureLoad(low_diffuse_tex, low_pixel, 0);
            let delta = low_pos - high_pos;
            let dist_sq = max(dot(delta, delta), 0.0001);
            let plane_distance = abs(dot(high_normal, delta));
            let depth_weight = exp(-abs(low_depth - high_depth) * gi.params0.z * 22.0);
            let plane_weight = exp(-plane_distance * gi.params0.z * (5.0 + f32(range)));
            let normal_weight = pow(
                saturate(dot(low_normal, high_normal)),
                gi.params0.w * (0.75 + f32(range) * 0.2),
            ) + 0.001;
            let distance_weight = 1.0 / (1.0 + dist_sq * gi.params1.x * (0.85 + f32(range) * 0.15));
            let variance_weight = 1.0 / (1.0 + low_diffuse.a * 8.0);
            let weight = depth_weight * plane_weight * normal_weight * distance_weight * variance_weight;
            let scaled_weight = weight * contribution;
            accum += low_diffuse.rgb * scaled_weight;
            variance_accum += low_diffuse.a * scaled_weight;
            weight_sum += scaled_weight;
        }
    }

    return vec4<f32>(
        max(accum / max(weight_sum, 0.0001), vec3<f32>(0.0)),
        clamp(variance_accum / max(weight_sum, 0.0001), 0.02, 1.0),
    );
}

@compute @workgroup_size(8, 8, 1)
fn cs_upsample_mip3_to_2(@builtin(global_invocation_id) gid: vec3<u32>) {
    let pixel = vec2<i32>(gid.xy);
    let size = texture_size(high_geometry_tex);
    if (pixel.x >= size.x || pixel.y >= size.y) {
        return;
    }
    textureStore(output_tex, pixel, upsample_pixel(pixel, 3, 2, 0.75));
}

@compute @workgroup_size(8, 8, 1)
fn cs_upsample_mip2_to_1(@builtin(global_invocation_id) gid: vec3<u32>) {
    let pixel = vec2<i32>(gid.xy);
    let size = texture_size(high_geometry_tex);
    if (pixel.x >= size.x || pixel.y >= size.y) {
        return;
    }
    textureStore(output_tex, pixel, upsample_pixel(pixel, 2, 3, 0.65));
}

@compute @workgroup_size(8, 8, 1)
fn cs_upsample_mip1_to_0(@builtin(global_invocation_id) gid: vec3<u32>) {
    let pixel = vec2<i32>(gid.xy);
    let size = texture_size(high_geometry_tex);
    if (pixel.x >= size.x || pixel.y >= size.y) {
        return;
    }
    textureStore(output_tex, pixel, upsample_pixel(pixel, 1, 2, 0.55));
}
