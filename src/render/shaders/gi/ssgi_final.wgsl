// WickedEngine-inspired final SSGI upsample/composite pass.
// This mirrors WickedEngine's 2x -> output step: low 2x diffuse/depth/normal
// is bilateral-upsampled against full-resolution scene depth and normal.

struct SsgiUniform {
    params0: vec4<f32>,
    params1: vec4<f32>,
    params2: vec4<f32>,
    inverse_projection: mat4x4<f32>,
};

@group(0) @binding(0)
var t_depth_low: texture_2d<f32>;
@group(0) @binding(1)
var t_normal_low: texture_2d<f32>;
@group(0) @binding(2)
var t_diffuse_low: texture_2d<f32>;
@group(0) @binding(3)
var t_scene_depth: texture_depth_2d;
@group(0) @binding(4)
var t_scene_normal: texture_2d<f32>;
@group(0) @binding(5)
var t_scene_color: texture_2d<f32>;

@group(1) @binding(0)
var<uniform> ssgi: SsgiUniform;

const UPSAMPLE_DEPTH_THRESHOLD: f32 = 0.1;

fn saturate(v: f32) -> f32 {
    return clamp(v, 0.0, 1.0);
}

fn luminance(color: vec3<f32>) -> f32 {
    return dot(max(color, vec3<f32>(0.0)), vec3<f32>(0.2126, 0.7152, 0.0722));
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

fn upsample_diffuse(output_pixel: vec2<i32>, high_dims: vec2<u32>) -> vec3<f32> {
    let low_dims = textureDimensions(t_diffuse_low);
    let scene_pixel = clamped_pixel(output_pixel, high_dims);
    let center_depth = textureLoad(t_scene_depth, scene_pixel, 0);
    let center_linear_depth = reconstruct_position_from_depth(scene_pixel, high_dims, center_depth).z;
    let center_normal = decode_view_normal(textureLoad(t_scene_normal, scene_pixel, 0).rgb);
    let low_base = output_pixel / vec2<i32>(2);
    let range = i32(max(ssgi.params0.y, 1.0));
    let spread = i32(max(ssgi.params0.z, 1.0));
    let normal_power = max(ssgi.params1.y, 0.001);

    var result = vec3<f32>(0.0);
    var sum = 0.0;
    for (var x = -range; x <= range; x = x + 1) {
        for (var y = -range; y <= range; y = y + 1) {
            let low_pixel = clamped_pixel(low_base + vec2<i32>(x, y) * spread, low_dims);
            let low_depth = textureLoad(t_depth_low, low_pixel, 0).r;
            let sample_linear_depth = reconstruct_position_from_depth(low_pixel, low_dims, low_depth).z;
            let sample_normal = decode_view_normal(textureLoad(t_normal_low, low_pixel, 0).rgb);
            let depth_weight = 1.0 - saturate(abs(sample_linear_depth - center_linear_depth) * UPSAMPLE_DEPTH_THRESHOLD);
            let normal_weight = pow(saturate(dot(sample_normal, center_normal)), normal_power) + 0.001;
            let weight = depth_weight * normal_weight;
            result = result + textureLoad(t_diffuse_low, low_pixel, 0).rgb * weight;
            sum = sum + weight;
        }
    }

    if (sum > 0.0) {
        result = result / sum;
    }
    return max(result, vec3<f32>(0.0));
}

fn stabilize_indirect(diffuse: vec3<f32>, scene_color: vec3<f32>) -> vec3<f32> {
    let diffuse_luma = luminance(diffuse);
    if (diffuse_luma <= 0.00001) {
        return vec3<f32>(0.0);
    }

    let desaturated = mix(vec3<f32>(diffuse_luma), diffuse, 0.35);
    let desaturated_luma = max(luminance(desaturated), 0.00001);
    let scene_luma = luminance(scene_color);
    let max_luma = max(0.04, scene_luma * 0.28 + 0.06);
    return desaturated * min(1.0, max_luma / desaturated_luma);
}

@fragment
fn fs_final_upsample(input: FullscreenOutput) -> @location(0) vec4<f32> {
    let dims = textureDimensions(t_scene_color);
    let pixel = vec2<i32>(input.position.xy);
    let scene_pixel = clamped_pixel(pixel, dims);
    let scene_color = textureLoad(t_scene_color, scene_pixel, 0);
    let center_depth = textureLoad(t_scene_depth, scene_pixel, 0);
    if (center_depth >= 0.99999) {
        return scene_color;
    }
    let diffuse = stabilize_indirect(upsample_diffuse(pixel, dims), scene_color.rgb) * ssgi.params0.x;
    return vec4<f32>(scene_color.rgb + diffuse, scene_color.a);
}
