// WickedEngine-inspired SSGI compute deinterleave.
// WickedEngine is MIT licensed. This WGSL port keeps the Texture2DArray atlas
// mapping, 16-slice deinterleave, light cutoff, and 0.96 energy loss. It runs
// once per mip level so it stays inside common wgpu storage texture limits.

struct SsgiUniform {
    params0: vec4<f32>,
    params1: vec4<f32>,
    params2: vec4<f32>,
    inverse_projection: mat4x4<f32>,
};

@group(0) @binding(0)
var t_scene_color: texture_2d<f32>;
@group(0) @binding(1)
var t_scene_depth: texture_depth_2d;
@group(0) @binding(2)
var t_scene_normal: texture_2d<f32>;

@group(1) @binding(0)
var<uniform> ssgi: SsgiUniform;

@group(2) @binding(0)
var atlas_depth: texture_storage_2d_array<r32float, write>;
@group(2) @binding(1)
var atlas_color: texture_storage_2d_array<rgba16float, write>;
@group(2) @binding(2)
var regular_depth: texture_storage_2d<r32float, write>;
@group(2) @binding(3)
var regular_normal: texture_storage_2d<rgba16float, write>;

fn clamped_pixel(pixel: vec2<i32>, dims: vec2<u32>) -> vec2<i32> {
    return clamp(pixel, vec2<i32>(0), vec2<i32>(dims) - vec2<i32>(1));
}

fn flatten_slice(slice_xy: vec2<u32>) -> u32 {
    return slice_xy.x + slice_xy.y * 4u;
}

@compute @workgroup_size(8, 8, 1)
fn cs_main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let regular_dims = textureDimensions(regular_depth);
    if (gid.x >= regular_dims.x || gid.y >= regular_dims.y) {
        return;
    }

    let regular_pixel_u = gid.xy;
    let regular_pixel = vec2<i32>(regular_pixel_u);
    let scene_dims = textureDimensions(t_scene_color);
    let scale = max(i32(round(ssgi.params2.x)), 1);
    // Wicked's deinterleave picks the same lattice points that its 16x16
    // groupshared tile fans out to 2x/4x/8x/16x outputs.
    let scene_pixel = clamped_pixel(regular_pixel * scale, scene_dims);

    var color = textureLoad(t_scene_color, scene_pixel, 0).rgb;
    if (all(color <= vec3<f32>(ssgi.params1.w))) {
        color = vec3<f32>(0.0);
    }
    color = max(color * ssgi.params1.z, vec3<f32>(0.0));

    let depth = textureLoad(t_scene_depth, scene_pixel, 0);
    let normal = textureLoad(t_scene_normal, scene_pixel, 0);
    let slice_xy = regular_pixel_u % vec2<u32>(4u);
    let atlas_pixel = regular_pixel_u / vec2<u32>(4u);
    let layer = flatten_slice(slice_xy);

    textureStore(atlas_depth, atlas_pixel, layer, vec4<f32>(depth, 0.0, 0.0, 1.0));
    textureStore(atlas_color, atlas_pixel, layer, vec4<f32>(color, 1.0));
    textureStore(regular_depth, regular_pixel_u, vec4<f32>(depth, 0.0, 0.0, 1.0));
    textureStore(regular_normal, regular_pixel_u, normal);
}
