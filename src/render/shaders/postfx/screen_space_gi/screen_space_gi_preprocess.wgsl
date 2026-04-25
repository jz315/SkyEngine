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
var input_tex: texture_2d<f32>;
@group(2) @binding(1)
var velocity_tex: texture_2d<f32>;
@group(2) @binding(2)
var input_sampler: sampler;

@group(3) @binding(0)
var mip_out0: texture_storage_2d<rgba16float, write>;
@group(3) @binding(1)
var mip_out1: texture_storage_2d<rgba16float, write>;
@group(3) @binding(2)
var mip_out2: texture_storage_2d<rgba16float, write>;
@group(3) @binding(3)
var mip_out3: texture_storage_2d<rgba16float, write>;

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

fn scene_max_pixel() -> vec2<i32> {
    return vec2<i32>(max(gi.viewport.xy - vec2<f32>(1.0), vec2<f32>(0.0)));
}

fn scene_pixel_from_uv(uv: vec2<f32>) -> vec2<i32> {
    let clamped = clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0));
    let pixel = vec2<i32>(clamped * gi.viewport.xy);
    return clamp(pixel, vec2<i32>(0), scene_max_pixel());
}

fn scene_pixel_to_uv(pixel: vec2<i32>) -> vec2<f32> {
    let clamped = clamp(pixel, vec2<i32>(0), scene_max_pixel());
    return (vec2<f32>(clamped) + vec2<f32>(0.5, 0.5)) * gi.viewport.zw;
}

fn scene_depth_at_pixel(pixel: vec2<i32>) -> f32 {
    return textureLoad(depth_tex, clamp(pixel, vec2<i32>(0), scene_max_pixel()), 0);
}

fn scene_normal_at_pixel(pixel: vec2<i32>) -> vec3<f32> {
    let encoded = textureLoad(normal_tex, clamp(pixel, vec2<i32>(0), scene_max_pixel()), 0);
    return safe_normalize(encoded.xyz * 2.0 - vec3<f32>(1.0, 1.0, 1.0));
}

fn scene_albedo_at_pixel(pixel: vec2<i32>) -> vec3<f32> {
    return textureLoad(albedo_tex, clamp(pixel, vec2<i32>(0), scene_max_pixel()), 0).rgb;
}

fn scene_material_at_pixel(pixel: vec2<i32>) -> vec4<f32> {
    return textureLoad(material_tex, clamp(pixel, vec2<i32>(0), scene_max_pixel()), 0);
}

fn scene_albedo_at_uv(uv: vec2<f32>) -> vec3<f32> {
    return textureLoad(albedo_tex, scene_pixel_from_uv(uv), 0).rgb;
}

fn scene_material_at_uv(uv: vec2<f32>) -> vec4<f32> {
    return textureLoad(material_tex, scene_pixel_from_uv(uv), 0);
}

fn input_radiance_at_uv(uv: vec2<f32>) -> vec3<f32> {
    return textureSampleLevel(
        input_tex,
        input_sampler,
        clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0)),
        0.0,
    ).rgb;
}

fn encode_scene_normal(normal: vec3<f32>) -> vec3<f32> {
    return safe_normalize(normal) * 0.5 + vec3<f32>(0.5, 0.5, 0.5);
}

fn radiance_source_weight(color: vec3<f32>) -> f32 {
    let luma = luminance(color);
    let source_mask = smoothstep(0.8, 2.4, luma);
    return source_mask * (0.5 + luma);
}

fn material_diffuse_weight(surface: vec4<f32>) -> f32 {
    let metallic = saturate(surface.x);
    let unlit = select(0.0, 1.0, surface.z > 0.5);
    return (1.0 - metallic * 0.85) * (1.0 - unlit * 0.9);
}

fn material_roughness_weight(surface: vec4<f32>) -> f32 {
    return 0.45 + saturate(surface.y) * 0.55;
}

fn bounce_radiance_from_surface(
    surface_albedo: vec3<f32>,
    surface_material: vec4<f32>,
    radiance: vec3<f32>,
) -> vec3<f32> {
    let diffuse_weight = material_diffuse_weight(surface_material);
    let roughness_weight = material_roughness_weight(surface_material);
    let diffuse_bounce = mix(
        radiance,
        surface_albedo * luminance(radiance),
        0.6,
    ) * diffuse_weight * roughness_weight;
    let emissive_bounce = radiance
        * (0.18 + select(0.0, 1.1, surface_material.z > 0.5))
        * (0.25 + luminance(surface_albedo));
    return diffuse_bounce + emissive_bounce;
}

fn gi_source_radiance(pixel: vec2<i32>, radiance: vec3<f32>) -> vec3<f32> {
    let bounced = bounce_radiance_from_surface(
        scene_albedo_at_pixel(pixel),
        scene_material_at_pixel(pixel),
        radiance,
    );
    return max(mix(radiance * 0.18, bounced, 0.9), vec3<f32>(0.0));
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

fn mip_grid_size(level: i32) -> i32 {
    if (level == 0) {
        return 2;
    }
    if (level == 1) {
        return 4;
    }
    if (level == 2) {
        return 8;
    }
    return 16;
}

fn write_mip_output(level: i32, pixel: vec2<i32>, value: vec4<f32>) {
    if (level == 0) {
        textureStore(mip_out0, pixel, value);
        return;
    }
    if (level == 1) {
        textureStore(mip_out1, pixel, value);
        return;
    }
    if (level == 2) {
        textureStore(mip_out2, pixel, value);
        return;
    }
    textureStore(mip_out3, pixel, value);
}

fn geometry_value_for_level(pixel: vec2<i32>, level: i32) -> vec4<f32> {
    let grid_size = mip_grid_size(level);
    let base_pixel = pixel * vec2<i32>(grid_size, grid_size);
    var best_depth = 1.0;
    var best_normal = vec3<f32>(0.0, 0.0, 1.0);

    for (var y: i32 = 0; y < grid_size; y = y + 1) {
        for (var x: i32 = 0; x < grid_size; x = x + 1) {
            let sample_pixel = clamp(base_pixel + vec2<i32>(x, y), vec2<i32>(0), scene_max_pixel());
            let sample_depth = scene_depth_at_pixel(sample_pixel);
            if (sample_depth < best_depth) {
                best_depth = sample_depth;
                best_normal = scene_normal_at_pixel(sample_pixel);
            }
        }
    }

    if (best_depth >= 0.99999) {
        return vec4<f32>(0.5, 0.5, 1.0, 1.0);
    }
    return vec4<f32>(encode_scene_normal(best_normal), best_depth);
}

fn radiance_value_for_level(pixel: vec2<i32>, level: i32) -> vec4<f32> {
    let grid_size = mip_grid_size(level);
    let base_pixel = pixel * vec2<i32>(grid_size, grid_size);
    let center_pixel = clamp(
        base_pixel + vec2<i32>(grid_size / 2, grid_size / 2),
        vec2<i32>(0),
        scene_max_pixel(),
    );
    let center_uv = scene_pixel_to_uv(center_pixel);
    var accum = vec3<f32>(0.0, 0.0, 0.0);
    var weight_sum = 0.0;
    var best_color = vec3<f32>(0.0, 0.0, 0.0);
    var best_weight = -1.0;

    for (var y: i32 = 0; y < grid_size; y = y + 1) {
        for (var x: i32 = 0; x < grid_size; x = x + 1) {
            let sample_pixel = clamp(base_pixel + vec2<i32>(x, y), vec2<i32>(0), scene_max_pixel());
            let sample_depth = scene_depth_at_pixel(sample_pixel);
            if (sample_depth >= 0.99999) {
                continue;
            }

            let sample_uv = scene_pixel_to_uv(sample_pixel);
            let sample_color = min(
                input_radiance_at_uv(sample_uv),
                vec3<f32>(12.0, 12.0, 12.0),
            );
            let source = gi_source_radiance(sample_pixel, sample_color);
            let weight =
                radiance_source_weight(source)
                * (0.35 + surface_similarity(center_uv, sample_uv) * 0.65);

            if (weight > best_weight) {
                best_weight = weight;
                best_color = source;
            }

            accum += source * weight;
            weight_sum += weight;
        }
    }

    if (weight_sum > 0.0001) {
        return vec4<f32>((accum / weight_sum) * 0.96, 1.0);
    }
    if (best_weight > 0.0) {
        return vec4<f32>(best_color * 0.96, 1.0);
    }
    return vec4<f32>(0.0, 0.0, 0.0, 1.0);
}

@compute @workgroup_size(8, 8, 1)
fn cs_build_geometry_mips(@builtin(global_invocation_id) gid: vec3<u32>) {
    let pixel = vec2<i32>(gid.xy);

    for (var level: i32 = 0; level < 4; level = level + 1) {
        let size = mip_output_size(level);
        if (all(pixel < size)) {
            write_mip_output(level, pixel, geometry_value_for_level(pixel, level));
        }
    }
}

@compute @workgroup_size(8, 8, 1)
fn cs_build_radiance_mips(@builtin(global_invocation_id) gid: vec3<u32>) {
    let pixel = vec2<i32>(gid.xy);

    for (var level: i32 = 0; level < 4; level = level + 1) {
        let size = mip_output_size(level);
        if (all(pixel < size)) {
            write_mip_output(level, pixel, radiance_value_for_level(pixel, level));
        }
    }
}
