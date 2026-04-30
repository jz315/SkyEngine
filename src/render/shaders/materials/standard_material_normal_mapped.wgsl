struct ViewUniform {
    view_proj: mat4x4<f32>,
    camera: vec4<f32>,
    viewport: vec4<f32>,
    view: mat4x4<f32>,
    projection: mat4x4<f32>,
    inverse_view: mat4x4<f32>,
    camera_position: vec4<f32>,
    near_far_time_delta: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> camera: ViewUniform;

struct StandardUniform {
    albedo: vec4<f32>,
    emissive: vec4<f32>,
    params: vec4<f32>,
    // x: receive shadows flag, y: alpha cutoff, z: alpha-test flag.
    shadow: vec4<f32>,
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
    light_view_proj: array<mat4x4<f32>, 4>,
    // xyz: normalized light direction, w: receiver normal bias.
    light_direction: vec4<f32>,
    cascade_splits: vec4<f32>,
    // x: compare bias, y: texel size, z: light radius, w: enabled flag.
    cascade_params: array<vec4<f32>, 4>,
    shadow_atlas_mul_add: vec4<f32>,
    // xy: atlas reciprocal resolution, z: guard-band texels, w: filter mode.
    shadow_atlas_resolution_rcp: vec4<f32>,
    // x: cascade count, y: cascade blend, z: coverage debug flag, w: enabled flag.
    shadow_params: vec4<f32>,
};

struct LightRecord {
    pos_radius: vec4<f32>,
    color: vec4<f32>,
    falloff: vec4<f32>,
    dir_shadow: vec4<f32>,
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
@group(3) @binding(9)
var t_shadow_transparent: texture_2d<f32>;
@group(3) @binding(10)
var s_shadow_transparent: sampler;

const DDGI_ATLAS_BORDER: u32 = 1u;
const SHADOW_CASCADE_MAX: u32 = 4u;
const SHADOW_FILTER_FIXED_PCF: u32 = 0u;
const SHADOW_FILTER_DITHERED_PCF: u32 = 1u;
const SHADOW_FILTER_PCSS: u32 = 2u;
const SHADOW_PCSS_BLOCKER_SAMPLES: u32 = 8u;
const LIGHT_KIND_POINT: u32 = 0u;
const LIGHT_KIND_DIRECTIONAL: u32 = 1u;
const LIGHT_KIND_SPOT: u32 = 2u;
const SHADOW_VOGEL_POINTS: array<vec2<f32>, 16> = array<vec2<f32>, 16>(
    vec2<f32>(0.17677665, 0.00000000),
    vec2<f32>(-0.22577983, 0.20681751),
    vec2<f32>(0.03458714, -0.39376867),
    vec2<f32>(0.28453016, 0.37120426),
    vec2<f32>(-0.52220953, -0.09245092),
    vec2<f32>(0.49475324, -0.31459379),
    vec2<f32>(-0.16560209, 0.61548841),
    vec2<f32>(-0.31540442, -0.60767603),
    vec2<f32>(0.68456841, 0.25023210),
    vec2<f32>(-0.71235347, 0.29377294),
    vec2<f32>(0.34362423, -0.73360229),
    vec2<f32>(0.25340176, 0.80903494),
    vec2<f32>(-0.76454973, -0.44352412),
    vec2<f32>(0.89722824, -0.19680285),
    vec2<f32>(-0.54790950, 0.77848911),
    vec2<f32>(-0.12594837, -0.97615927),
);

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

fn shadow_cascade_count() -> u32 {
    return clamp(u32(max(shadow.shadow_params.x, 1.0)), 1u, SHADOW_CASCADE_MAX);
}

fn shadow_cascade_index(view_depth: f32) -> u32 {
    let cascade_count = shadow_cascade_count();
    var cascade = 0u;
    for (var i = 0u; i < SHADOW_CASCADE_MAX; i = i + 1u) {
        if (i < cascade_count && view_depth > shadow.cascade_splits[i]) {
            cascade = min(i + 1u, cascade_count - 1u);
        }
    }
    return cascade;
}

fn shadow_cascade_edge_fade(light_ndc: vec3<f32>, blend_width: f32) -> f32 {
    if (blend_width <= 0.0) {
        return 0.0;
    }
    // WickedEngine fades near cascade projection edges:
    // saturate(saturate(abs(shadow_box)) - 0.8) * 5.
    let width = clamp(blend_width, 0.001, 0.5);
    let fade_start = 1.0 - width;
    let shadow_box = vec3<f32>(light_ndc.xy, light_ndc.z * 2.0 - 1.0);
    let edge = clamp((abs(shadow_box) - vec3<f32>(fade_start)) / width, vec3<f32>(0.0), vec3<f32>(1.0));
    return max(max(edge.x, edge.y), edge.z);
}

fn shadow_border_clamp(cascade: u32) -> vec4<f32> {
    // WickedEngine shadow_border_clamp(): clamp to the last texel center to avoid
    // filtering into neighboring atlas rects.
    let border_texels = max(0.75, shadow.shadow_atlas_resolution_rcp.z);
    let border_size = border_texels * shadow.shadow_atlas_resolution_rcp.xy;
    let top_left = vec2<f32>(f32(cascade), 0.0) * shadow.shadow_atlas_mul_add.xy
        + shadow.shadow_atlas_mul_add.zw
        + border_size;
    let bottom_right = vec2<f32>(f32(cascade + 1u), 1.0) * shadow.shadow_atlas_mul_add.xy
        + shadow.shadow_atlas_mul_add.zw
        - border_size;
    return vec4<f32>(top_left, bottom_right);
}

fn shadow_2d_uv(local_uv: vec2<f32>, cascade: u32) -> vec2<f32> {
    var shadow_uv = local_uv;
    shadow_uv.x = shadow_uv.x + f32(cascade);
    return shadow_uv * shadow.shadow_atlas_mul_add.xy + shadow.shadow_atlas_mul_add.zw;
}

fn shadow_sampling_mode() -> u32 {
    return min(u32(max(shadow.shadow_atlas_resolution_rcp.w, 0.0)), SHADOW_FILTER_PCSS);
}

fn shadow_interleaved_gradient_noise(pixel: vec2<f32>) -> f32 {
    return fract(52.9829189 * fract(dot(pixel, vec2<f32>(0.06711056, 0.00583715))));
}

fn shadow_rotate_disk(point: vec2<f32>, angle: f32) -> vec2<f32> {
    let s = sin(angle);
    let c = cos(angle);
    return vec2<f32>(point.x * c - point.y * s, point.x * s + point.y * c);
}

fn shadow_load_depth(sample_uv: vec2<f32>) -> f32 {
    let atlas_size_u = textureDimensions(t_shadow);
    let atlas_size = vec2<f32>(f32(atlas_size_u.x), f32(atlas_size_u.y));
    let clamped = clamp(sample_uv * atlas_size, vec2<f32>(0.0), atlas_size - vec2<f32>(1.0));
    return textureLoad(t_shadow, vec2<i32>(floor(clamped)), 0);
}

fn shadow_filter_pcf(
    uv: vec2<f32>,
    compare_depth: f32,
    spread: vec2<f32>,
    uv_min: vec2<f32>,
    uv_max: vec2<f32>,
    rotation: f32,
) -> vec3<f32> {
    var visibility = vec3<f32>(0.0);
    for (var i = 0u; i < 16u; i = i + 1u) {
        let disk = shadow_rotate_disk(SHADOW_VOGEL_POINTS[i], rotation);
        let sample_uv = clamp(uv + disk * spread, uv_min, uv_max);
        let pcf = textureSampleCompare(t_shadow, s_shadow, sample_uv, compare_depth);
        visibility += shadow_apply_transparent(sample_uv, compare_depth, pcf);
    }
    return visibility / 16.0;
}

fn shadow_find_blocker(
    uv: vec2<f32>,
    compare_depth: f32,
    spread: vec2<f32>,
    uv_min: vec2<f32>,
    uv_max: vec2<f32>,
    rotation: f32,
) -> vec2<f32> {
    var average_blocker = 0.0;
    var blocker_count = 0.0;
    for (var i = 0u; i < SHADOW_PCSS_BLOCKER_SAMPLES; i = i + 1u) {
        let disk = shadow_rotate_disk(SHADOW_VOGEL_POINTS[i], rotation);
        let sample_uv = clamp(uv + disk * spread, uv_min, uv_max);
        let sample_depth = shadow_load_depth(sample_uv);
        if (sample_depth < compare_depth) {
            average_blocker += sample_depth;
            blocker_count += 1.0;
        }
    }
    return vec2<f32>(average_blocker / max(blocker_count, 1.0), blocker_count);
}

fn shadow_filter_pcss(
    uv: vec2<f32>,
    compare_depth: f32,
    light_radius: f32,
    uv_min: vec2<f32>,
    uv_max: vec2<f32>,
    rotation: f32,
) -> vec3<f32> {
    // Wicked-style PCSS shape: search for blockers first, then grow the
    // filtering radius from receiver/blocker separation. Fixed PCF remains the
    // default because this is deliberately heavier.
    let base_filter_texels = max(light_radius, 0.0) * 8.0 + 2.0;
    let blocker_spread = shadow.shadow_atlas_resolution_rcp.xy * (base_filter_texels * 1.5);
    let blocker = shadow_find_blocker(uv, compare_depth, blocker_spread, uv_min, uv_max, rotation);
    if (blocker.y < 0.5) {
        return vec3<f32>(1.0);
    }

    let receiver_gap = max(compare_depth - blocker.x, 0.0);
    let penumbra_texels = clamp(
        base_filter_texels + receiver_gap * max(light_radius, 0.001) * 512.0,
        1.0,
        48.0,
    );
    let filter_spread = shadow.shadow_atlas_resolution_rcp.xy * penumbra_texels;
    return shadow_filter_pcf(uv, compare_depth, filter_spread, uv_min, uv_max, rotation);
}

fn shadow_apply_transparent(sample_uv: vec2<f32>, compare_depth: f32, pcf: f32) -> vec3<f32> {
    var visibility = vec3<f32>(pcf);
    // Wicked's transparent shadow map is sampled independently from the depth
    // map inside the PCF loop. The transparent pass stores a reversed
    // secondary-depth key so this remains the same `a > cmp` shape as Wicked.
    let transparent_shadow = textureSampleLevel(t_shadow_transparent, s_shadow_transparent, sample_uv, 0.0);
    let secondary_depth_key = 1.0 - compare_depth;
    if (transparent_shadow.a > secondary_depth_key) {
        visibility *= clamp(transparent_shadow.rgb, vec3<f32>(0.0), vec3<f32>(1.0));
    }
    return visibility;
}

fn shadow_sample_cascade(receiver_position: vec3<f32>, cascade: u32) -> vec3<f32> {
    let cascade_params = shadow.cascade_params[cascade];
    if (cascade_params.w < 0.5) {
        return vec3<f32>(1.0);
    }

    let light_clip = shadow.light_view_proj[cascade] * vec4<f32>(receiver_position, 1.0);
    let inv_w = select(1.0, 1.0 / light_clip.w, abs(light_clip.w) > 0.00001);
    let light_ndc = light_clip.xyz * inv_w;
    let depth = light_ndc.z;
    if (depth <= 0.0 || depth >= 1.0) {
        return vec3<f32>(1.0);
    }

    let local_uv = vec2<f32>(light_ndc.x * 0.5 + 0.5, light_ndc.y * -0.5 + 0.5);
    if (any(local_uv < vec2<f32>(0.0)) || any(local_uv > vec2<f32>(1.0))) {
        return vec3<f32>(1.0);
    }

    let uv = shadow_2d_uv(local_uv, cascade);
    let compare_depth = depth - cascade_params.x;
    let light_radius = cascade_params.z;
    if (shadow.shadow_atlas_resolution_rcp.x <= 0.0 || shadow.shadow_atlas_resolution_rcp.y <= 0.0) {
        let pcf = textureSampleCompare(t_shadow, s_shadow, uv, compare_depth);
        return shadow_apply_transparent(uv, compare_depth, pcf);
    }

    // Ported from WickedEngine's MIT-licensed shadowHF.hlsli shape:
    // 16-sample Vogel disk, border clamping, optional dither rotation, and
    // optional PCSS blocker search.
    let border = shadow_border_clamp(cascade);
    let uv_min = border.xy;
    let uv_max = border.zw;
    let mode = shadow_sampling_mode();
    let atlas_pixel = uv / max(shadow.shadow_atlas_resolution_rcp.xy, vec2<f32>(0.000001));
    let random_angle = shadow_interleaved_gradient_noise(atlas_pixel + vec2<f32>(f32(cascade) * 19.19, 0.0)) * 6.28318531;
    let rotation = select(0.0, random_angle, mode != SHADOW_FILTER_FIXED_PCF);
    if (mode == SHADOW_FILTER_PCSS) {
        return shadow_filter_pcss(uv, compare_depth, light_radius, uv_min, uv_max, rotation);
    }

    let spread = shadow.shadow_atlas_resolution_rcp.xy * (max(light_radius, 0.0) * 8.0 + 2.0);
    return shadow_filter_pcf(uv, compare_depth, spread, uv_min, uv_max, rotation);
}

fn shadow_transmittance(world_position: vec3<f32>, normal: vec3<f32>) -> vec3<f32> {
    if (shadow.shadow_params.w < 0.5 || material.shadow.x < 0.5) {
        return vec3<f32>(1.0);
    }

    let receiver_position = world_position + safe_normalize(normal) * max(shadow.light_direction.w, 0.0);
    let receiver_view = camera.view * vec4<f32>(receiver_position, 1.0);
    let cascade = shadow_cascade_index(max(-receiver_view.z, 0.0));
    let transmittance = shadow_sample_cascade(receiver_position, cascade);
    let cascade_count = shadow_cascade_count();
    if (cascade + 1u >= cascade_count) {
        return transmittance;
    }

    let light_clip = shadow.light_view_proj[cascade] * vec4<f32>(receiver_position, 1.0);
    let inv_w = select(1.0, 1.0 / light_clip.w, abs(light_clip.w) > 0.00001);
    let light_ndc = light_clip.xyz * inv_w;
    let cascade_fade = shadow_cascade_edge_fade(light_ndc, shadow.shadow_params.y);
    if (cascade_fade <= 0.0) {
        return transmittance;
    }
    let fallback_transmittance = shadow_sample_cascade(receiver_position, cascade + 1u);
    return mix(transmittance, fallback_transmittance, cascade_fade);
}

fn shadow_cascade_debug_tint(cascade: u32) -> vec3<f32> {
    if (cascade == 0u) {
        return vec3<f32>(0.12, 0.62, 1.0);
    }
    if (cascade == 1u) {
        return vec3<f32>(0.18, 0.95, 0.36);
    }
    if (cascade == 2u) {
        return vec3<f32>(1.0, 0.76, 0.16);
    }
    return vec3<f32>(1.0, 0.24, 0.46);
}

fn shadow_cascade_coverage_color(world_position: vec3<f32>, normal: vec3<f32>) -> vec3<f32> {
    if (shadow.shadow_params.w < 0.5 || material.shadow.x < 0.5) {
        return vec3<f32>(0.0);
    }

    let receiver_position = world_position + safe_normalize(normal) * max(shadow.light_direction.w, 0.0);
    // View-space depth is max(-z, 0). Split distances are camera view-distance units.
    let receiver_view = camera.view * vec4<f32>(receiver_position, 1.0);
    let cascade = shadow_cascade_index(max(-receiver_view.z, 0.0));
    let tint = shadow_cascade_debug_tint(cascade);
    let cascade_count = shadow_cascade_count();
    if (cascade + 1u >= cascade_count) {
        return tint;
    }

    let light_clip = shadow.light_view_proj[cascade] * vec4<f32>(receiver_position, 1.0);
    let inv_w = select(1.0, 1.0 / light_clip.w, abs(light_clip.w) > 0.00001);
    let light_ndc = light_clip.xyz * inv_w;
    let cascade_fade = shadow_cascade_edge_fade(light_ndc, shadow.shadow_params.y);
    let blended = mix(tint, shadow_cascade_debug_tint(cascade + 1u), cascade_fade);
    return mix(blended, vec3<f32>(1.0), cascade_fade * 0.18);
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
    if (material.shadow.z > 0.5 && base.a < material.shadow.y) {
        discard;
    }
    let sampled = textureSample(t_normal, s_albedo, input.uv).xyz * 2.0 - vec3<f32>(1.0);
    let normal = safe_normalize(
        sampled.x * input.tangent +
        sampled.y * input.bitangent +
        sampled.z * input.normal
    );
    if (shadow.shadow_params.z > 0.5 && material.shadow.x > 0.5) {
        return vec4<f32>(shadow_cascade_coverage_color(input.world_position, normal), base.a);
    }
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
        let light_kind = u32(round(light.falloff.y));
        if (light_kind == LIGHT_KIND_DIRECTIONAL) {
            let light_dir = safe_normalize(-light.pos_radius.xyz);
            let ndotl = max(dot(normal, light_dir), 0.0);
            if (ndotl <= 0.0001) {
                continue;
            }
            var shadow_term = vec3<f32>(1.0);
            if (shadow.shadow_params.w > 0.5 &&
                material.shadow.x > 0.5 &&
                dot(light_dir, normalize(-shadow.light_direction.xyz)) > 0.999) {
                shadow_term = shadow_transmittance(input.world_position, normal);
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
        var spot_term = 1.0;
        if (light_kind == LIGHT_KIND_SPOT) {
            let spot_direction = safe_normalize(light.dir_shadow.xyz);
            let cone_cos = dot(spot_direction, -light_dir);
            let inner_cos = light.falloff.z;
            let outer_cos = light.falloff.w;
            spot_term = clamp((cone_cos - outer_cos) / max(inner_cos - outer_cos, 0.0001), 0.0, 1.0);
            if (spot_term <= 0.0001) {
                continue;
            }
            spot_term = spot_term * spot_term;
        }
        let half_dir = safe_normalize(light_dir + view_dir);
        let fresnel = fresnel_schlick(max(dot(half_dir, view_dir), 0.0), f0);
        let distribution = distribution_ggx(normal, half_dir, roughness);
        let geometry = geometry_smith(normal, view_dir, light_dir, roughness);
        let specular = fresnel * (distribution * geometry / max(4.0 * ndotv * ndotl, 0.0001));
        let diffuse = (vec3<f32>(1.0) - fresnel) * (1.0 - metallic) * base.rgb / 3.14159265;
        lighting += (diffuse + specular) * light.color.rgb * ndotl * attenuation * spot_term;
    }

    let emissive_sample = textureSample(t_emissive, s_albedo, input.uv).rgb;
    let emissive = emissive_sample * material.emissive.rgb;
    var indirect = sample_ddgi(input.world_position, normal) * base.rgb * (1.0 - metallic);
    if (ddgi.counts_enabled.w == 0u) {
        indirect = base.rgb * mix(0.0015, 0.0065, roughness);
    }
    return vec4<f32>(lighting + indirect + emissive, base.a);
}
