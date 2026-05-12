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
    // x: compare bias, y: world units per texel, z: Wicked-style filter radius, w: light depth range.
    cascade_params: array<vec4<f32>, 4>,
    shadow_atlas_mul_add: vec4<f32>,
    // xy: atlas reciprocal resolution, z: guard-band texels, w: filter mode.
    shadow_atlas_resolution_rcp: vec4<f32>,
    // x: cascade count, y: cascade blend, z: material shadow debug mode,
    // w: enabled plus temporal rotation seed in the fractional part.
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

@group(3) @binding(5)
var t_shadow_transparent: texture_2d<f32>;
@group(3) @binding(6)
var s_shadow_transparent: sampler;

const SHADOW_CASCADE_MAX: u32 = 4u;
const SHADOW_FILTER_FIXED_PCF: u32 = 0u;
const SHADOW_FILTER_DITHERED_PCF: u32 = 1u;
const SHADOW_FILTER_PCSS: u32 = 2u;
const SHADOW_PCSS_BLOCKER_GRID_RADIUS: i32 = 2;
const SHADOW_PCSS_BLOCKER_TAP_COUNT: f32 = 100.0;
const SHADOW_PCF_STEPS_MIN: u32 = 10u;
const SHADOW_PCF_STEPS_MAX: u32 = 16u;
const SHADOW_PCSS_MAX_FILTER_TEXELS: f32 = 36.0;
const SHADOW_FILTER_RADIUS_SCALE: f32 = 8.0;
const SHADOW_FILTER_RADIUS_BIAS: f32 = 2.0;
const SHADOW_PCSS_BLOCKER_GRID_STEP_SCALE: f32 = 4.0;
const SHADOW_PCSS_BLOCKER_DEPTH_BIAS_TEXELS: f32 = 6.0;
const SHADOW_PCSS_PENUMBRA_SCALE: f32 = 200.0;
const SHADOW_PCSS_MAX_PENUMBRA_SCALE: f32 = 4.0;
const SHADOW_RECEIVER_NORMAL_TEXEL_BIAS: f32 = 0.25;
const SHADOW_COMPARE_TEXEL_BIAS_BASE: f32 = 0.25;
const SHADOW_COMPARE_TEXEL_BIAS_SLOPE: f32 = 0.75;
const SHADOW_MATERIAL_COMPARE_BIAS_SCALE: f32 = 0.0;
const SHADOW_DEBUG_NONE: u32 = 0u;
const SHADOW_DEBUG_SAMPLED_CASCADE: u32 = 1u;
const SHADOW_DEBUG_SPLIT_COVERAGE: u32 = 2u;
const SHADOW_DEBUG_FADE: u32 = 3u;
const SHADOW_DEBUG_COMPARE_DELTA: u32 = 4u;
const SHADOW_DEBUG_BIAS: u32 = 5u;
const SHADOW_DEBUG_PCSS: u32 = 6u;
const SHADOW_DEBUG_DIRECT_LIGHTING: u32 = 7u;
const SHADOW_DEBUG_INDIRECT_LIGHTING: u32 = 8u;
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

fn shadow_debug_mode() -> u32 {
    return u32(clamp(round(shadow.shadow_params.z), 0.0, 8.0));
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

fn shadow_receiver_view_depth(world_position: vec3<f32>) -> f32 {
    let view_position = camera.view * vec4<f32>(world_position, 1.0);
    return max(-view_position.z, 0.0);
}

struct ShadowCascadeProjection {
    light_ndc: vec3<f32>,
    local_uv: vec2<f32>,
    depth: f32,
    inside: f32,
}

fn shadow_project_cascade(receiver_position: vec3<f32>, cascade: u32) -> ShadowCascadeProjection {
    let light_clip = shadow.light_view_proj[cascade] * vec4<f32>(receiver_position, 1.0);
    let inv_w = select(1.0, 1.0 / light_clip.w, abs(light_clip.w) > 0.00001);
    let light_ndc = light_clip.xyz * inv_w;
    let local_uv = vec2<f32>(light_ndc.x * 0.5 + 0.5, light_ndc.y * -0.5 + 0.5);
    let inside = shadow.cascade_params[cascade].w > 0.0
        && light_ndc.z > 0.0
        && light_ndc.z < 1.0
        && all(local_uv >= vec2<f32>(0.0))
        && all(local_uv <= vec2<f32>(1.0));
    return ShadowCascadeProjection(light_ndc, local_uv, light_ndc.z, select(0.0, 1.0, inside));
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

fn shadow_temporal_rotation_seed() -> f32 {
    return fract(max(shadow.shadow_params.w, 0.0));
}

fn shadow_filter_radius_texels(light_radius: f32) -> f32 {
    if (light_radius <= 0.0) {
        return 0.0;
    }
    return clamp(
        light_radius * SHADOW_FILTER_RADIUS_SCALE + SHADOW_FILTER_RADIUS_BIAS,
        SHADOW_FILTER_RADIUS_BIAS,
        SHADOW_PCSS_MAX_FILTER_TEXELS,
    );
}

fn shadow_receiver_ndotl(normal: vec3<f32>) -> f32 {
    let light_to_receiver = safe_normalize(-shadow.light_direction.xyz);
    return clamp(dot(safe_normalize(normal), light_to_receiver), 0.0, 1.0);
}

fn shadow_receiver_bias_world(normal: vec3<f32>, cascade: u32) -> f32 {
    let texel_world_size = max(shadow.cascade_params[cascade].y, 0.000001);
    let ndotl = shadow_receiver_ndotl(normal);
    let angle_scale = 1.0 - ndotl;
    let texel_bias = texel_world_size * SHADOW_RECEIVER_NORMAL_TEXEL_BIAS * angle_scale;
    return max(max(shadow.light_direction.w, 0.0), texel_bias);
}

fn shadow_compare_depth(cascade: u32, depth: f32, normal: vec3<f32>) -> f32 {
    return clamp(depth - shadow_compare_bias_depth(cascade, normal), 0.0, 1.0);
}

fn shadow_compare_bias_depth(cascade: u32, normal: vec3<f32>) -> f32 {
    let cascade_params = shadow.cascade_params[cascade];
    let texel_world_size = max(cascade_params.y, 0.000001);
    let depth_range = max(cascade_params.w, texel_world_size);
    let ndotl = shadow_receiver_ndotl(normal);
    let texel_depth_bias = (texel_world_size / depth_range)
        * (SHADOW_COMPARE_TEXEL_BIAS_BASE + SHADOW_COMPARE_TEXEL_BIAS_SLOPE * (1.0 - ndotl));
    let normal_depth_bias = shadow_receiver_bias_world(normal, cascade) / depth_range;
    return max(max(cascade_params.x * SHADOW_MATERIAL_COMPARE_BIAS_SCALE, texel_depth_bias), normal_depth_bias);
}

fn shadow_pcss_blocker_depth_bias(cascade: u32) -> f32 {
    let cascade_params = shadow.cascade_params[cascade];
    let texel_world_size = max(cascade_params.y, 0.000001);
    let depth_range = max(cascade_params.w, texel_world_size);
    return (texel_world_size / depth_range) * SHADOW_PCSS_BLOCKER_DEPTH_BIAS_TEXELS;
}

fn shadow_receiver_plane_depth_gradient(cascade: u32, world_dx: vec3<f32>, world_dy: vec3<f32>) -> vec2<f32> {
    let light_dx = shadow.light_view_proj[cascade] * vec4<f32>(world_dx, 0.0);
    let light_dy = shadow.light_view_proj[cascade] * vec4<f32>(world_dy, 0.0);
    let uv_dx = vec2<f32>(light_dx.x * 0.5, light_dx.y * -0.5) * shadow.shadow_atlas_mul_add.xy;
    let uv_dy = vec2<f32>(light_dy.x * 0.5, light_dy.y * -0.5) * shadow.shadow_atlas_mul_add.xy;
    let det = uv_dx.x * uv_dy.y - uv_dx.y * uv_dy.x;
    if (abs(det) <= 0.00000001) {
        return vec2<f32>(0.0);
    }
    return vec2<f32>(
        (light_dx.z * uv_dy.y - light_dy.z * uv_dx.y) / det,
        (uv_dx.x * light_dy.z - uv_dy.x * light_dx.z) / det,
    );
}

fn shadow_receiver_plane_compare_depth(
    center_uv: vec2<f32>,
    sample_uv: vec2<f32>,
    compare_depth: f32,
    depth_gradient: vec2<f32>,
) -> f32 {
    let plane_bias = dot(depth_gradient, sample_uv - center_uv);
    return clamp(compare_depth + clamp(plane_bias, -0.01, 0.01), 0.0, 1.0);
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

fn shadow_load_compare_depth(sample_uv: vec2<f32>, cascade: u32) -> f32 {
    let border = shadow_border_clamp(cascade);
    return shadow_load_depth(clamp(sample_uv, border.xy, border.zw));
}

fn shadow_filter_pcf(
    uv: vec2<f32>,
    compare_depth: f32,
    depth_gradient: vec2<f32>,
    spread: vec2<f32>,
    uv_min: vec2<f32>,
    uv_max: vec2<f32>,
    rotation: f32,
    step_count: u32,
) -> vec3<f32> {
    var visibility = vec3<f32>(0.0);
    let steps = clamp(step_count, 1u, 16u);
    for (var i = 0u; i < 16u; i = i + 1u) {
        if (i >= steps) {
            break;
        }
        let disk = shadow_rotate_disk(SHADOW_VOGEL_POINTS[i], rotation);
        let sample_uv = clamp(uv + disk * spread, uv_min, uv_max);
        let sample_compare_depth = shadow_receiver_plane_compare_depth(uv, sample_uv, compare_depth, depth_gradient);
        let pcf = textureSampleCompare(t_shadow, s_shadow, sample_uv, sample_compare_depth);
        visibility += shadow_apply_transparent(sample_uv, sample_compare_depth, pcf);
    }
    return visibility / f32(steps);
}

struct ShadowPcssBlocker {
    average_gap: f32,
    count: f32,
};

fn shadow_find_blocker(
    cascade: u32,
    uv: vec2<f32>,
    compare_depth: f32,
    depth_gradient: vec2<f32>,
    spread: vec2<f32>,
    uv_min: vec2<f32>,
    uv_max: vec2<f32>,
    _rotation: f32,
) -> ShadowPcssBlocker {
    var average_receiver_gap = 0.0;
    var blocker_count = 0.0;
    let blocker_depth_bias = shadow_pcss_blocker_depth_bias(cascade);
    for (var x = -SHADOW_PCSS_BLOCKER_GRID_RADIUS; x <= SHADOW_PCSS_BLOCKER_GRID_RADIUS; x = x + 1) {
        for (var y = -SHADOW_PCSS_BLOCKER_GRID_RADIUS; y <= SHADOW_PCSS_BLOCKER_GRID_RADIUS; y = y + 1) {
            let base_offset = vec2<f32>(f32(x), f32(y)) * SHADOW_PCSS_BLOCKER_GRID_STEP_SCALE * spread;
            for (var tap = 0u; tap < 4u; tap = tap + 1u) {
                let tap_offset = vec2<f32>(
                    select(-0.5, 0.5, (tap & 1u) != 0u),
                    select(-0.5, 0.5, (tap & 2u) != 0u),
                ) * shadow.shadow_atlas_resolution_rcp.xy;
                let sample_uv = clamp(uv + base_offset + tap_offset, uv_min, uv_max);
                let sample_depth = shadow_load_depth(sample_uv);
                let sample_compare_depth = shadow_receiver_plane_compare_depth(uv, sample_uv, compare_depth, depth_gradient);
                let receiver_gap = sample_compare_depth - sample_depth;
                if (receiver_gap > blocker_depth_bias) {
                    average_receiver_gap += receiver_gap - blocker_depth_bias;
                    blocker_count += 1.0;
                }
            }
        }
    }
    return ShadowPcssBlocker(average_receiver_gap / max(blocker_count, 1.0), blocker_count);
}

fn shadow_pcss_filter_texels(base_filter_texels: f32, average_gap: f32) -> f32 {
    let penumbra = clamp(
        max(average_gap, 0.0) * SHADOW_PCSS_PENUMBRA_SCALE,
        0.0,
        SHADOW_PCSS_MAX_PENUMBRA_SCALE,
    );
    return clamp(
        base_filter_texels * penumbra,
        SHADOW_FILTER_RADIUS_BIAS,
        min(base_filter_texels * SHADOW_PCSS_MAX_PENUMBRA_SCALE, SHADOW_PCSS_MAX_FILTER_TEXELS),
    );
}

fn shadow_filter_pcss(
    cascade: u32,
    uv: vec2<f32>,
    compare_depth: f32,
    depth_gradient: vec2<f32>,
    light_radius: f32,
    uv_min: vec2<f32>,
    uv_max: vec2<f32>,
    rotation: f32,
) -> vec3<f32> {
    let base_filter_texels = shadow_filter_radius_texels(light_radius);
    let blocker_spread = shadow.shadow_atlas_resolution_rcp.xy * base_filter_texels;
    let blocker = shadow_find_blocker(
        cascade,
        uv,
        compare_depth,
        depth_gradient,
        blocker_spread,
        uv_min,
        uv_max,
        rotation,
    );
    if (blocker.count < 0.5) {
        return vec3<f32>(1.0);
    }

    let filter_texels = shadow_pcss_filter_texels(base_filter_texels, blocker.average_gap);
    let filter_spread = shadow.shadow_atlas_resolution_rcp.xy * filter_texels;
    return shadow_filter_pcf(
        uv,
        compare_depth,
        depth_gradient,
        filter_spread,
        uv_min,
        uv_max,
        rotation,
        SHADOW_PCF_STEPS_MAX,
    );
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

fn shadow_sample_projected_cascade(
    cascade: u32,
    local_uv: vec2<f32>,
    depth: f32,
    normal: vec3<f32>,
    screen_pixel: vec2<f32>,
    world_dx: vec3<f32>,
    world_dy: vec3<f32>,
) -> vec3<f32> {
    let cascade_params = shadow.cascade_params[cascade];
    let uv = shadow_2d_uv(local_uv, cascade);
    let compare_depth = shadow_compare_depth(cascade, depth, normal);
    let depth_gradient = shadow_receiver_plane_depth_gradient(cascade, world_dx, world_dy);
    let light_radius = cascade_params.z;
    if (shadow.shadow_atlas_resolution_rcp.x <= 0.0 || shadow.shadow_atlas_resolution_rcp.y <= 0.0) {
        let pcf = textureSampleCompare(t_shadow, s_shadow, uv, compare_depth);
        return shadow_apply_transparent(uv, compare_depth, pcf);
    }

    // Ported from WickedEngine's MIT-licensed shadowHF.hlsli shape:
    // 16-sample Vogel disk, border clamping, optional dither rotation, and
    // optional PCSS blocker search. PCSS also rotates the disk because its
    // larger penumbra kernel otherwise exposes the fixed Vogel lattice.
    let border = shadow_border_clamp(cascade);
    let uv_min = border.xy;
    let uv_max = border.zw;
    let mode = shadow_sampling_mode();
    let temporal_seed = shadow_temporal_rotation_seed();
    let random_angle = shadow_interleaved_gradient_noise(
        floor(screen_pixel) + vec2<f32>(temporal_seed * 256.0, temporal_seed * 163.0)
    ) * 6.28318531;
    let rotation = select(
        0.0,
        random_angle,
        mode == SHADOW_FILTER_DITHERED_PCF || mode == SHADOW_FILTER_PCSS,
    );
    let filter_texels = shadow_filter_radius_texels(light_radius);
    if (filter_texels <= 0.0) {
        let sample_uv = clamp(uv, uv_min, uv_max);
        let pcf = textureSampleCompare(t_shadow, s_shadow, sample_uv, compare_depth);
        return shadow_apply_transparent(sample_uv, compare_depth, pcf);
    }
    if (mode == SHADOW_FILTER_PCSS) {
        return shadow_filter_pcss(
            cascade,
            uv,
            compare_depth,
            depth_gradient,
            light_radius,
            uv_min,
            uv_max,
            rotation,
        );
    }

    let spread = shadow.shadow_atlas_resolution_rcp.xy * filter_texels;
    return shadow_filter_pcf(
        uv,
        compare_depth,
        depth_gradient,
        spread,
        uv_min,
        uv_max,
        rotation,
        SHADOW_PCF_STEPS_MAX,
    );
}

fn shadow_sample_biased_cascade(
    world_position: vec3<f32>,
    normal: vec3<f32>,
    cascade: u32,
    screen_pixel: vec2<f32>,
    world_dx: vec3<f32>,
    world_dy: vec3<f32>,
) -> vec3<f32> {
    let projection = shadow_project_cascade(world_position, cascade);
    return shadow_sample_projected_cascade(
        cascade,
        projection.local_uv,
        projection.depth,
        normal,
        screen_pixel,
        world_dx,
        world_dy,
    );
}

fn shadow_sample_cascade_if_inside(
    world_position: vec3<f32>,
    normal: vec3<f32>,
    cascade: u32,
    screen_pixel: vec2<f32>,
    world_dx: vec3<f32>,
    world_dy: vec3<f32>,
) -> vec3<f32> {
    let projection = shadow_project_cascade(world_position, cascade);
    if (projection.inside < 0.5) {
        return vec3<f32>(1.0);
    }
    return shadow_sample_projected_cascade(
        cascade,
        projection.local_uv,
        projection.depth,
        normal,
        screen_pixel,
        world_dx,
        world_dy,
    );
}

fn shadow_next_inside_cascade(world_position: vec3<f32>, start_cascade: u32, cascade_count: u32) -> u32 {
    for (var cascade = start_cascade; cascade < SHADOW_CASCADE_MAX; cascade = cascade + 1u) {
        if (cascade >= cascade_count) {
            break;
        }
        let projection = shadow_project_cascade(world_position, cascade);
        if (projection.inside > 0.5) {
            return cascade;
        }
    }
    return cascade_count;
}

fn shadow_active_cascade(world_position: vec3<f32>, cascade_count: u32) -> u32 {
    // Match WickedEngine's directional shadow selection: try the tightest
    // cascade first and only fall through when the receiver is outside it.
    return shadow_next_inside_cascade(world_position, 0u, cascade_count);
}

fn shadow_transmittance(
    world_position: vec3<f32>,
    normal: vec3<f32>,
    screen_pixel: vec2<f32>,
    world_dx: vec3<f32>,
    world_dy: vec3<f32>,
) -> vec3<f32> {
    if (shadow.shadow_params.w < 0.5 || material.shadow.x < 0.5) {
        return vec3<f32>(1.0);
    }

    let cascade_count = shadow_cascade_count();
    let cascade = shadow_active_cascade(world_position, cascade_count);
    if (cascade >= cascade_count) {
        return vec3<f32>(1.0);
    }

    let projection = shadow_project_cascade(world_position, cascade);
    let transmittance = shadow_sample_biased_cascade(
        world_position,
        normal,
        cascade,
        screen_pixel,
        world_dx,
        world_dy,
    );

    var blended_transmittance = transmittance;

    let edge_fade = shadow_cascade_edge_fade(projection.light_ndc, shadow.shadow_params.y);
    if (edge_fade <= 0.0) {
        return blended_transmittance;
    }
    if (cascade + 1u >= cascade_count) {
        return mix(blended_transmittance, vec3<f32>(1.0), edge_fade);
    }

    let fallback_cascade = shadow_next_inside_cascade(world_position, cascade + 1u, cascade_count);
    if (fallback_cascade >= cascade_count) {
        return mix(blended_transmittance, vec3<f32>(1.0), edge_fade);
    }
    let fallback_transmittance = shadow_sample_cascade_if_inside(
        world_position,
        normal,
        fallback_cascade,
        screen_pixel,
        world_dx,
        world_dy,
    );
    return mix(blended_transmittance, fallback_transmittance, edge_fade);
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

    let cascade_count = shadow_cascade_count();
    let cascade = shadow_active_cascade(world_position, cascade_count);
    if (cascade >= cascade_count) {
        return vec3<f32>(0.0);
    }

    let projection = shadow_project_cascade(world_position, cascade);
    var blended = shadow_cascade_debug_tint(cascade);
    let edge_fade = shadow_cascade_edge_fade(projection.light_ndc, shadow.shadow_params.y);
    if (edge_fade <= 0.0) {
        return blended;
    }
    if (cascade + 1u >= cascade_count) {
        return mix(blended, vec3<f32>(0.0), edge_fade);
    }
    let fallback_cascade = shadow_next_inside_cascade(world_position, cascade + 1u, cascade_count);
    if (fallback_cascade >= cascade_count) {
        return mix(blended, vec3<f32>(0.0), edge_fade);
    }
    let fallback_tint = shadow_cascade_debug_tint(fallback_cascade);
    blended = mix(blended, fallback_tint, edge_fade);
    return mix(blended, vec3<f32>(1.0), edge_fade * 0.18);
}

fn shadow_split_coverage_color(world_position: vec3<f32>) -> vec3<f32> {
    if (shadow.shadow_params.w < 0.5 || material.shadow.x < 0.5) {
        return vec3<f32>(0.0);
    }

    return shadow_cascade_debug_tint(shadow_cascade_index(shadow_receiver_view_depth(world_position)));
}

fn shadow_fade_debug_color(world_position: vec3<f32>, normal: vec3<f32>) -> vec3<f32> {
    if (shadow.shadow_params.w < 0.5 || material.shadow.x < 0.5) {
        return vec3<f32>(0.0);
    }

    let cascade_count = shadow_cascade_count();
    let cascade = shadow_active_cascade(world_position, cascade_count);
    if (cascade >= cascade_count) {
        return vec3<f32>(0.0);
    }

    let projection = shadow_project_cascade(world_position, cascade);
    let fade = shadow_cascade_edge_fade(projection.light_ndc, shadow.shadow_params.y);
    return mix(vec3<f32>(0.02, 0.02, 0.02), vec3<f32>(1.0, 0.85, 0.12), fade);
}

fn shadow_compare_delta_debug_color(world_position: vec3<f32>, normal: vec3<f32>) -> vec3<f32> {
    if (shadow.shadow_params.w < 0.5 || material.shadow.x < 0.5) {
        return vec3<f32>(0.0);
    }

    let cascade_count = shadow_cascade_count();
    let cascade = shadow_active_cascade(world_position, cascade_count);
    if (cascade >= cascade_count) {
        return vec3<f32>(0.0);
    }

    let projection = shadow_project_cascade(world_position, cascade);
    let uv = shadow_2d_uv(projection.local_uv, cascade);
    let stored_depth = shadow_load_compare_depth(uv, cascade);
    let compare_depth = shadow_compare_depth(cascade, projection.depth, normal);
    let delta = compare_depth - stored_depth;
    let scale = 180.0;
    let shadowed = clamp(delta * scale, 0.0, 1.0);
    let lit = clamp(-delta * scale, 0.0, 1.0);
    let near = 1.0 - clamp(abs(delta) * scale * 8.0, 0.0, 1.0);
    return vec3<f32>(shadowed, near, lit);
}

fn shadow_bias_debug_color(world_position: vec3<f32>, normal: vec3<f32>) -> vec3<f32> {
    if (shadow.shadow_params.w < 0.5 || material.shadow.x < 0.5) {
        return vec3<f32>(0.0);
    }

    let cascade_count = shadow_cascade_count();
    let cascade = shadow_active_cascade(world_position, cascade_count);
    if (cascade >= cascade_count) {
        return vec3<f32>(0.0);
    }

    let cascade_params = shadow.cascade_params[cascade];
    let texel_world_size = max(cascade_params.y, 0.000001);
    let depth_range = max(cascade_params.w, texel_world_size);
    let normal_bias_texels = shadow_receiver_bias_world(normal, cascade) / texel_world_size;
    let compare_bias_texels = shadow_compare_bias_depth(cascade, normal) * depth_range / texel_world_size;
    let filter_texels = shadow_filter_radius_texels(cascade_params.z);
    return vec3<f32>(
        clamp(normal_bias_texels / 2.0, 0.0, 1.0),
        clamp(compare_bias_texels / 2.0, 0.0, 1.0),
        clamp(filter_texels / SHADOW_PCSS_MAX_FILTER_TEXELS, 0.0, 1.0),
    );
}

fn shadow_pcss_debug_color(
    world_position: vec3<f32>,
    normal: vec3<f32>,
    world_dx: vec3<f32>,
    world_dy: vec3<f32>,
) -> vec3<f32> {
    if (shadow.shadow_params.w < 0.5 || material.shadow.x < 0.5) {
        return vec3<f32>(0.0);
    }

    let cascade_count = shadow_cascade_count();
    let cascade = shadow_active_cascade(world_position, cascade_count);
    if (cascade >= cascade_count) {
        return vec3<f32>(0.0);
    }

    let projection = shadow_project_cascade(world_position, cascade);
    let uv = shadow_2d_uv(projection.local_uv, cascade);
    let compare_depth = shadow_compare_depth(cascade, projection.depth, normal);
    let depth_gradient = shadow_receiver_plane_depth_gradient(cascade, world_dx, world_dy);
    let base_filter_texels = shadow_filter_radius_texels(shadow.cascade_params[cascade].z);
    let blocker = shadow_find_blocker(
        cascade,
        uv,
        compare_depth,
        depth_gradient,
        shadow.shadow_atlas_resolution_rcp.xy * base_filter_texels,
        shadow_border_clamp(cascade).xy,
        shadow_border_clamp(cascade).zw,
        0.0,
    );
    let penumbra = clamp(
        max(blocker.average_gap, 0.0) * SHADOW_PCSS_PENUMBRA_SCALE,
        0.0,
        SHADOW_PCSS_MAX_PENUMBRA_SCALE,
    );
    let filter_texels = select(
        0.0,
        shadow_pcss_filter_texels(base_filter_texels, blocker.average_gap),
        blocker.count >= 0.5,
    );
    return vec3<f32>(
        clamp(blocker.count / SHADOW_PCSS_BLOCKER_TAP_COUNT, 0.0, 1.0),
        clamp(penumbra / SHADOW_PCSS_MAX_PENUMBRA_SCALE, 0.0, 1.0),
        clamp(filter_texels / SHADOW_PCSS_MAX_FILTER_TEXELS, 0.0, 1.0),
    );
}

fn shadow_debug_color(
    world_position: vec3<f32>,
    normal: vec3<f32>,
    world_dx: vec3<f32>,
    world_dy: vec3<f32>,
) -> vec3<f32> {
    let mode = shadow_debug_mode();
    if (mode == SHADOW_DEBUG_SAMPLED_CASCADE) {
        return shadow_cascade_coverage_color(world_position, normal);
    }
    if (mode == SHADOW_DEBUG_SPLIT_COVERAGE) {
        return shadow_split_coverage_color(world_position);
    }
    if (mode == SHADOW_DEBUG_FADE) {
        return shadow_fade_debug_color(world_position, normal);
    }
    if (mode == SHADOW_DEBUG_COMPARE_DELTA) {
        return shadow_compare_delta_debug_color(world_position, normal);
    }
    if (mode == SHADOW_DEBUG_BIAS) {
        return shadow_bias_debug_color(world_position, normal);
    }
    if (mode == SHADOW_DEBUG_PCSS) {
        return shadow_pcss_debug_color(world_position, normal, world_dx, world_dy);
    }
    if (mode == SHADOW_DEBUG_DIRECT_LIGHTING) {
        return vec3<f32>(0.0);
    }
    if (mode == SHADOW_DEBUG_INDIRECT_LIGHTING) {
        return vec3<f32>(0.0);
    }
    return vec3<f32>(0.0);
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

/*GI_SHADER*/

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let base = textureSample(t_albedo, s_albedo, input.uv) * material.albedo;
    let world_dx = dpdx(input.world_position);
    let world_dy = dpdy(input.world_position);
    if (material.shadow.z > 0.5 && base.a < material.shadow.y) {
        discard;
    }
    let sampled = textureSample(t_normal, s_albedo, input.uv).xyz * 2.0 - vec3<f32>(1.0);
    let normal = safe_normalize(
        sampled.x * input.tangent +
        sampled.y * input.bitangent +
        sampled.z * input.normal
    );
    let geometric_normal = safe_normalize(input.normal);
    if (shadow_debug_mode() != SHADOW_DEBUG_NONE && material.shadow.x > 0.5) {
        let mode = shadow_debug_mode();
        if (mode == SHADOW_DEBUG_DIRECT_LIGHTING) {
            var lighting = vec3<f32>(0.0);
            let view_dir = safe_normalize(camera.camera.xyz - input.world_position);
            let metallic = clamp(material.params.x, 0.0, 1.0);
            let roughness = clamp(material.params.y, 0.05, 1.0);
            let f0 = mix(vec3<f32>(0.04), base.rgb, metallic);
            let ndotv = max(dot(normal, view_dir), 0.0);
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
                        shadow_term = shadow_transmittance(
                            input.world_position,
                            geometric_normal,
                            input.clip_position.xy,
                            world_dx,
                            world_dy,
                        );
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
            return vec4<f32>(lighting, base.a);
        }
        if (mode == SHADOW_DEBUG_INDIRECT_LIGHTING) {
            let metallic = clamp(material.params.x, 0.0, 1.0);
            let roughness = clamp(material.params.y, 0.05, 1.0);
            let indirect = gi_sample_indirect_diffuse(input.world_position, normal, base.rgb, metallic, roughness);
            return vec4<f32>(indirect, base.a);
        }
        return vec4<f32>(shadow_debug_color(input.world_position, geometric_normal, world_dx, world_dy), base.a);
    }
    if (gi_debug_mode() != 0u) {
        return vec4<f32>(gi_debug_color(input.world_position, normal), base.a);
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
                shadow_term = shadow_transmittance(
                    input.world_position,
                    geometric_normal,
                    input.clip_position.xy,
                    world_dx,
                    world_dy,
                );
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
    let indirect = gi_sample_indirect_diffuse(input.world_position, normal, base.rgb, metallic, roughness);
    return vec4<f32>(lighting + indirect + emissive, base.a);
}
