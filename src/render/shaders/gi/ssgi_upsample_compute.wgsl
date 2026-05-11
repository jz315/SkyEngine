// WickedEngine-inspired SSGI bilateral upsample compute.
// This ports the depth/normal weighted upsample shape from ssgi_upsampleCS.

struct SsgiUniform {
    params0: vec4<f32>,
    params1: vec4<f32>,
    params2: vec4<f32>,
    inverse_projection: mat4x4<f32>,
};

@group(0) @binding(0)
var input_depth_low: texture_2d<f32>;
@group(0) @binding(1)
var input_normal_low: texture_2d<f32>;
@group(0) @binding(2)
var input_diffuse_low: texture_2d<f32>;
@group(0) @binding(3)
var input_depth_high: texture_2d<f32>;
@group(0) @binding(4)
var input_normal_high: texture_2d<f32>;
@group(0) @binding(5)
var input_diffuse_high: texture_2d<f32>;

@group(1) @binding(0)
var<uniform> ssgi: SsgiUniform;

@group(2) @binding(0)
var output_diffuse: texture_storage_2d<rgba16float, write>;

fn saturate(v: f32) -> f32 {
    return clamp(v, 0.0, 1.0);
}

fn clamped_pixel(pixel: vec2<i32>, dims: vec2<u32>) -> vec2<i32> {
    return clamp(pixel, vec2<i32>(0), vec2<i32>(dims) - vec2<i32>(1));
}

fn pixel_uv(pixel: vec2<i32>, dims: vec2<u32>) -> vec2<f32> {
    return (vec2<f32>(clamped_pixel(pixel, dims)) + vec2<f32>(0.5)) / vec2<f32>(dims);
}

fn decode_view_normal(encoded: vec3<f32>) -> vec3<f32> {
    let normal = encoded * 2.0 - vec3<f32>(1.0);
    let len_sq = dot(normal, normal);
    if (len_sq <= 0.000001) {
        return vec3<f32>(0.0, 0.0, -1.0);
    }
    let unit = normal * inverseSqrt(len_sq);
    return vec3<f32>(unit.x, unit.y, -unit.z);
}

fn reconstruct_position_from_depth(pixel: vec2<i32>, dims: vec2<u32>, depth: f32) -> vec3<f32> {
    let uv = pixel_uv(pixel, dims);
    let clip = vec4<f32>(uv.x * 2.0 - 1.0, (1.0 - uv.y) * 2.0 - 1.0, depth, 1.0);
    let view = ssgi.inverse_projection * clip;
    var position = view.xyz;
    if (abs(view.w) > 0.000001) {
        position = position / view.w;
    }
    // Wicked's view-space SSGI math uses positive Z as distance from camera.
    // SkyEngine uses a right-handed view matrix, so projected view-space Z is
    // negative in front of the camera.
    position.z = -position.z;
    return position;
}

fn load_depth_high(pixel: vec2<i32>, dims: vec2<u32>) -> f32 {
    return textureLoad(input_depth_high, clamped_pixel(pixel, dims), 0).r;
}

fn load_depth_low(pixel: vec2<i32>, dims: vec2<u32>) -> f32 {
    return textureLoad(input_depth_low, clamped_pixel(pixel, dims), 0).r;
}

fn load_normal_high(pixel: vec2<i32>, dims: vec2<u32>) -> vec3<f32> {
    return decode_view_normal(textureLoad(input_normal_high, clamped_pixel(pixel, dims), 0).rgb);
}

fn load_normal_low(pixel: vec2<i32>, dims: vec2<u32>) -> vec3<f32> {
    return decode_view_normal(textureLoad(input_normal_low, clamped_pixel(pixel, dims), 0).rgb);
}

fn bilateral_depth_weight(depth_delta: f32) -> f32 {
    let soft_reject = 1.0 - saturate(depth_delta * max(ssgi.params0.w, 0.0001));
    return soft_reject * soft_reject * soft_reject * soft_reject;
}

fn luminance(color: vec3<f32>) -> f32 {
    return dot(max(color, vec3<f32>(0.0)), vec3<f32>(0.2126, 0.7152, 0.0722));
}

fn limit_luminance(color: vec3<f32>, max_luma: f32) -> vec3<f32> {
    let luma = luminance(color);
    if (luma <= max_luma || luma <= 0.00001) {
        return color;
    }
    return color * (max_luma / luma);
}

const SSGI_MAX_FILTERED_LUMINANCE: f32 = 2.0;
const SSGI_LOW_MIP_BLEND: f32 = 0.35;

fn upsample_diffuse(output_pixel: vec2<i32>, high_dims: vec2<u32>) -> vec3<f32> {
    let low_dims = textureDimensions(input_diffuse_low);
    let center_depth = load_depth_high(output_pixel, high_dims);
    let center_linear_depth =
        reconstruct_position_from_depth(output_pixel, high_dims, center_depth).z;
    let center_normal = load_normal_high(output_pixel, high_dims);
    let low_base = output_pixel / vec2<i32>(2);
    let range = i32(max(ssgi.params0.y, 1.0));
    let spread = i32(max(ssgi.params0.z, 1.0));
    let normal_power = max(ssgi.params1.y, 0.001);

    var result = vec3<f32>(0.0);
    var sum = 0.0;
    for (var x = -range; x <= range; x = x + 1) {
        for (var y = -range; y <= range; y = y + 1) {
            let low_pixel = clamped_pixel(low_base + vec2<i32>(x, y) * spread, low_dims);
            let low_depth = load_depth_low(low_pixel, low_dims);
            let sample_linear_depth =
                reconstruct_position_from_depth(low_pixel, low_dims, low_depth).z;
            let sample_normal = load_normal_low(low_pixel, low_dims);
            let depth_weight = bilateral_depth_weight(abs(sample_linear_depth - center_linear_depth));
            let normal_weight = pow(saturate(dot(sample_normal, center_normal)), normal_power);
            let weight = depth_weight * normal_weight;
            result = result + textureLoad(input_diffuse_low, low_pixel, 0).rgb * weight;
            sum = sum + weight;
        }
    }

    if (sum > 0.0) {
        result = result / sum;
    }
    let high_diffuse = max(textureLoad(input_diffuse_high, clamped_pixel(output_pixel, high_dims), 0).rgb, vec3<f32>(0.0));
    let combined = mix(high_diffuse, result, SSGI_LOW_MIP_BLEND);
    return limit_luminance(max(combined, vec3<f32>(0.0)), SSGI_MAX_FILTERED_LUMINANCE);
}

@compute @workgroup_size(8, 8, 1)
fn cs_main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let dims = textureDimensions(input_depth_high);
    if (gid.x >= dims.x || gid.y >= dims.y) {
        return;
    }

    let pixel = vec2<i32>(gid.xy);
    textureStore(output_diffuse, gid.xy, vec4<f32>(upsample_diffuse(pixel, dims), 1.0));
}
