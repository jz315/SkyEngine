pub(crate) const DDGI_SAMPLING_SHADER: &str = r#"
struct GiDdgiUniform {
    origin_spacing: vec4<f32>,
    counts_enabled: vec4<u32>,
    irradiance_atlas_params: vec4<u32>,
    visibility_atlas_params: vec4<u32>,
    trace_params: vec4<f32>,
    frame_params: vec4<u32>,
    ambient: vec4<f32>,
};

@group(2) @binding(0)
var<uniform> gi_ddgi: GiDdgiUniform;
@group(2) @binding(1)
var t_gi_ddgi_irradiance: texture_2d<f32>;
@group(2) @binding(2)
var t_gi_ddgi_visibility: texture_2d<f32>;
@group(2) @binding(3)
var s_gi_ddgi: sampler;

const GI_DDGI_ATLAS_BORDER: u32 = 1u;

fn gi_ddgi_probe_tile_resolution(resolution: u32) -> u32 {
    return max(1u, resolution) + GI_DDGI_ATLAS_BORDER * 2u;
}

fn gi_ddgi_probe_uv(coord: vec3<u32>, direction: vec3<f32>, atlas_params: vec4<u32>) -> vec2<f32> {
    let res = max(1u, atlas_params.z);
    let tile_res = gi_ddgi_probe_tile_resolution(res);
    let cy = max(1u, gi_ddgi.counts_enabled.y);
    let oct = oct_encode(direction);
    let local = (oct * 0.5 + vec2<f32>(0.5)) * f32(max(1u, res) - 1u);
    let pixel = vec2<f32>(
        f32(coord.x * tile_res + GI_DDGI_ATLAS_BORDER) + local.x,
        f32((coord.z * cy + coord.y) * tile_res + GI_DDGI_ATLAS_BORDER) + local.y,
    );
    return (pixel + vec2<f32>(0.5)) / vec2<f32>(
        f32(max(1u, atlas_params.x)),
        f32(max(1u, atlas_params.y)),
    );
}

fn gi_ddgi_irradiance_probe(coord: vec3<u32>, normal: vec3<f32>) -> vec3<f32> {
    return max(
        textureSampleLevel(
            t_gi_ddgi_irradiance,
            s_gi_ddgi,
            gi_ddgi_probe_uv(coord, normal, gi_ddgi.irradiance_atlas_params),
            0.0,
        ).rgb,
        vec3<f32>(0.0),
    );
}

fn gi_ddgi_visibility_probe(coord: vec3<u32>, direction: vec3<f32>) -> vec2<f32> {
    return textureSampleLevel(
        t_gi_ddgi_visibility,
        s_gi_ddgi,
        gi_ddgi_probe_uv(coord, direction, gi_ddgi.visibility_atlas_params),
        0.0,
    ).rg;
}

fn gi_ddgi_probe_world_position(coord: vec3<u32>) -> vec3<f32> {
    return gi_ddgi.origin_spacing.xyz + vec3<f32>(coord) * max(gi_ddgi.origin_spacing.w, 0.001);
}

fn gi_ddgi_visibility_weight(coord: vec3<u32>, world_position: vec3<f32>, normal: vec3<f32>) -> f32 {
    let probe_position = gi_ddgi_probe_world_position(coord);
    let to_point = world_position - probe_position;
    let distance = length(to_point);
    if (distance <= 0.0001) {
        return 1.0;
    }
    let direction = to_point / distance;
    let moments = gi_ddgi_visibility_probe(coord, direction);
    let mean = moments.x;
    let variance = max(moments.y - mean * mean, 0.0001);
    if (distance <= mean + gi_ddgi.trace_params.w) {
        return max(dot(normal, -direction), 0.1);
    }
    let chebyshev = variance / (variance + (distance - mean) * (distance - mean));
    return clamp(chebyshev * max(dot(normal, -direction), 0.1), 0.0, 1.0);
}

fn gi_ddgi_sample(world_position: vec3<f32>, normal: vec3<f32>) -> vec3<f32> {
    if (gi_ddgi.counts_enabled.w == 0u) {
        return vec3<f32>(0.0);
    }
    let spacing = max(gi_ddgi.origin_spacing.w, 0.001);
    let counts = vec3<f32>(
        f32(max(1u, gi_ddgi.counts_enabled.x)),
        f32(max(1u, gi_ddgi.counts_enabled.y)),
        f32(max(1u, gi_ddgi.counts_enabled.z)),
    );
    let local = (world_position + normal * gi_ddgi.trace_params.z - gi_ddgi.origin_spacing.xyz) / spacing;
    let clamped = clamp(local, vec3<f32>(0.0), counts - vec3<f32>(1.0));
    let base_coord = vec3<u32>(floor(clamped));
    let next_coord = min(
        base_coord + vec3<u32>(1u),
        vec3<u32>(
            max(1u, gi_ddgi.counts_enabled.x) - 1u,
            max(1u, gi_ddgi.counts_enabled.y) - 1u,
            max(1u, gi_ddgi.counts_enabled.z) - 1u,
        ),
    );
    let frac = fract(clamped);
    let c000 = vec3<u32>(base_coord.x, base_coord.y, base_coord.z);
    let c100 = vec3<u32>(next_coord.x, base_coord.y, base_coord.z);
    let c010 = vec3<u32>(base_coord.x, next_coord.y, base_coord.z);
    let c110 = vec3<u32>(next_coord.x, next_coord.y, base_coord.z);
    let c001 = vec3<u32>(base_coord.x, base_coord.y, next_coord.z);
    let c101 = vec3<u32>(next_coord.x, base_coord.y, next_coord.z);
    let c011 = vec3<u32>(base_coord.x, next_coord.y, next_coord.z);
    let c111 = vec3<u32>(next_coord.x, next_coord.y, next_coord.z);
    let w000 = (1.0 - frac.x) * (1.0 - frac.y) * (1.0 - frac.z) * gi_ddgi_visibility_weight(c000, world_position, normal);
    let w100 = frac.x * (1.0 - frac.y) * (1.0 - frac.z) * gi_ddgi_visibility_weight(c100, world_position, normal);
    let w010 = (1.0 - frac.x) * frac.y * (1.0 - frac.z) * gi_ddgi_visibility_weight(c010, world_position, normal);
    let w110 = frac.x * frac.y * (1.0 - frac.z) * gi_ddgi_visibility_weight(c110, world_position, normal);
    let w001 = (1.0 - frac.x) * (1.0 - frac.y) * frac.z * gi_ddgi_visibility_weight(c001, world_position, normal);
    let w101 = frac.x * (1.0 - frac.y) * frac.z * gi_ddgi_visibility_weight(c101, world_position, normal);
    let w011 = (1.0 - frac.x) * frac.y * frac.z * gi_ddgi_visibility_weight(c011, world_position, normal);
    let w111 = frac.x * frac.y * frac.z * gi_ddgi_visibility_weight(c111, world_position, normal);
    let accum =
        gi_ddgi_irradiance_probe(c000, normal) * w000 +
        gi_ddgi_irradiance_probe(c100, normal) * w100 +
        gi_ddgi_irradiance_probe(c010, normal) * w010 +
        gi_ddgi_irradiance_probe(c110, normal) * w110 +
        gi_ddgi_irradiance_probe(c001, normal) * w001 +
        gi_ddgi_irradiance_probe(c101, normal) * w101 +
        gi_ddgi_irradiance_probe(c011, normal) * w011 +
        gi_ddgi_irradiance_probe(c111, normal) * w111;
    let weight = max(w000 + w100 + w010 + w110 + w001 + w101 + w011 + w111, 0.0001);
    return accum / weight;
}

fn gi_debug_mode() -> u32 {
    return gi_ddgi.frame_params.w >> 16u;
}

fn gi_ddgi_probe_count() -> u32 {
    return max(1u, gi_ddgi.counts_enabled.x * gi_ddgi.counts_enabled.y * gi_ddgi.counts_enabled.z);
}

fn gi_ddgi_probe_index(coord: vec3<u32>) -> u32 {
    return coord.x + coord.y * gi_ddgi.counts_enabled.x + coord.z * gi_ddgi.counts_enabled.x * gi_ddgi.counts_enabled.y;
}

fn gi_ddgi_nearest_probe_coord(world_position: vec3<f32>) -> vec3<u32> {
    let spacing = max(gi_ddgi.origin_spacing.w, 0.001);
    let counts = vec3<f32>(
        f32(max(1u, gi_ddgi.counts_enabled.x)),
        f32(max(1u, gi_ddgi.counts_enabled.y)),
        f32(max(1u, gi_ddgi.counts_enabled.z)),
    );
    let local = (world_position - gi_ddgi.origin_spacing.xyz) / spacing;
    return vec3<u32>(clamp(round(local), vec3<f32>(0.0), counts - vec3<f32>(1.0)));
}

fn gi_ddgi_probe_is_in_update_budget(index: u32) -> bool {
    let budget = max(1u, gi_ddgi.frame_params.z);
    let count = gi_ddgi_probe_count();
    if (budget >= count) {
        return true;
    }
    let start = (gi_ddgi.frame_params.x * budget) % count;
    let end = start + budget;
    if (end <= count) {
        return index >= start && index < end;
    }
    return index >= start || index < (end % count);
}

fn gi_debug_color(world_position: vec3<f32>, normal: vec3<f32>) -> vec3<f32> {
    let mode = gi_debug_mode();
    if (mode == 2u) {
        return gi_ddgi_sample(world_position, normal);
    }
    let coord = gi_ddgi_nearest_probe_coord(world_position + normal * gi_ddgi.trace_params.z);
    let probe_position = gi_ddgi_probe_world_position(coord);
    let dist = distance(world_position, probe_position);
    let spacing = max(gi_ddgi.origin_spacing.w, 0.001);
    if (mode == 1u) {
        let radius = spacing * 0.14;
        let shell = smoothstep(radius, radius * 0.55, dist);
        return mix(vec3<f32>(0.02, 0.03, 0.04), vec3<f32>(0.1, 0.75, 1.0), shell);
    }
    if (mode == 3u) {
        let direction = safe_normalize(world_position - probe_position);
        let moments = gi_ddgi_visibility_probe(coord, direction);
        let mean_distance = clamp(moments.x / max(gi_ddgi.trace_params.x, 0.001), 0.0, 1.0);
        let visibility = gi_ddgi_visibility_weight(coord, world_position, normal);
        return vec3<f32>(visibility, mean_distance, moments.y / max(gi_ddgi.trace_params.x * gi_ddgi.trace_params.x, 0.001));
    }
    if (mode == 4u) {
        let index = gi_ddgi_probe_index(coord);
        if (gi_ddgi_probe_is_in_update_budget(index)) {
            return vec3<f32>(1.0, 0.65, 0.08);
        }
        return vec3<f32>(0.025, 0.04, 0.07);
    }
    return vec3<f32>(0.0);
}

fn gi_sample_indirect_diffuse(
    world_position: vec3<f32>,
    normal: vec3<f32>,
    base_color: vec3<f32>,
    metallic: f32,
    roughness: f32,
) -> vec3<f32> {
    let sampled = gi_ddgi_sample(world_position, normal) * base_color * (1.0 - metallic);
    if (gi_ddgi.counts_enabled.w == 0u) {
        return base_color * (1.0 - metallic) * mix(0.0015, 0.0065, roughness);
    }
    return sampled;
}
"#;
