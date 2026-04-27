struct ViewUniform {
    view_proj: mat4x4<f32>,
    camera: vec4<f32>,
    viewport: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> camera: ViewUniform;

struct StandardUniform {
    albedo: vec4<f32>,
    emissive: vec4<f32>,
    params: vec4<f32>,
};

@group(1) @binding(0)
var<uniform> material: StandardUniform;

@group(1) @binding(1)
var t_albedo: texture_2d<f32>;
@group(1) @binding(2)
var s_albedo: sampler;
@group(1) @binding(3)
var t_emissive: texture_2d<f32>;
@group(1) @binding(4)
var t_normal: texture_2d<f32>;

struct ShadowUniform {
    light_view_proj: mat4x4<f32>,
    light_direction: vec4<f32>,
    shadow_params: vec4<f32>,
};

struct LightRecord {
    pos_radius: vec4<f32>,
    color: vec4<f32>,
    falloff: vec4<f32>,
};

struct LightTableMeta {
    count: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
};

@group(3) @binding(0)
var<storage, read> light_table: array<LightRecord>;
@group(3) @binding(1)
var<uniform> light_meta: LightTableMeta;
@group(3) @binding(2)
var<uniform> shadow: ShadowUniform;
@group(3) @binding(3)
var t_shadow: texture_depth_2d;
@group(3) @binding(4)
var s_shadow: sampler_comparison;

struct DdgiUniform {
    origin_spacing: vec4<f32>,
    counts_enabled: vec4<u32>,
    irradiance_atlas_params: vec4<u32>,
    visibility_atlas_params: vec4<u32>,
    trace_params: vec4<f32>,
    frame_params: vec4<u32>,
    ambient: vec4<f32>,
};

@group(3) @binding(5)
var<uniform> ddgi: DdgiUniform;
@group(3) @binding(6)
var t_ddgi_irradiance: texture_2d<f32>;
@group(3) @binding(7)
var t_ddgi_visibility: texture_2d<f32>;
@group(3) @binding(8)
var s_ddgi: sampler;

const DDGI_ATLAS_BORDER: u32 = 1u;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) tangent: vec4<f32>,
    @location(3) uv: vec2<f32>,
    @location(8) model_col0: vec4<f32>,
    @location(9) model_col1: vec4<f32>,
    @location(10) model_col2: vec4<f32>,
    @location(11) model_col3: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) tangent: vec3<f32>,
    @location(3) bitangent: vec3<f32>,
    @location(4) uv: vec2<f32>,
};

fn safe_normalize(v: vec3<f32>) -> vec3<f32> {
    let len_sq = dot(v, v);
    if (len_sq <= 0.000001) {
        return vec3<f32>(0.0, 0.0, 1.0);
    }
    return v * inverseSqrt(len_sq);
}

fn fresnel_schlick(cos_theta: f32, f0: vec3<f32>) -> vec3<f32> {
    let m = clamp(1.0 - cos_theta, 0.0, 1.0);
    let m2 = m * m;
    let m5 = m2 * m2 * m;
    return f0 + (vec3<f32>(1.0) - f0) * m5;
}

fn distribution_ggx(normal: vec3<f32>, half_dir: vec3<f32>, roughness: f32) -> f32 {
    let a = roughness * roughness;
    let a2 = a * a;
    let ndoth = max(dot(normal, half_dir), 0.0);
    let ndoth2 = ndoth * ndoth;
    let denom = ndoth2 * (a2 - 1.0) + 1.0;
    return a2 / max(3.14159265 * denom * denom, 0.0001);
}

fn geometry_schlick_ggx(ndotv: f32, roughness: f32) -> f32 {
    let r = roughness + 1.0;
    let k = (r * r) * 0.125;
    return ndotv / max(ndotv * (1.0 - k) + k, 0.0001);
}

fn geometry_smith(normal: vec3<f32>, view_dir: vec3<f32>, light_dir: vec3<f32>, roughness: f32) -> f32 {
    return geometry_schlick_ggx(max(dot(normal, view_dir), 0.0), roughness)
        * geometry_schlick_ggx(max(dot(normal, light_dir), 0.0), roughness);
}

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    let model = mat4x4<f32>(
        input.model_col0,
        input.model_col1,
        input.model_col2,
        input.model_col3,
    );
    let world_position = model * vec4<f32>(input.position, 1.0);
    let world_normal = normalize((model * vec4<f32>(input.normal, 0.0)).xyz);
    let world_tangent_raw = (model * vec4<f32>(input.tangent.xyz, 0.0)).xyz;
    let world_tangent = normalize(world_tangent_raw - world_normal * dot(world_normal, world_tangent_raw));
    let world_bitangent = normalize(cross(world_normal, world_tangent) * input.tangent.w);

    output.clip_position = camera.view_proj * world_position;
    output.world_position = world_position.xyz;
    output.normal = world_normal;
    output.tangent = world_tangent;
    output.bitangent = world_bitangent;
    output.uv = input.uv;
    return output;
}

fn shadow_visibility(world_position: vec3<f32>) -> f32 {
    if (shadow.shadow_params.w < 0.5) {
        return 1.0;
    }

    let light_clip = shadow.light_view_proj * vec4<f32>(world_position, 1.0);
    let inv_w = select(1.0, 1.0 / light_clip.w, abs(light_clip.w) > 0.00001);
    let light_ndc = light_clip.xyz * inv_w;
    let depth = light_ndc.z;
    if (depth <= 0.0 || depth >= 1.0) {
        return 1.0;
    }

    let uv = vec2<f32>(light_ndc.x * 0.5 + 0.5, light_ndc.y * -0.5 + 0.5);
    if (any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0))) {
        return 1.0;
    }

    let compare_depth = depth - shadow.shadow_params.x;
    let texel = shadow.shadow_params.y;
    let radius = shadow.shadow_params.z;
    if (texel <= 0.0 || radius <= 0.0) {
        return textureSampleCompare(t_shadow, s_shadow, uv, compare_depth);
    }

    var visibility = 0.0;
    for (var y = -1; y <= 1; y = y + 1) {
        for (var x = -1; x <= 1; x = x + 1) {
            let offset = vec2<f32>(f32(x), f32(y)) * texel * radius;
            visibility += textureSampleCompare(t_shadow, s_shadow, uv + offset, compare_depth);
        }
    }
    return visibility / 9.0;
}

fn sign_not_zero(v: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(
        select(-1.0, 1.0, v.x >= 0.0),
        select(-1.0, 1.0, v.y >= 0.0),
    );
}

fn oct_wrap(v: vec2<f32>) -> vec2<f32> {
    return (vec2<f32>(1.0) - abs(v.yx)) * sign_not_zero(v);
}

fn oct_encode(normal: vec3<f32>) -> vec2<f32> {
    let denom = max(abs(normal.x) + abs(normal.y) + abs(normal.z), 0.000001);
    var p = normal.xy / denom;
    if (normal.z < 0.0) {
        p = oct_wrap(p);
    }
    return p * 0.5 + vec2<f32>(0.5);
}

fn ddgi_probe_tile_resolution(resolution: u32) -> u32 {
    return max(1u, resolution) + DDGI_ATLAS_BORDER * 2u;
}

fn ddgi_probe_uv(coord: vec3<u32>, direction: vec3<f32>, atlas_params: vec4<u32>) -> vec2<f32> {
    let res = max(1u, atlas_params.z);
    let tile_res = ddgi_probe_tile_resolution(res);
    let cy = max(1u, ddgi.counts_enabled.y);
    let encoded = oct_encode(safe_normalize(direction));
    let local = clamp(
        encoded * f32(res),
        vec2<f32>(0.5),
        vec2<f32>(max(f32(res) - 0.5, 0.5)),
    );
    let pixel = vec2<f32>(
        f32(coord.x * tile_res + DDGI_ATLAS_BORDER) + local.x,
        f32((coord.z * cy + coord.y) * tile_res + DDGI_ATLAS_BORDER) + local.y,
    );
    let atlas_size = vec2<f32>(
        f32(max(1u, atlas_params.x)),
        f32(max(1u, atlas_params.y)),
    );
    return pixel / atlas_size;
}

fn ddgi_irradiance_probe(coord: vec3<u32>, normal: vec3<f32>) -> vec3<f32> {
    return textureSampleLevel(
        t_ddgi_irradiance,
        s_ddgi,
        ddgi_probe_uv(coord, normal, ddgi.irradiance_atlas_params),
        0.0,
    ).rgb;
}

fn ddgi_visibility_probe(coord: vec3<u32>, direction: vec3<f32>) -> vec2<f32> {
    return textureSampleLevel(
        t_ddgi_visibility,
        s_ddgi,
        ddgi_probe_uv(coord, direction, ddgi.visibility_atlas_params),
        0.0,
    ).rg;
}

fn ddgi_probe_world_position(coord: vec3<u32>) -> vec3<f32> {
    return ddgi.origin_spacing.xyz + vec3<f32>(coord) * max(ddgi.origin_spacing.w, 0.001);
}

fn ddgi_visibility_weight(coord: vec3<u32>, world_position: vec3<f32>, normal: vec3<f32>) -> f32 {
    let probe_position = ddgi_probe_world_position(coord);
    let to_surface = world_position - probe_position;
    let distance = length(to_surface);
    var direction = normal;
    if (distance > 0.0001) {
        direction = to_surface / distance;
    }
    let moments = ddgi_visibility_probe(coord, direction);
    let mean = max(moments.x, 0.0001);
    let mean_sq = max(moments.y, mean * mean);
    let variance = max(mean_sq - mean * mean, 0.0001);
    if (distance <= mean + ddgi.trace_params.w) {
        return 1.0;
    }
    let delta = distance - mean;
    return clamp(variance / (variance + delta * delta), 0.08, 1.0);
}

fn sample_ddgi(world_position: vec3<f32>, normal: vec3<f32>) -> vec3<f32> {
    if (ddgi.counts_enabled.w == 0u) {
        return vec3<f32>(0.0);
    }

    let spacing = max(ddgi.origin_spacing.w, 0.001);
    let counts_f = vec3<f32>(
        f32(max(1u, ddgi.counts_enabled.x)),
        f32(max(1u, ddgi.counts_enabled.y)),
        f32(max(1u, ddgi.counts_enabled.z)),
    );
    let local = (world_position + normal * ddgi.trace_params.z - ddgi.origin_spacing.xyz) / spacing;
    let clamped = clamp(local, vec3<f32>(0.0), max(counts_f - vec3<f32>(1.0), vec3<f32>(0.0)));
    let base_f = floor(clamped);
    let frac = clamped - base_f;
    let base = vec3<u32>(base_f);
    let max_coord = vec3<u32>(
        max(1u, ddgi.counts_enabled.x) - 1u,
        max(1u, ddgi.counts_enabled.y) - 1u,
        max(1u, ddgi.counts_enabled.z) - 1u,
    );

    let c000 = base;
    let c100 = min(base + vec3<u32>(1u, 0u, 0u), max_coord);
    let c010 = min(base + vec3<u32>(0u, 1u, 0u), max_coord);
    let c110 = min(base + vec3<u32>(1u, 1u, 0u), max_coord);
    let c001 = min(base + vec3<u32>(0u, 0u, 1u), max_coord);
    let c101 = min(base + vec3<u32>(1u, 0u, 1u), max_coord);
    let c011 = min(base + vec3<u32>(0u, 1u, 1u), max_coord);
    let c111 = min(base + vec3<u32>(1u, 1u, 1u), max_coord);

    let w000 = (1.0 - frac.x) * (1.0 - frac.y) * (1.0 - frac.z) * ddgi_visibility_weight(c000, world_position, normal);
    let w100 = frac.x * (1.0 - frac.y) * (1.0 - frac.z) * ddgi_visibility_weight(c100, world_position, normal);
    let w010 = (1.0 - frac.x) * frac.y * (1.0 - frac.z) * ddgi_visibility_weight(c010, world_position, normal);
    let w110 = frac.x * frac.y * (1.0 - frac.z) * ddgi_visibility_weight(c110, world_position, normal);
    let w001 = (1.0 - frac.x) * (1.0 - frac.y) * frac.z * ddgi_visibility_weight(c001, world_position, normal);
    let w101 = frac.x * (1.0 - frac.y) * frac.z * ddgi_visibility_weight(c101, world_position, normal);
    let w011 = (1.0 - frac.x) * frac.y * frac.z * ddgi_visibility_weight(c011, world_position, normal);
    let w111 = frac.x * frac.y * frac.z * ddgi_visibility_weight(c111, world_position, normal);

    let weighted =
        ddgi_irradiance_probe(c000, normal) * w000 +
        ddgi_irradiance_probe(c100, normal) * w100 +
        ddgi_irradiance_probe(c010, normal) * w010 +
        ddgi_irradiance_probe(c110, normal) * w110 +
        ddgi_irradiance_probe(c001, normal) * w001 +
        ddgi_irradiance_probe(c101, normal) * w101 +
        ddgi_irradiance_probe(c011, normal) * w011 +
        ddgi_irradiance_probe(c111, normal) * w111;
    let weight_sum = w000 + w100 + w010 + w110 + w001 + w101 + w011 + w111;
    return max(weighted / max(weight_sum, 0.0001), vec3<f32>(0.0));
}

fn ddgi_debug_mode() -> u32 {
    return ddgi.frame_params.w >> 16u;
}

fn ddgi_probe_count() -> u32 {
    return max(1u, ddgi.counts_enabled.x * ddgi.counts_enabled.y * ddgi.counts_enabled.z);
}

fn ddgi_probe_index(coord: vec3<u32>) -> u32 {
    return coord.x + coord.y * ddgi.counts_enabled.x + coord.z * ddgi.counts_enabled.x * ddgi.counts_enabled.y;
}

fn ddgi_nearest_probe_coord(world_position: vec3<f32>) -> vec3<u32> {
    let spacing = max(ddgi.origin_spacing.w, 0.001);
    let counts = vec3<u32>(
        max(1u, ddgi.counts_enabled.x),
        max(1u, ddgi.counts_enabled.y),
        max(1u, ddgi.counts_enabled.z),
    );
    let max_coord = vec3<f32>(
        f32(counts.x - 1u),
        f32(counts.y - 1u),
        f32(counts.z - 1u),
    );
    let local = (world_position - ddgi.origin_spacing.xyz) / spacing;
    return vec3<u32>(clamp(round(local), vec3<f32>(0.0), max_coord));
}

fn ddgi_probe_is_in_update_budget(index: u32) -> bool {
    let budget = max(1u, ddgi.frame_params.z);
    let count = ddgi_probe_count();
    if (budget >= count) {
        return true;
    }
    let start = (ddgi.frame_params.x * budget) % count;
    let end = start + budget;
    if (end < count) {
        return index >= start && index < end;
    }
    return index >= start || index < (end % count);
}

fn ddgi_debug_color(world_position: vec3<f32>, normal: vec3<f32>) -> vec3<f32> {
    let mode = ddgi_debug_mode();
    if (mode == 2u) {
        return sample_ddgi(world_position, normal);
    }

    let coord = ddgi_nearest_probe_coord(world_position + normal * ddgi.trace_params.z);
    let probe_position = ddgi_probe_world_position(coord);
    let to_probe = world_position - probe_position;
    let probe_distance = length(to_probe);
    let spacing = max(ddgi.origin_spacing.w, 0.001);
    let marker = 1.0 - smoothstep(spacing * 0.035, spacing * 0.12, probe_distance);

    if (mode == 1u) {
        let tint = 0.22 + 0.52 * fract(vec3<f32>(coord) * vec3<f32>(0.37, 0.53, 0.71));
        return mix(tint, vec3<f32>(1.0, 0.86, 0.18), marker);
    }

    var direction = normal;
    if (probe_distance > 0.0001) {
        direction = to_probe / probe_distance;
    }
    let visibility = ddgi_visibility_weight(coord, world_position, normal);
    if (mode == 3u) {
        let moments = ddgi_visibility_probe(coord, direction);
        let mean_distance = clamp(moments.x / max(ddgi.trace_params.x, 0.001), 0.0, 1.0);
        return vec3<f32>(1.0 - visibility, visibility, mean_distance);
    }

    if (mode == 4u) {
        let index = ddgi_probe_index(coord);
        var color = vec3<f32>(0.12, 0.22, 0.75);
        if (ddgi_probe_is_in_update_budget(index)) {
            color = vec3<f32>(1.0, 0.55, 0.08);
        }
        return mix(color, vec3<f32>(1.0), marker * 0.55);
    }

    return vec3<f32>(0.0);
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let base = textureSample(t_albedo, s_albedo, input.uv) * material.albedo;
    let sampled = textureSample(t_normal, s_albedo, input.uv).xyz * 2.0 - vec3<f32>(1.0);
    let normal = safe_normalize(
        sampled.x * input.tangent +
        sampled.y * input.bitangent +
        sampled.z * input.normal
    );
    if (ddgi_debug_mode() != 0u && ddgi.counts_enabled.w != 0u) {
        return vec4<f32>(ddgi_debug_color(input.world_position, normal), base.a);
    }
    let view_dir = safe_normalize(camera.camera.xyz - input.world_position);
    let metallic = clamp(material.params.x, 0.0, 1.0);
    let roughness = clamp(material.params.y, 0.05, 1.0);
    let f0 = mix(vec3<f32>(0.04), base.rgb, metallic);
    let ndotv = max(dot(normal, view_dir), 0.0);
    var lighting = vec3<f32>(0.0);

    for (var i: u32 = 0u; i < light_meta.count; i = i + 1u) {
        let light = light_table[i];
        if (light.falloff.y > 0.5) {
            let light_dir = safe_normalize(-light.pos_radius.xyz);
            let ndotl = max(dot(normal, light_dir), 0.0);
            if (ndotl <= 0.0001) {
                continue;
            }
            var shadow_term = 1.0;
            if (shadow.shadow_params.w > 0.5 &&
                dot(light_dir, normalize(-shadow.light_direction.xyz)) > 0.999) {
                shadow_term = shadow_visibility(input.world_position);
            }
            let half_dir = safe_normalize(light_dir + view_dir);
            let fresnel = fresnel_schlick(max(dot(half_dir, view_dir), 0.0), f0);
            let distribution = distribution_ggx(normal, half_dir, roughness);
            let geometry = geometry_smith(normal, view_dir, light_dir, roughness);
            let specular = fresnel * (distribution * geometry / max(4.0 * ndotv * ndotl, 0.0001));
            let diffuse = (vec3<f32>(1.0) - fresnel) * (1.0 - metallic) * base.rgb / 3.14159265;
            lighting += (diffuse + specular) * light.color.rgb * ndotl * shadow_term;
            continue;
        }

        let light_vector = light.pos_radius.xyz - input.world_position;
        let dist = length(light_vector);
        let radius = light.pos_radius.w;
        if (radius <= 0.0 || dist >= radius) {
            continue;
        }

        let attenuation = pow(
            max(1.0 - dist / radius, 0.0),
            max(light.falloff.x, 0.001),
        );
        let light_dir = light_vector / max(dist, 0.0001);
        let ndotl = max(dot(normal, light_dir), 0.0);
        if (ndotl <= 0.0001) {
            continue;
        }
        let half_dir = safe_normalize(light_dir + view_dir);
        let fresnel = fresnel_schlick(max(dot(half_dir, view_dir), 0.0), f0);
        let distribution = distribution_ggx(normal, half_dir, roughness);
        let geometry = geometry_smith(normal, view_dir, light_dir, roughness);
        let specular = fresnel * (distribution * geometry / max(4.0 * ndotv * ndotl, 0.0001));
        let diffuse = (vec3<f32>(1.0) - fresnel) * (1.0 - metallic) * base.rgb / 3.14159265;
        lighting += (diffuse + specular) * light.color.rgb * ndotl * attenuation;
    }

    let emissive_sample = textureSample(t_emissive, s_albedo, input.uv).rgb;
    let emissive = emissive_sample * material.emissive.rgb;
    var indirect = sample_ddgi(input.world_position, normal) * base.rgb * (1.0 - metallic);
    if (ddgi.counts_enabled.w == 0u) {
        indirect = base.rgb * mix(0.0015, 0.0065, roughness);
    }
    return vec4<f32>(lighting + indirect + emissive, base.a);
}
