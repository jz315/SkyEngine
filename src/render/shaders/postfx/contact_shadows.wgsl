struct ContactShadowsUniform {
    projection: mat4x4<f32>,
    inverse_projection: mat4x4<f32>,
    light_direction_view: vec4<f32>,
    params0: vec4<f32>,
    params1: vec4<f32>,
};

@group(0) @binding(0)
var t_scene_color: texture_2d<f32>;
@group(0) @binding(1)
var t_scene_depth: texture_depth_2d;
@group(0) @binding(2)
var t_scene_normal: texture_2d<f32>;

@group(1) @binding(0)
var<uniform> contact: ContactShadowsUniform;

const PI: f32 = 3.14159265;
const TAU: f32 = 6.28318531;
const AO_SLICES: u32 = 2u;
const SSRT_STEP_RATIO: f32 = 1.65;
const GOLDEN_RATIO_CONJUGATE: f32 = 0.61803399;

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

fn clamped_pixel(pixel: vec2<i32>, dims: vec2<u32>) -> vec2<i32> {
    return clamp(pixel, vec2<i32>(0), vec2<i32>(dims) - vec2<i32>(1));
}

fn pixel_uv(pixel: vec2<i32>, dims: vec2<u32>) -> vec2<f32> {
    return (vec2<f32>(clamped_pixel(pixel, dims)) + vec2<f32>(0.5)) / vec2<f32>(dims);
}

fn interleaved_gradient_noise(pixel: vec2<f32>) -> f32 {
    return fract(52.9829189 * fract(dot(pixel, vec2<f32>(0.06711056, 0.00583715))));
}

fn decode_view_normal(encoded: vec3<f32>) -> vec3<f32> {
    return safe_normalize(encoded * 2.0 - vec3<f32>(1.0));
}

fn reconstruct_view_position(pixel: vec2<i32>, dims: vec2<u32>, depth: f32) -> vec3<f32> {
    let uv = pixel_uv(pixel, dims);
    let clip = vec4<f32>(uv.x * 2.0 - 1.0, (1.0 - uv.y) * 2.0 - 1.0, depth, 1.0);
    let view = contact.inverse_projection * clip;
    if (abs(view.w) > 0.000001) {
        return view.xyz / view.w;
    }
    return view.xyz;
}

fn project_view_position(view_position: vec3<f32>) -> vec3<f32> {
    let clip = contact.projection * vec4<f32>(view_position, 1.0);
    if (abs(clip.w) <= 0.000001) {
        return vec3<f32>(-1.0);
    }
    let ndc = clip.xyz / clip.w;
    return vec3<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5, ndc.z);
}

fn screen_space_shadow(
    pixel: vec2<i32>,
    dims: vec2<u32>,
    view_position: vec3<f32>,
    view_normal: vec3<f32>,
    light_direction: vec3<f32>,
) -> f32 {
    let ndotl = dot(view_normal, light_direction);
    if (ndotl <= 0.001) {
        return 1.0;
    }

    let steps = max(u32(round(contact.params0.w)), 1u);
    let max_distance = max(contact.params0.y, 0.001);
    let thickness = max(contact.params0.z, 0.001);
    let frame_seed = contact.params1.w;
    let noise = interleaved_gradient_noise(
        vec2<f32>(pixel) + vec2<f32>(frame_seed * 0.7548777, frame_seed * 0.5698403)
    );
    let origin_jitter = interleaved_gradient_noise(
        vec2<f32>(pixel) + vec2<f32>(31.0 + frame_seed * 0.242372, 11.0 + frame_seed * 0.367879)
    );
    let origin = view_position
        + view_normal * max(0.008, thickness * 0.12)
        + light_direction * (origin_jitter * thickness * 0.12);
    var occlusion = 0.0;
    let steps_f = f32(steps);
    let initial_step = max_distance
        * ((SSRT_STEP_RATIO - 1.0) / max(pow(SSRT_STEP_RATIO, steps_f) - 1.0, 0.0001));
    var segment_start = max(thickness * 0.08, initial_step * 0.35);
    var segment_length = initial_step;

    for (var i = 0u; i < 32u; i = i + 1u) {
        if (i >= steps) {
            break;
        }
        let segment_jitter = fract(noise + f32(i) * GOLDEN_RATIO_CONJUGATE);
        let sample_distance = min(segment_start + segment_length * segment_jitter, max_distance);
        segment_start = segment_start + segment_length;
        segment_length = segment_length * SSRT_STEP_RATIO;

        let ray_t = saturate(sample_distance / max_distance);
        let ray_position = origin + light_direction * sample_distance;
        let projected = project_view_position(ray_position);
        if (projected.z <= 0.0 || projected.z >= 1.0 ||
            projected.x < 0.0 || projected.x > 1.0 ||
            projected.y < 0.0 || projected.y > 1.0) {
            break;
        }

        let sample_pixel = clamped_pixel(vec2<i32>(projected.xy * vec2<f32>(dims)), dims);
        let sample_depth = textureLoad(t_scene_depth, sample_pixel, 0);
        if (sample_depth >= 0.99999) {
            continue;
        }

        let sample_position = reconstruct_view_position(sample_pixel, dims, sample_depth);
        let ray_depth = max(-ray_position.z, 0.0);
        let sample_depth_linear = max(-sample_position.z, 0.0);
        let depth_delta = ray_depth - sample_depth_linear;
        let depth_thickness = max(thickness, ray_depth * 0.018);
        if (depth_delta > 0.0) {
            let hit_weight = 1.0 - smoothstep(depth_thickness * 0.35, depth_thickness, depth_delta);
            let distance_weight = (1.0 - ray_t) * (1.0 - ray_t);
            let contact_weight = saturate(ndotl * 1.35) * hit_weight * distance_weight;
            occlusion = occlusion + (1.0 - occlusion) * contact_weight;
        }
    }

    return 1.0 - saturate(occlusion * max(contact.params0.x, 0.0));
}

fn horizon_ao(
    pixel: vec2<i32>,
    dims: vec2<u32>,
    view_position: vec3<f32>,
    view_normal: vec3<f32>,
) -> f32 {
    let ao_intensity = max(contact.params1.x, 0.0);
    if (ao_intensity <= 0.0) {
        return 1.0;
    }

    let radius_pixels = max(contact.params1.y, 1.0);
    let steps = max(u32(round(contact.params1.z)), 1u);
    let frame_seed = contact.params1.w;
    let noise = interleaved_gradient_noise(
        vec2<f32>(pixel) + vec2<f32>(17.0 + frame_seed * 0.381966, 43.0 + frame_seed * 0.618034)
    );
    var occlusion = 0.0;
    var weight_sum = 0.0;

    for (var slice = 0u; slice < AO_SLICES; slice = slice + 1u) {
        let angle = (f32(slice) + noise) * (PI / f32(AO_SLICES));
        let axis = vec2<f32>(cos(angle), sin(angle));

        for (var side = 0u; side < 2u; side = side + 1u) {
            let side_sign = select(-1.0, 1.0, side == 1u);
            for (var step = 1u; step <= 16u; step = step + 1u) {
                if (step > steps) {
                    break;
                }
                let step_ratio = f32(step) / f32(steps);
                let offset = axis * side_sign * (step_ratio * radius_pixels);
                let sample_pixel = clamped_pixel(pixel + vec2<i32>(round(offset)), dims);
                let sample_depth = textureLoad(t_scene_depth, sample_pixel, 0);
                if (sample_depth >= 0.99999) {
                    continue;
                }

                let sample_position = reconstruct_view_position(sample_pixel, dims, sample_depth);
                let horizon = sample_position - view_position;
                let horizon_len = length(horizon);
                if (horizon_len <= 0.0001) {
                    continue;
                }

                let horizon_dir = horizon / horizon_len;
                let normal_term = max(dot(view_normal, horizon_dir) - 0.05, 0.0);
                let distance_fade = 1.0 - saturate(horizon_len / max(contact.params0.y, 0.001));
                let weight = distance_fade * distance_fade;
                occlusion = occlusion + normal_term * weight;
                weight_sum = weight_sum + weight;
            }
        }
    }

    let ao = 1.0 - saturate((occlusion / max(weight_sum, 0.0001)) * ao_intensity * 3.0);
    return clamp(ao, 1.0 - ao_intensity, 1.0);
}

@fragment
fn fs_main(input: FullscreenOutput) -> @location(0) vec4<f32> {
    let dims = textureDimensions(t_scene_color);
    let pixel = clamped_pixel(vec2<i32>(input.position.xy), dims);
    let color = textureLoad(t_scene_color, pixel, 0);
    let depth = textureLoad(t_scene_depth, pixel, 0);
    if (depth >= 0.99999) {
        return color;
    }

    let view_position = reconstruct_view_position(pixel, dims, depth);
    let view_normal = decode_view_normal(textureLoad(t_scene_normal, pixel, 0).rgb);
    let light_direction = safe_normalize(contact.light_direction_view.xyz);
    let contact_visibility = screen_space_shadow(pixel, dims, view_position, view_normal, light_direction);
    let ao_visibility = horizon_ao(pixel, dims, view_position, view_normal);
    let visibility = min(contact_visibility, ao_visibility);
    let luminance = dot(max(color.rgb, vec3<f32>(0.0)), vec3<f32>(0.2126, 0.7152, 0.0722));
    let emissive_guard = smoothstep(2.0, 8.0, luminance);
    let shadowed_visibility = mix(visibility, 1.0, emissive_guard);
    return vec4<f32>(max(color.rgb * shadowed_visibility, vec3<f32>(0.0)), color.a);
}
