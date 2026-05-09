// WickedEngine-inspired temporal AA resolve.
// Ported from WickedEngine's temporalaaCS.hlsl shape under the MIT license:
// 8x8 compute, 3x3 neighborhood clamp, closest-depth velocity selection,
// disocclusion fallback, and Reinhard HDR correction before temporal blend.

struct TaaUniform {
    resolution: vec4<f32>,
    params0: vec4<f32>,
    params1: vec4<f32>,
};

@group(0) @binding(0)
var t_current: texture_2d<f32>;
@group(0) @binding(1)
var t_history: texture_2d<f32>;
@group(0) @binding(2)
var t_depth: texture_depth_2d;
@group(0) @binding(3)
var t_depth_history: texture_depth_2d;
@group(0) @binding(4)
var t_velocity: texture_2d<f32>;
@group(0) @binding(5)
var s_linear: sampler;

@group(1) @binding(0)
var<uniform> taa: TaaUniform;

@group(2) @binding(0)
var output: texture_storage_2d<rgba16float, write>;

const THREADCOUNT: u32 = 8u;
const TILE_BORDER: i32 = 1;
const TILE_SIZE: u32 = 10u;
const TILE_CACHE_COUNT: u32 = TILE_SIZE * TILE_SIZE;
const HALF_MAX: f32 = 65504.0;

var<workgroup> tile_cache: array<vec4<f32>, TILE_CACHE_COUNT>;

fn cache_index(coord: vec2<i32>) -> u32 {
    let c = clamp(coord, vec2<i32>(0), vec2<i32>(i32(TILE_SIZE) - 1));
    return u32(c.x) + u32(c.y) * TILE_SIZE;
}

fn clamped_pixel(pixel: vec2<i32>, dims: vec2<u32>) -> vec2<i32> {
    return clamp(pixel, vec2<i32>(0), vec2<i32>(dims) - vec2<i32>(1));
}

fn uv_to_pixel(uv: vec2<f32>, dims: vec2<u32>) -> vec2<i32> {
    return clamped_pixel(vec2<i32>(uv * vec2<f32>(dims)), dims);
}

fn is_saturated_uv(uv: vec2<f32>) -> bool {
    return all(uv >= vec2<f32>(0.0)) && all(uv <= vec2<f32>(1.0));
}

fn saturate(v: f32) -> f32 {
    return clamp(v, 0.0, 1.0);
}

fn tonemap(x: vec3<f32>) -> vec3<f32> {
    return x / (x + vec3<f32>(1.0));
}

fn inverse_tonemap(x: vec3<f32>) -> vec3<f32> {
    return x / max(vec3<f32>(1.0) - x, vec3<f32>(0.0001));
}

fn linear_depth(depth: f32) -> f32 {
    let near = max(taa.params1.y, 0.0001);
    let far = max(taa.params1.z, near + 0.0001);
    return (near * far) / max(far - depth * (far - near), 0.0001);
}

fn load_cached_sample(pixel: vec2<i32>, dims: vec2<u32>) -> vec4<f32> {
    let p = clamped_pixel(pixel, dims);
    let color = textureLoad(t_current, p, 0).rgb;
    let depth = linear_depth(textureLoad(t_depth, p, 0));
    return vec4<f32>(color, depth);
}

@compute @workgroup_size(8, 8, 1)
fn cs_main(
    @builtin(global_invocation_id) gid: vec3<u32>,
    @builtin(local_invocation_id) lid: vec3<u32>,
    @builtin(workgroup_id) wid: vec3<u32>,
) {
    let dims = vec2<u32>(u32(taa.resolution.x), u32(taa.resolution.y));
    let tile_upperleft = vec2<i32>(wid.xy * vec2<u32>(THREADCOUNT)) - vec2<i32>(TILE_BORDER);

    var y = lid.y;
    loop {
        if (y >= TILE_SIZE) {
            break;
        }
        var x = lid.x;
        loop {
            if (x >= TILE_SIZE) {
                break;
            }
            let local = vec2<i32>(i32(x), i32(y));
            let pixel = tile_upperleft + local;
            tile_cache[cache_index(local)] = load_cached_sample(pixel, dims);
            x = x + THREADCOUNT;
        }
        y = y + THREADCOUNT;
    }
    workgroupBarrier();

    if (gid.x >= dims.x || gid.y >= dims.y) {
        return;
    }

    let pixel = vec2<i32>(gid.xy);
    let uv = (vec2<f32>(gid.xy) + vec2<f32>(0.5)) * taa.resolution.zw;
    let pixel_local = vec2<i32>(TILE_BORDER) + vec2<i32>(lid.xy);

    var neighborhood_min = vec3<f32>(10000.0);
    var neighborhood_max = vec3<f32>(-10000.0);
    var current = vec3<f32>(0.0);
    var best_depth = 1000000.0;
    var best_offset = vec2<i32>(0);

    for (var yy = -TILE_BORDER; yy <= TILE_BORDER; yy = yy + 1) {
        for (var xx = -TILE_BORDER; xx <= TILE_BORDER; xx = xx + 1) {
            let offset = vec2<i32>(xx, yy);
            let neighbor = tile_cache[cache_index(pixel_local + offset)];
            neighborhood_min = min(neighborhood_min, neighbor.rgb);
            neighborhood_max = max(neighborhood_max, neighbor.rgb);
            if (xx == 0 && yy == 0) {
                current = neighbor.rgb;
            }
            if (neighbor.a < best_depth) {
                best_depth = neighbor.a;
                best_offset = offset;
            }
        }
    }

    let velocity_pixel = clamped_pixel(pixel + best_offset, dims);
    let velocity = textureLoad(t_velocity, velocity_pixel, 0).xy;
    let prev_uv_unclamped = uv + velocity;
    let prev_uv = clamp(prev_uv_unclamped, vec2<f32>(0.0), vec2<f32>(1.0));
    let jitter_velocity = vec2<f32>(taa.params1.x, taa.params1.w);
    let stable_velocity = velocity - jitter_velocity;
    let stable_velocity_pixels =
        max(abs(stable_velocity.x) * taa.resolution.x, abs(stable_velocity.y) * taa.resolution.y);

    let depth_current = linear_depth(textureLoad(t_depth, pixel, 0));
    let depth_history = linear_depth(textureLoad(t_depth_history, uv_to_pixel(prev_uv, dims), 0));
    let base_current_weight = clamp(taa.params0.y, 0.0, 1.0);
    let motion_current_weight = max(base_current_weight, clamp(taa.params0.z, 0.0, 1.0));
    var blendfactor = mix(
        base_current_weight,
        motion_current_weight,
        saturate((stable_velocity_pixels - 0.1) * 0.65),
    );

    let depth_delta = abs(depth_current - depth_history);
    let depth_threshold = max(0.08, depth_current * 0.03);
    if (stable_velocity_pixels > 0.25 && depth_delta > depth_threshold) {
        blendfactor = 1.0;
    }
    if (!is_saturated_uv(prev_uv_unclamped) || taa.params0.x > 0.5) {
        blendfactor = 1.0;
    }

    var history = textureSampleLevel(t_history, s_linear, prev_uv, 0.0).rgb;
    let clamp_expand = max(taa.params0.w, 0.0);
    history = clamp(history, neighborhood_min - vec3<f32>(clamp_expand), neighborhood_max + vec3<f32>(clamp_expand));

    let resolved = inverse_tonemap(mix(tonemap(history), tonemap(current), blendfactor));
    textureStore(output, gid.xy, vec4<f32>(clamp(resolved, vec3<f32>(0.0), vec3<f32>(HALF_MAX)), 1.0));
}
