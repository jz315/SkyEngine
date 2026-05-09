// WickedEngine-inspired SSGI diffuse compute.
// This is a WGSL port of the ssgiCS diffuse math using Texture2DArray depth
// and color atlases. The first wired version keeps direct texture loads instead
// of Wicked's groupshared R11G11B10 cache so the algorithm lands cleanly first.

struct SsgiUniform {
    params0: vec4<f32>,
    params1: vec4<f32>,
    params2: vec4<f32>,
    inverse_projection: mat4x4<f32>,
};

@group(0) @binding(0)
var input_depth: texture_2d_array<f32>;
@group(0) @binding(1)
var input_color: texture_2d_array<f32>;
@group(0) @binding(2)
var input_normal: texture_2d<f32>;

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

fn atlas_layer(regular_pixel: vec2<i32>, dims: vec2<u32>) -> u32 {
    let pixel = vec2<u32>(clamped_pixel(regular_pixel, dims));
    let slice_xy = pixel % vec2<u32>(4u);
    return slice_xy.x + slice_xy.y * 4u;
}

fn atlas_pixel(regular_pixel: vec2<i32>, dims: vec2<u32>) -> vec2<i32> {
    let pixel = vec2<u32>(clamped_pixel(regular_pixel, dims));
    return vec2<i32>(pixel / vec2<u32>(4u));
}

fn load_atlas_depth(regular_pixel: vec2<i32>, dims: vec2<u32>) -> f32 {
    return textureLoad(
        input_depth,
        atlas_pixel(regular_pixel, dims),
        i32(atlas_layer(regular_pixel, dims)),
        0,
    ).r;
}

fn lit_color(regular_pixel: vec2<i32>, dims: vec2<u32>) -> vec3<f32> {
    return max(
        textureLoad(
            input_color,
            atlas_pixel(regular_pixel, dims),
            i32(atlas_layer(regular_pixel, dims)),
            0,
        ).rgb,
        vec3<f32>(0.0),
    );
}

fn reconstruct_atlas(regular_pixel: vec2<i32>, dims: vec2<u32>) -> vec3<f32> {
    return reconstruct_position_from_depth(
        regular_pixel,
        dims,
        load_atlas_depth(regular_pixel, dims),
    );
}

fn load_normal(pixel: vec2<i32>, dims: vec2<u32>) -> vec3<f32> {
    return decode_view_normal(textureLoad(input_normal, clamped_pixel(pixel, dims), 0).rgb);
}

fn compute_diffuse(
    origin_position: vec3<f32>,
    origin_normal: vec3<f32>,
    origin_pixel: vec2<i32>,
    sample_pixel: vec2<i32>,
    dims: vec2<u32>,
) -> vec3<f32> {
    var color = lit_color(sample_pixel, dims);
    if (all(color <= vec3<f32>(0.0))) {
        return vec3<f32>(0.0);
    }

    let sample_position = reconstruct_atlas(sample_pixel, dims);
    let origin_to_sample = sample_position - origin_position;
    var occlusion = saturate(dot(origin_normal, origin_to_sample));
    occlusion = occlusion * saturate(1.0 + origin_to_sample.z * ssgi.params0.w);

    if (occlusion > 0.0) {
        let delta = sample_pixel - origin_pixel;
        var step_count = max(abs(delta.x), abs(delta.y));
        step_count = (step_count + 1) / 2;
        if (step_count > 0) {
            let step_count_f = f32(step_count);
            let increment = vec2<f32>(delta) / step_count_f;
            var xy = vec2<f32>(origin_pixel);
            for (var i = 0; i < step_count - 1; i = i + 1) {
                xy = xy + increment;
                let loc = vec2<i32>(xy);
                let dt = f32(i) / step_count_f;
                let z = mix(origin_position.z, sample_position.z, dt);
                let sample_z = reconstruct_atlas(loc, dims).z;
                if (sample_z < z - 0.1) {
                    color = lit_color(loc, dims);
                    break;
                }
            }
        }
    }

    return occlusion * color;
}

fn diffuse_at_pixel(pixel: vec2<i32>, dims: vec2<u32>) -> vec3<f32> {
    if (load_atlas_depth(pixel, dims) >= 0.99999) {
        return vec3<f32>(0.0);
    }

    let origin_position = reconstruct_atlas(pixel, dims);
    let origin_normal = load_normal(pixel, dims);
    let range = i32(max(ssgi.params0.y, 1.0));
    let spread = i32(max(ssgi.params0.z, 1.0));
    let range_spread_rcp2 = ssgi.params1.x;

    var diffuse = vec3<f32>(0.0);
    var sum = 0.0;
    for (var x = -range; x <= range; x = x + 1) {
        for (var y = -range; y <= range; y = y + 1) {
            let offset = vec2<i32>(x, y) * spread;
            let weight = saturate(1.0 - f32(abs(offset.x) * abs(offset.y)) * range_spread_rcp2);
            diffuse = diffuse + compute_diffuse(
                origin_position,
                origin_normal,
                pixel,
                pixel + offset,
                dims,
            ) * weight;
            sum = sum + weight;
        }
    }

    if (sum > 0.0) {
        diffuse = diffuse / sum;
    }
    return max(diffuse, vec3<f32>(0.0));
}

@compute @workgroup_size(8, 8, 1)
fn cs_main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let dims = textureDimensions(input_normal);
    if (gid.x >= dims.x || gid.y >= dims.y) {
        return;
    }

    let pixel = vec2<i32>(gid.xy);
    textureStore(output_diffuse, gid.xy, vec4<f32>(diffuse_at_pixel(pixel, dims), 1.0));
}
