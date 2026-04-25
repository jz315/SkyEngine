@group(0) @binding(0)
var depth_tex: texture_depth_2d;
@group(0) @binding(1)
var normal_tex: texture_2d<f32>;
@group(0) @binding(2)
var albedo_tex: texture_2d<f32>;
@group(0) @binding(3)
var material_tex: texture_2d<f32>;
@group(0) @binding(4)
var emissive_tex: texture_2d<f32>;

struct GlobalIlluminationUniform {
    inverse_projection: mat4x4<f32>,
    inverse_view: mat4x4<f32>,
    viewport: vec4<f32>,
    probe_origin_spacing: vec4<f32>,
    probe_counts: vec4<u32>,
    ambient_color: vec4<f32>,
    ground_color: vec4<f32>,
    params0: vec4<f32>,
    params1: vec4<f32>,
};

@group(1) @binding(0)
var<uniform> gi: GlobalIlluminationUniform;

@group(2) @binding(0)
var input_tex: texture_2d<f32>;
@group(2) @binding(1)
var input_sampler: sampler;

struct ProbeSample {
    sh0: vec4<f32>,
    shx: vec4<f32>,
    shy: vec4<f32>,
    shz: vec4<f32>,
};

struct ProbeLighting {
    irradiance: vec3<f32>,
    enclosure: f32,
    directionality: f32,
};

@group(3) @binding(0)
var<storage, read> probe_data: array<ProbeSample>;

const DETAIL_DIRS: array<vec2<f32>, 8> = array<vec2<f32>, 8>(
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

fn scene_max_pixel() -> vec2<i32> {
    return vec2<i32>(max(gi.viewport.xy - vec2<f32>(1.0), vec2<f32>(0.0)));
}

fn scene_pixel_from_uv(uv: vec2<f32>) -> vec2<i32> {
    let clamped = clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0));
    let pixel = vec2<i32>(clamped * gi.viewport.xy);
    return clamp(pixel, vec2<i32>(0), scene_max_pixel());
}

fn scene_color_at_uv(uv: vec2<f32>) -> vec3<f32> {
    return textureSampleLevel(
        input_tex,
        input_sampler,
        clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0)),
        0.0,
    ).rgb;
}

fn scene_depth_at_uv(uv: vec2<f32>) -> f32 {
    return textureLoad(depth_tex, scene_pixel_from_uv(uv), 0);
}

fn scene_normal_at_uv(uv: vec2<f32>) -> vec3<f32> {
    let encoded = textureLoad(normal_tex, scene_pixel_from_uv(uv), 0);
    return safe_normalize(encoded.xyz * 2.0 - vec3<f32>(1.0, 1.0, 1.0));
}

fn scene_albedo_at_uv(uv: vec2<f32>) -> vec3<f32> {
    return textureLoad(albedo_tex, scene_pixel_from_uv(uv), 0).rgb;
}

fn scene_material_at_uv(uv: vec2<f32>) -> vec4<f32> {
    return textureLoad(material_tex, scene_pixel_from_uv(uv), 0);
}

fn scene_emissive_at_uv(uv: vec2<f32>) -> vec3<f32> {
    return textureLoad(emissive_tex, scene_pixel_from_uv(uv), 0).rgb;
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

fn hemisphere_ambient(normal_vs: vec3<f32>) -> vec3<f32> {
    let world_normal = safe_normalize((gi.inverse_view * vec4<f32>(normal_vs, 0.0)).xyz);
    let hemi = saturate(world_normal.y * 0.5 + 0.5);
    return mix(gi.ground_color.rgb, gi.ambient_color.rgb * gi.params1.w, hemi);
}

fn probe_index(coord: vec3<u32>) -> u32 {
    return coord.x
        + coord.y * gi.probe_counts.x
        + coord.z * gi.probe_counts.x * gi.probe_counts.y;
}

fn probe_value(coord: vec3<u32>) -> ProbeSample {
    let clamped = min(
        coord,
        vec3<u32>(
            gi.probe_counts.x - 1u,
            gi.probe_counts.y - 1u,
            gi.probe_counts.z - 1u,
        ),
    );
    return probe_data[probe_index(clamped)];
}

fn evaluate_probe(sample: ProbeSample, world_normal: vec3<f32>) -> ProbeLighting {
    let directionality = saturate(sample.shx.w);
    let enclosure = saturate(sample.sh0.w);
    let linear =
        sample.shx.rgb * world_normal.x
        + sample.shy.rgb * world_normal.y
        + sample.shz.rgb * world_normal.z;
    let base = sample.sh0.rgb * mix(0.86, 0.64, enclosure);
    let directional = linear * mix(0.42, 0.92, directionality);
    let irradiance = max(base + directional, vec3<f32>(0.0));
    return ProbeLighting(irradiance, enclosure, directionality);
}

fn sample_probe_grid(world_pos: vec3<f32>, world_normal: vec3<f32>) -> ProbeLighting {
    if (gi.probe_counts.x == 0u || gi.probe_counts.y == 0u || gi.probe_counts.z == 0u) {
        return ProbeLighting(vec3<f32>(0.0), 0.0, 0.0);
    }

    let spacing = max(gi.probe_origin_spacing.w, 0.001);
    let local = (world_pos - gi.probe_origin_spacing.xyz) / spacing;
    let max_coord = vec3<f32>(
        f32(max(gi.probe_counts.x, 1u) - 1u),
        f32(max(gi.probe_counts.y, 1u) - 1u),
        f32(max(gi.probe_counts.z, 1u) - 1u),
    );
    let clamped = clamp(local, vec3<f32>(0.0), max_coord);
    let base = vec3<u32>(clamped);
    let frac = fract(clamped);

    let p000 = probe_value(base);
    let p100 = probe_value(min(base + vec3<u32>(1u, 0u, 0u), vec3<u32>(gi.probe_counts.xyz - vec3<u32>(1u))));
    let p010 = probe_value(min(base + vec3<u32>(0u, 1u, 0u), vec3<u32>(gi.probe_counts.xyz - vec3<u32>(1u))));
    let p110 = probe_value(min(base + vec3<u32>(1u, 1u, 0u), vec3<u32>(gi.probe_counts.xyz - vec3<u32>(1u))));
    let p001 = probe_value(min(base + vec3<u32>(0u, 0u, 1u), vec3<u32>(gi.probe_counts.xyz - vec3<u32>(1u))));
    let p101 = probe_value(min(base + vec3<u32>(1u, 0u, 1u), vec3<u32>(gi.probe_counts.xyz - vec3<u32>(1u))));
    let p011 = probe_value(min(base + vec3<u32>(0u, 1u, 1u), vec3<u32>(gi.probe_counts.xyz - vec3<u32>(1u))));
    let p111 = probe_value(min(base + vec3<u32>(1u, 1u, 1u), vec3<u32>(gi.probe_counts.xyz - vec3<u32>(1u))));

    let sh0_00 = mix(p000.sh0, p100.sh0, frac.x);
    let sh0_10 = mix(p010.sh0, p110.sh0, frac.x);
    let sh0_01 = mix(p001.sh0, p101.sh0, frac.x);
    let sh0_11 = mix(p011.sh0, p111.sh0, frac.x);
    let sh0_0 = mix(sh0_00, sh0_10, frac.y);
    let sh0_1 = mix(sh0_01, sh0_11, frac.y);
    let sh0 = mix(sh0_0, sh0_1, frac.z);

    let shx_00 = mix(p000.shx, p100.shx, frac.x);
    let shx_10 = mix(p010.shx, p110.shx, frac.x);
    let shx_01 = mix(p001.shx, p101.shx, frac.x);
    let shx_11 = mix(p011.shx, p111.shx, frac.x);
    let shx_0 = mix(shx_00, shx_10, frac.y);
    let shx_1 = mix(shx_01, shx_11, frac.y);
    let shx = mix(shx_0, shx_1, frac.z);

    let shy_00 = mix(p000.shy, p100.shy, frac.x);
    let shy_10 = mix(p010.shy, p110.shy, frac.x);
    let shy_01 = mix(p001.shy, p101.shy, frac.x);
    let shy_11 = mix(p011.shy, p111.shy, frac.x);
    let shy_0 = mix(shy_00, shy_10, frac.y);
    let shy_1 = mix(shy_01, shy_11, frac.y);
    let shy = mix(shy_0, shy_1, frac.z);

    let shz_00 = mix(p000.shz, p100.shz, frac.x);
    let shz_10 = mix(p010.shz, p110.shz, frac.x);
    let shz_01 = mix(p001.shz, p101.shz, frac.x);
    let shz_11 = mix(p011.shz, p111.shz, frac.x);
    let shz_0 = mix(shz_00, shz_10, frac.y);
    let shz_1 = mix(shz_01, shz_11, frac.y);
    let shz = mix(shz_0, shz_1, frac.z);

    return evaluate_probe(ProbeSample(sh0, shx, shy, shz), world_normal);
}

fn ambient_occlusion(
    uv: vec2<f32>,
    center_pos: vec3<f32>,
    center_normal: vec3<f32>,
    center_depth: f32,
) -> f32 {
    var occlusion = 0.0;
    var weight_sum = 0.0;
    let base_radius = max(2.0, gi.params1.x * 0.28);

    for (var dir_index: i32 = 0; dir_index < 8; dir_index = dir_index + 1) {
        let dir = DETAIL_DIRS[dir_index];
        for (var ring_index: i32 = 0; ring_index < 2; ring_index = ring_index + 1) {
            let sample_uv = uv + dir * gi.viewport.zw * base_radius * (1.0 + f32(ring_index));
            let sample_depth = scene_depth_at_uv(sample_uv);
            if (sample_depth >= 0.99999) {
                continue;
            }

            let sample_pos = reconstruct_view_position(sample_uv, sample_depth);
            let delta = sample_pos - center_pos;
            let dist_sq = max(dot(delta, delta), 0.0001);
            let delta_dir = delta * inverseSqrt(dist_sq);
            let horizon = 1.0 - saturate(dot(center_normal, delta_dir));
            let plane_distance = abs(dot(center_normal, delta));
            let depth_weight = exp(-abs(sample_depth - center_depth) * gi.params1.y * 18.0);
            let plane_weight = exp(-plane_distance * gi.params1.y * 2.4);
            let range_weight = 1.0 / (1.0 + dist_sq * 14.0);
            let weight = depth_weight * plane_weight * range_weight;
            occlusion += horizon * weight;
            weight_sum += weight;
        }
    }

    let raw = occlusion / max(weight_sum, 0.0001);
    return 1.0 - saturate(raw * 1.35);
}

fn screen_space_detail(uv: vec2<f32>, center_depth: f32, center_normal: vec3<f32>) -> vec3<f32> {
    var accum = vec3<f32>(0.0);
    var weight_sum = 0.0;

    for (var dir_index: i32 = 0; dir_index < 8; dir_index = dir_index + 1) {
        let dir = DETAIL_DIRS[dir_index];
        for (var ring_index: i32 = 0; ring_index < 2; ring_index = ring_index + 1) {
            let ring_scale = 0.75 + f32(ring_index) * 0.85;
            let sample_uv = uv + dir * gi.viewport.zw * gi.params1.x * ring_scale;
            let sample_depth = scene_depth_at_uv(sample_uv);
            if (sample_depth >= 0.99999) {
                continue;
            }

            let sample_normal = scene_normal_at_uv(sample_uv);
            let sample_color = scene_color_at_uv(sample_uv);
            let sample_emissive = scene_emissive_at_uv(sample_uv);
            let sample_albedo = scene_albedo_at_uv(sample_uv);
            let sample_material = scene_material_at_uv(sample_uv);
            let kernel_weight = exp(-0.85 * f32(ring_index + 1));
            let depth_weight = exp(-abs(sample_depth - center_depth) * gi.params1.y * 18.0);
            let normal_weight = pow(
                saturate(dot(center_normal, sample_normal)),
                gi.params1.z,
            ) + 0.001;
            let diffuse_weight = 1.0 - saturate(sample_material.x) * 0.85;
            let highlight = max(sample_color - sample_albedo * 0.34, vec3<f32>(0.0));
            let bleed =
                (highlight * 0.055 + sample_albedo * min(luminance(highlight), 1.0) * 0.028)
                    * diffuse_weight
                + sample_emissive * 0.26;
            let weight = kernel_weight * depth_weight * normal_weight;
            accum += bleed * weight;
            weight_sum += weight;
        }
    }

    if (weight_sum <= 0.0001) {
        return vec3<f32>(0.0);
    }
    return min(accum / weight_sum, vec3<f32>(0.20));
}

@fragment
fn fs_main(in: FullscreenOutput) -> @location(0) vec4<f32> {
    let uv = in.uv;
    let scene = textureSampleLevel(
        input_tex,
        input_sampler,
        clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0)),
        0.0,
    );
    let center_depth = scene_depth_at_uv(uv);
    if (center_depth >= 0.99999) {
        return scene;
    }

    let material = scene_material_at_uv(uv);
    if (material.z > 0.5) {
        return scene;
    }

    let albedo = scene_albedo_at_uv(uv);
    let normal = scene_normal_at_uv(uv);
    let world_normal = safe_normalize((gi.inverse_view * vec4<f32>(normal, 0.0)).xyz);
    let center_pos = reconstruct_view_position(uv, center_depth);
    let world_pos = reconstruct_world_position(uv, center_depth);
    let probe = sample_probe_grid(world_pos, world_normal);
    let probe_gi = min(probe.irradiance, vec3<f32>(1.2));
    let detail_gi = screen_space_detail(uv, center_depth, normal);
    let ao = ambient_occlusion(uv, center_pos, normal, center_depth);
    let receiver_diffuse = 1.0 - saturate(material.x) * 0.9;
    let roughness = 0.38 + saturate(material.y) * 0.62;
    let hemi = hemisphere_ambient(normal);
    let hemi_weight = mix(0.10, 0.022, probe.enclosure);

    var indirect =
        probe_gi * gi.params0.y
        + detail_gi * gi.params0.z * mix(0.48, 0.92, probe.directionality)
        + hemi * hemi_weight;
    indirect *= gi.params0.x * receiver_diffuse * mix(0.18, 0.78, roughness);
    indirect *= mix(1.0, max(ao * ao, 0.22), gi.params0.w * mix(0.72, 1.0, probe.enclosure));

    let scene_ao = scene.rgb * mix(1.0, ao * ao, 0.34 + gi.params0.w * 0.56);
    let indirect_term = indirect * albedo;
    return vec4<f32>(scene_ao + indirect_term, scene.a);
}
