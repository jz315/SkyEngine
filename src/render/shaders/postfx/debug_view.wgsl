// WickedEngine-inspired debug buffer viewer.
// Wicked uses named debug resources such as debugUAV and direct image draws of
// intermediate render targets. This pass keeps the same plain "show the buffer"
// shape for SkyEngine's programmable modern 3D pipeline.

struct DebugViewUniform {
    params: vec4<u32>,
    atlas_mul_add: vec4<f32>,
};

@group(0) @binding(0)
var depth_tex: texture_depth_2d;
@group(0) @binding(1)
var source_tex: texture_2d<f32>;

@group(1) @binding(0)
var<uniform> debug_view: DebugViewUniform;

const MODE_SCENE_DEPTH: u32 = 0u;
const MODE_SCENE_NORMAL: u32 = 1u;
const MODE_SOURCE_RGB: u32 = 2u;
const MODE_ROUGHNESS: u32 = 3u;
const MODE_METALLIC: u32 = 4u;
const MODE_VELOCITY: u32 = 5u;
const MODE_SHADOW_DEPTH: u32 = 6u;

fn clamped_pixel(uv: vec2<f32>, dims: vec2<u32>) -> vec2<i32> {
    let size = max(dims, vec2<u32>(1u));
    let pixel = vec2<i32>(uv * vec2<f32>(size));
    return clamp(pixel, vec2<i32>(0), vec2<i32>(size) - vec2<i32>(1));
}

fn source_color(uv: vec2<f32>) -> vec4<f32> {
    let dims = textureDimensions(source_tex);
    return textureLoad(source_tex, clamped_pixel(uv, dims), 0);
}

fn scene_depth(uv: vec2<f32>) -> f32 {
    let dims = textureDimensions(depth_tex);
    return textureLoad(depth_tex, clamped_pixel(uv, dims), 0);
}

fn shadow_depth_uv(uv: vec2<f32>) -> vec2<f32> {
    let cascade_count = max(debug_view.params.z, 1u);
    if (cascade_count <= 1u) {
        return uv * debug_view.atlas_mul_add.xy + debug_view.atlas_mul_add.zw;
    }
    let cascade = min(debug_view.params.y, cascade_count - 1u);
    return vec2<f32>(uv.x + f32(cascade), uv.y) * debug_view.atlas_mul_add.xy
        + debug_view.atlas_mul_add.zw;
}

fn visualize_velocity(v: vec2<f32>) -> vec3<f32> {
    return vec3<f32>(v * 0.5 + vec2<f32>(0.5), length(v) * 16.0);
}

fn visualize_shadow_depth(depth: f32) -> vec3<f32> {
    return vec3<f32>(1.0 - clamp(depth, 0.0, 1.0));
}

@fragment
fn fs_main(in: FullscreenOutput) -> @location(0) vec4<f32> {
    let mode = debug_view.params.x;
    let source = source_color(in.uv);

    if (mode == MODE_SCENE_DEPTH) {
        let depth = scene_depth(in.uv);
        return vec4<f32>(vec3<f32>(depth), 1.0);
    }
    if (mode == MODE_SCENE_NORMAL) {
        return vec4<f32>(source.rgb, 1.0);
    }
    if (mode == MODE_ROUGHNESS) {
        return vec4<f32>(vec3<f32>(source.r), 1.0);
    }
    if (mode == MODE_METALLIC) {
        return vec4<f32>(vec3<f32>(source.g), 1.0);
    }
    if (mode == MODE_VELOCITY) {
        return vec4<f32>(visualize_velocity(source.xy), 1.0);
    }
    if (mode == MODE_SHADOW_DEPTH) {
        let depth = scene_depth(shadow_depth_uv(in.uv));
        return vec4<f32>(visualize_shadow_depth(depth), 1.0);
    }

    return vec4<f32>(max(source.rgb, vec3<f32>(0.0)), 1.0);
}
