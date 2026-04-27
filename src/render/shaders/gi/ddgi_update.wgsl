struct DdgiUniform {
    origin_spacing: vec4<f32>,
    counts_enabled: vec4<u32>,
    irradiance_atlas_params: vec4<u32>,
    visibility_atlas_params: vec4<u32>,
    trace_params: vec4<f32>,
    frame_params: vec4<u32>,
    ambient: vec4<f32>,
};

struct GiTriangle {
    p0: vec4<f32>,
    p1: vec4<f32>,
    p2: vec4<f32>,
    normal_emissive: vec4<f32>,
    albedo: vec4<f32>,
};

struct GiBvhNode {
    bounds_min: vec4<f32>,
    bounds_max: vec4<f32>,
    payload: vec4<u32>,
};

struct LightRecord {
    pos_radius: vec4<f32>,
    color: vec4<f32>,
    falloff: vec4<f32>,
};

struct LightMeta {
    count: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
};

@group(0) @binding(0)
var<uniform> ddgi: DdgiUniform;
@group(0) @binding(1)
var<storage, read> triangles: array<GiTriangle>;
@group(0) @binding(2)
var<storage, read> lights: array<LightRecord>;
@group(0) @binding(3)
var<uniform> light_meta: LightMeta;
@group(0) @binding(4)
var irradiance_out: texture_storage_2d<rgba16float, write>;
@group(0) @binding(5)
var irradiance_prev: texture_2d<f32>;
@group(0) @binding(6)
var visibility_out: texture_storage_2d<rgba16float, write>;
@group(0) @binding(7)
var irradiance_sampler: sampler;
@group(0) @binding(8)
var<storage, read> bvh_nodes: array<GiBvhNode>;
@group(0) @binding(9)
var visibility_prev: texture_2d<f32>;

const PI: f32 = 3.14159265359;
const GOLDEN: f32 = 2.39996322973;
const DDGI_ATLAS_BORDER: u32 = 1u;

fn saturate(v: f32) -> f32 {
    return clamp(v, 0.0, 1.0);
}

fn safe_normalize(v: vec3<f32>) -> vec3<f32> {
    let len_sq = dot(v, v);
    if (len_sq <= 0.000001) {
        return vec3<f32>(0.0, 1.0, 0.0);
    }
    return v * inverseSqrt(len_sq);
}

fn luminance(color: vec3<f32>) -> f32 {
    return dot(color, vec3<f32>(0.2126, 0.7152, 0.0722));
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

fn oct_decode(encoded: vec2<f32>) -> vec3<f32> {
    var v = vec3<f32>(encoded.x, encoded.y, 1.0 - abs(encoded.x) - abs(encoded.y));
    if (v.z < 0.0) {
        v = vec3<f32>(oct_wrap(v.xy), v.z);
    }
    return safe_normalize(v);
}

fn hash11(v: u32) -> f32 {
    var x = v;
    x = (x ^ 61u) ^ (x >> 16u);
    x = x + (x << 3u);
    x = x ^ (x >> 4u);
    x = x * 0x27d4eb2du;
    x = x ^ (x >> 15u);
    return f32(x & 0x00ffffffu) / f32(0x01000000u);
}

fn probe_count() -> u32 {
    return max(1u, ddgi.counts_enabled.x * ddgi.counts_enabled.y * ddgi.counts_enabled.z);
}

fn probe_coord(index: u32) -> vec3<u32> {
    let cx = max(1u, ddgi.counts_enabled.x);
    let cy = max(1u, ddgi.counts_enabled.y);
    let x = index % cx;
    let y = (index / cx) % cy;
    let z = index / (cx * cy);
    return vec3<u32>(x, y, z);
}

fn probe_index(coord: vec3<u32>) -> u32 {
    return coord.x + coord.y * ddgi.counts_enabled.x + coord.z * ddgi.counts_enabled.x * ddgi.counts_enabled.y;
}

fn probe_index_from_pixel(pixel: vec2<u32>, resolution: u32) -> u32 {
    let tile_res = probe_tile_resolution(resolution);
    let px = min(pixel.x / tile_res, ddgi.counts_enabled.x - 1u);
    let row = pixel.y / tile_res;
    let py = min(row % ddgi.counts_enabled.y, ddgi.counts_enabled.y - 1u);
    let pz = min(row / ddgi.counts_enabled.y, ddgi.counts_enabled.z - 1u);
    return px + py * ddgi.counts_enabled.x + pz * ddgi.counts_enabled.x * ddgi.counts_enabled.y;
}

fn probe_position(index: u32) -> vec3<f32> {
    let coord = probe_coord(index);
    return ddgi.origin_spacing.xyz + vec3<f32>(coord) * ddgi.origin_spacing.w;
}

fn probe_coord_from_world(world_position: vec3<f32>) -> vec3<u32> {
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

fn probe_tile_resolution(resolution: u32) -> u32 {
    return max(1u, resolution) + DDGI_ATLAS_BORDER * 2u;
}

fn probe_uv_from_coord_direction(coord: vec3<u32>, direction: vec3<f32>) -> vec2<f32> {
    let res = max(1u, ddgi.irradiance_atlas_params.z);
    let tile_res = probe_tile_resolution(res);
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
        f32(max(1u, ddgi.irradiance_atlas_params.x)),
        f32(max(1u, ddgi.irradiance_atlas_params.y)),
    );
    return pixel / atlas_size;
}

fn probe_texel_direction(pixel: vec2<u32>, resolution: u32) -> vec3<f32> {
    let res = max(1u, resolution);
    let tile_res = probe_tile_resolution(res);
    let local_pixel = clamp(
        vec2<i32>(
            i32(pixel.x % tile_res) - i32(DDGI_ATLAS_BORDER),
            i32(pixel.y % tile_res) - i32(DDGI_ATLAS_BORDER),
        ),
        vec2<i32>(0),
        vec2<i32>(i32(res) - 1),
    );
    let local = vec2<f32>(
        f32(local_pixel.x) + 0.5,
        f32(local_pixel.y) + 0.5,
    ) / f32(res);
    return oct_decode(local * 2.0 - vec2<f32>(1.0));
}

fn should_update_probe(index: u32) -> bool {
    let budget = max(1u, ddgi.frame_params.z);
    if (budget >= probe_count()) {
        return true;
    }
    let start = (ddgi.frame_params.x * budget) % probe_count();
    let wrapped = start + budget >= probe_count();
    if (!wrapped) {
        return index >= start && index < start + budget;
    }
    return index >= start || index < ((start + budget) % probe_count());
}

fn fibonacci_direction(sample_index: u32, sample_count: u32, seed: u32) -> vec3<f32> {
    let count = max(1u, sample_count);
    let jitter = hash11(seed + sample_index * 747796405u);
    let z = 1.0 - 2.0 * (f32(sample_index) + jitter) / f32(count);
    let r = sqrt(max(0.0, 1.0 - z * z));
    let phi = (f32(sample_index) + hash11(seed ^ 0xa511e9b3u)) * GOLDEN;
    return vec3<f32>(cos(phi) * r, z, sin(phi) * r);
}

fn intersect_triangle(origin: vec3<f32>, dir: vec3<f32>, tri: GiTriangle, max_t: f32) -> vec2<f32> {
    let v0 = tri.p0.xyz;
    let v1 = tri.p1.xyz;
    let v2 = tri.p2.xyz;
    let e1 = v1 - v0;
    let e2 = v2 - v0;
    let pvec = cross(dir, e2);
    let det = dot(e1, pvec);
    if (abs(det) <= 0.0000001) {
        return vec2<f32>(-1.0, 0.0);
    }
    let inv_det = 1.0 / det;
    let tvec = origin - v0;
    let u = dot(tvec, pvec) * inv_det;
    if (u < 0.0 || u > 1.0) {
        return vec2<f32>(-1.0, 0.0);
    }
    let qvec = cross(tvec, e1);
    let v = dot(dir, qvec) * inv_det;
    if (v < 0.0 || u + v > 1.0) {
        return vec2<f32>(-1.0, 0.0);
    }
    let t = dot(e2, qvec) * inv_det;
    if (t <= 0.001 || t >= max_t) {
        return vec2<f32>(-1.0, 0.0);
    }
    return vec2<f32>(t, select(-1.0, 1.0, det > 0.0));
}

fn safe_inverse_component(v: f32) -> f32 {
    let fallback = select(-0.000001, 0.000001, v >= 0.0);
    let safe = select(fallback, v, abs(v) > 0.000001);
    return 1.0 / safe;
}

fn safe_inverse_dir(v: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        safe_inverse_component(v.x),
        safe_inverse_component(v.y),
        safe_inverse_component(v.z),
    );
}

fn intersect_aabb(origin: vec3<f32>, inv_dir: vec3<f32>, node: GiBvhNode, max_t: f32) -> bool {
    let t0 = (node.bounds_min.xyz - origin) * inv_dir;
    let t1 = (node.bounds_max.xyz - origin) * inv_dir;
    let near_t = min(t0, t1);
    let far_t = max(t0, t1);
    let entry = max(max(near_t.x, near_t.y), max(near_t.z, 0.0));
    let exit = min(min(far_t.x, far_t.y), min(far_t.z, max_t));
    return exit >= entry;
}

fn trace_scene(origin: vec3<f32>, dir: vec3<f32>, max_t: f32) -> vec4<f32> {
    var best_t = max_t;
    var best_index = 0xffffffffu;
    let node_count = min(ddgi.irradiance_atlas_params.w, arrayLength(&bvh_nodes));
    if (node_count == 0u) {
        return vec4<f32>(best_t, f32(best_index), 0.0, 0.0);
    }

    let tri_count = arrayLength(&triangles);
    let inv_dir = safe_inverse_dir(dir);
    var stack: array<u32, 64>;
    var stack_count = 1u;
    stack[0] = 0u;

    loop {
        if (stack_count == 0u) {
            break;
        }
        stack_count = stack_count - 1u;
        let node_index = stack[stack_count];
        if (node_index >= node_count) {
            continue;
        }
        let node = bvh_nodes[node_index];
        if (!intersect_aabb(origin, inv_dir, node, best_t)) {
            continue;
        }

        if (node.payload.w != 0u) {
            let tri_index = node.payload.z;
            if (tri_index >= tri_count) {
                continue;
            }
            let hit = intersect_triangle(origin, dir, triangles[tri_index], best_t);
            if (hit.x > 0.0) {
                best_t = hit.x;
                best_index = tri_index;
            }
            continue;
        }

        let right = node.payload.y;
        if (right < node_count && stack_count < 64u) {
            stack[stack_count] = right;
            stack_count = stack_count + 1u;
        }
        let left = node.payload.x;
        if (left < node_count && stack_count < 64u) {
            stack[stack_count] = left;
            stack_count = stack_count + 1u;
        }
    }
    return vec4<f32>(best_t, f32(best_index), 0.0, select(0.0, 1.0, best_index != 0xffffffffu));
}

fn occluded(origin: vec3<f32>, dir: vec3<f32>, max_t: f32) -> bool {
    return trace_scene(origin, dir, max_t).w > 0.5;
}

fn direct_lighting(hit_pos: vec3<f32>, normal: vec3<f32>, albedo: vec3<f32>) -> vec3<f32> {
    var result = vec3<f32>(0.0);
    let max_lights = min(light_meta.count, arrayLength(&lights));
    for (var i = 0u; i < max_lights; i = i + 1u) {
        let light = lights[i];
        if (light.falloff.y > 0.5) {
            let light_dir = safe_normalize(-light.pos_radius.xyz);
            let ndotl = max(dot(normal, light_dir), 0.0);
            if (ndotl <= 0.0001) {
                continue;
            }
            let shadow_origin = hit_pos + normal * ddgi.trace_params.z + light_dir * 0.015;
            if (!occluded(shadow_origin, light_dir, ddgi.trace_params.x)) {
                result += albedo * light.color.rgb * ndotl / PI;
            }
            continue;
        }

        let to_light = light.pos_radius.xyz - hit_pos;
        let dist = length(to_light);
        let radius = max(light.pos_radius.w, 0.001);
        if (dist >= radius) {
            continue;
        }
        let light_dir = to_light / max(dist, 0.0001);
        let ndotl = max(dot(normal, light_dir), 0.0);
        if (ndotl <= 0.0001) {
            continue;
        }
        let attenuation = pow(max(1.0 - dist / radius, 0.0), max(light.falloff.x, 0.001));
        let shadow_origin = hit_pos + normal * ddgi.trace_params.z + light_dir * 0.015;
        if (!occluded(shadow_origin, light_dir, dist - 0.03)) {
            result += albedo * light.color.rgb * ndotl * attenuation / PI;
        }
    }
    return result;
}

fn sample_previous_pixel(pixel: vec2<u32>) -> vec3<f32> {
    return textureLoad(irradiance_prev, vec2<i32>(i32(pixel.x), i32(pixel.y)), 0).rgb;
}

fn sample_previous_visibility_pixel(pixel: vec2<u32>) -> vec2<f32> {
    return textureLoad(visibility_prev, vec2<i32>(i32(pixel.x), i32(pixel.y)), 0).rg;
}

fn sample_previous_probe_direction(coord: vec3<u32>, direction: vec3<f32>) -> vec3<f32> {
    return textureSampleLevel(
        irradiance_prev,
        irradiance_sampler,
        probe_uv_from_coord_direction(coord, direction),
        0.0,
    ).rgb;
}

fn bounce_count() -> u32 {
    return max(1u, ddgi.frame_params.w & 0xffffu);
}

struct ProbeSample {
    irradiance: vec3<f32>,
    visibility: vec2<f32>,
    hit_ratio: f32,
};

fn temporal_blend_factor() -> f32 {
    let first_sweep_budget = ddgi.frame_params.x * max(1u, ddgi.frame_params.z);
    if (first_sweep_budget < probe_count()) {
        return 0.0;
    }
    return ddgi.trace_params.y;
}

fn trace_probe(index: u32, texel_direction: vec3<f32>) -> ProbeSample {
    let max_ray_distance = ddgi.trace_params.x;
    if (ddgi.counts_enabled.w == 0u) {
        return ProbeSample(vec3<f32>(0.0), vec2<f32>(max_ray_distance, max_ray_distance * max_ray_distance), 0.0);
    }

    let origin = probe_position(index);
    let rays = max(1u, ddgi.frame_params.y);
    var accum = vec3<f32>(0.0);
    var weight_sum = 0.0;
    var distance_sum = 0.0;
    var distance_sq_sum = 0.0;
    var visibility_weight = 0.0;
    var hit_count = 0.0;

    for (var ray_index = 0u; ray_index < rays; ray_index = ray_index + 1u) {
        let dir = fibonacci_direction(ray_index, rays, index ^ ddgi.frame_params.x);
        let sample_weight = max(dot(dir, texel_direction), 0.0);
        if (sample_weight <= 0.00001) {
            continue;
        }

        let hit = trace_scene(origin, dir, max_ray_distance);
        let sample_distance = select(max_ray_distance, hit.x, hit.w > 0.5);
        distance_sum += sample_distance * sample_weight;
        distance_sq_sum += sample_distance * sample_distance * sample_weight;
        visibility_weight += sample_weight;
        weight_sum += sample_weight;

        if (hit.w < 0.5) {
            accum += ddgi.ambient.rgb * 0.025 * sample_weight;
            continue;
        }

        let tri_index = u32(hit.y);
        let tri = triangles[tri_index];
        let normal = safe_normalize(tri.normal_emissive.xyz);
        var facing_normal = normal;
        if (dot(normal, -dir) <= 0.0) {
            facing_normal = -normal;
        }
        let hit_pos = origin + dir * hit.x;
        let albedo = tri.albedo.rgb * (1.0 - saturate(tri.albedo.a) * 0.85);
        let emissive = albedo * tri.normal_emissive.w * 1.8;
        var bounce = vec3<f32>(0.0);
        if (bounce_count() > 1u) {
            let bounce_coord = probe_coord_from_world(hit_pos + facing_normal * ddgi.trace_params.z);
            bounce = sample_previous_probe_direction(bounce_coord, facing_normal) * albedo * 0.5;
        }
        accum += (emissive + direct_lighting(hit_pos, facing_normal, albedo) + bounce) * sample_weight;
        hit_count += 1.0;
    }

    let current = max(accum / max(weight_sum, 0.0001) + ddgi.ambient.rgb * 0.015, vec3<f32>(0.0));
    let visibility = vec2<f32>(
        distance_sum / max(visibility_weight, 0.0001),
        distance_sq_sum / max(visibility_weight, 0.0001),
    );
    return ProbeSample(current, visibility, hit_count / f32(rays));
}

@compute @workgroup_size(8, 8, 1)
fn cs_main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let pixel = gid.xy;
    let in_irradiance = pixel.x < ddgi.irradiance_atlas_params.x && pixel.y < ddgi.irradiance_atlas_params.y;
    let in_visibility = pixel.x < ddgi.visibility_atlas_params.x && pixel.y < ddgi.visibility_atlas_params.y;
    if (!in_irradiance && !in_visibility) {
        return;
    }

    let enabled = ddgi.counts_enabled.w != 0u;
    let max_ray_distance = ddgi.trace_params.x;
    let blend = temporal_blend_factor();
    var common_sample = ProbeSample(vec3<f32>(0.0), vec2<f32>(max_ray_distance, max_ray_distance * max_ray_distance), 0.0);
    var common_probe_index = 0xffffffffu;
    var common_valid = false;

    if (in_irradiance) {
        let irradiance_res = max(1u, ddgi.irradiance_atlas_params.z);
        let irradiance_probe = probe_index_from_pixel(pixel, irradiance_res);
        let texel_direction = probe_texel_direction(pixel, irradiance_res);
        if (!enabled) {
            textureStore(irradiance_out, vec2<i32>(pixel), vec4<f32>(0.0, 0.0, 0.0, 1.0));
        } else if (!should_update_probe(irradiance_probe)) {
            textureStore(irradiance_out, vec2<i32>(pixel), vec4<f32>(sample_previous_pixel(pixel), 1.0));
        } else {
            common_sample = trace_probe(irradiance_probe, texel_direction);
            common_probe_index = irradiance_probe;
            common_valid = true;
            let previous = sample_previous_pixel(pixel);
            let blended = mix(common_sample.irradiance, previous, blend);
            textureStore(irradiance_out, vec2<i32>(pixel), vec4<f32>(blended, 1.0));
        }
    }

    if (in_visibility) {
        let visibility_res = max(1u, ddgi.visibility_atlas_params.z);
        let visibility_probe = probe_index_from_pixel(pixel, visibility_res);
        if (!enabled) {
            textureStore(
                visibility_out,
                vec2<i32>(pixel),
                vec4<f32>(max_ray_distance, max_ray_distance * max_ray_distance, 0.0, 1.0),
            );
        } else if (!should_update_probe(visibility_probe)) {
            let previous = sample_previous_visibility_pixel(pixel);
            textureStore(visibility_out, vec2<i32>(pixel), vec4<f32>(previous, 0.0, 1.0));
        } else {
            var value = common_sample;
            if (!common_valid ||
                common_probe_index != visibility_probe ||
                ddgi.irradiance_atlas_params.z != ddgi.visibility_atlas_params.z) {
                let texel_direction = probe_texel_direction(pixel, visibility_res);
                value = trace_probe(visibility_probe, texel_direction);
            }
            let previous = sample_previous_visibility_pixel(pixel);
            let blended = mix(value.visibility, previous, blend);
            textureStore(visibility_out, vec2<i32>(pixel), vec4<f32>(blended, value.hit_ratio, 1.0));
        }
    }
}
