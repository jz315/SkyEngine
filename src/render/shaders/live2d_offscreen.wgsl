struct CompositeUniforms {
    clip_matrix: mat4x4<f32>,
    base_color: vec4<f32>,
    multiply_color: vec4<f32>,
    screen_color: vec4<f32>,
    channel_flag: vec4<f32>,
    use_mask: f32,
    inverted_mask: f32,
    color_blend_type: u32,
    alpha_blend_type: u32,
};

@group(0) @binding(0) var<uniform> uniforms: CompositeUniforms;
@group(1) @binding(0) var source_texture: texture_2d<f32>;
@group(1) @binding(1) var mask_texture: texture_2d<f32>;
@group(1) @binding(2) var destination_texture: texture_2d<f32>;
@group(1) @binding(3) var tex_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) clip_pos: vec4<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VertexOutput {
    var output: VertexOutput;

    let x = f32(i32(idx & 1u) * 4 - 1);
    let y = f32(i32(idx >> 1u) * 4 - 1);
    let pos4 = vec4<f32>(x, y, 0.0, 1.0);

    output.position = pos4;
    output.uv = vec2<f32>((x + 1.0) * 0.5, 1.0 - (y + 1.0) * 0.5);
    output.clip_pos = uniforms.clip_matrix * pos4;
    return output;
}

fn sample_mask(input: VertexOutput) -> f32 {
    if uniforms.use_mask <= 0.5 {
        return 1.0;
    }

    let mask_uv = input.clip_pos.xy / input.clip_pos.w;
    let clip_mask =
        (vec4<f32>(1.0) - textureSample(mask_texture, tex_sampler, mask_uv)) * uniforms.channel_flag;
    let mask_value = clip_mask.r + clip_mask.g + clip_mask.b + clip_mask.a;
    if uniforms.inverted_mask > 0.5 {
        return 1.0 - mask_value;
    }
    return mask_value;
}

fn apply_compatible_source(input: VertexOutput) -> vec4<f32> {
    var tex_color = textureSample(source_texture, tex_sampler, input.uv);
    tex_color = vec4<f32>(tex_color.rgb * uniforms.multiply_color.rgb, tex_color.a);

    let screen_sum = tex_color.rgb + uniforms.screen_color.rgb * tex_color.a;
    let screen_product = tex_color.rgb * uniforms.screen_color.rgb;
    tex_color = vec4<f32>(screen_sum - screen_product, tex_color.a);

    return tex_color * uniforms.base_color;
}

@fragment
fn compatible_fs(input: VertexOutput) -> @location(0) vec4<f32> {
    let mask_value = sample_mask(input);
    return apply_compatible_source(input) * mask_value;
}

fn convert_premultiplied_to_straight(source: vec4<f32>) -> vec4<f32> {
    if abs(source.a) < 0.00001 {
        return vec4<f32>(0.0, 0.0, 0.0, source.a);
    }
    return vec4<f32>(source.rgb / source.a, source.a);
}

fn color_burn(color_source: f32, color_destination: f32) -> f32 {
    if abs(color_destination - 1.0) < 0.000001 {
        return 1.0;
    }
    if abs(color_source) < 0.000001 {
        return 0.0;
    }
    return 1.0 - min(1.0, (1.0 - color_destination) / color_source);
}

fn color_dodge(color_source: f32, color_destination: f32) -> f32 {
    if color_destination <= 0.0 {
        return 0.0;
    }
    if color_source == 1.0 {
        return 1.0;
    }
    return min(1.0, color_destination / (1.0 - color_source));
}

fn overlay(color_source: f32, color_destination: f32) -> f32 {
    let mul = 2.0 * color_source * color_destination;
    let scr = 1.0 - 2.0 * (1.0 - color_source) * (1.0 - color_destination);
    if color_destination < 0.5 {
        return mul;
    }
    return scr;
}

fn soft_light(color_source: f32, color_destination: f32) -> f32 {
    let val1 =
        color_destination - (1.0 - 2.0 * color_source) * color_destination * (1.0 - color_destination);
    let val2 = color_destination
        + (2.0 * color_source - 1.0)
            * color_destination
            * ((16.0 * color_destination - 12.0) * color_destination + 3.0);
    let val3 =
        color_destination + (2.0 * color_source - 1.0) * (sqrt(color_destination) - color_destination);

    if color_source <= 0.5 {
        return val1;
    }
    if color_destination <= 0.25 {
        return val2;
    }
    return val3;
}

fn hard_light(color_source: f32, color_destination: f32) -> f32 {
    let mul = 2.0 * color_source * color_destination;
    let scr = 1.0 - 2.0 * (1.0 - color_source) * (1.0 - color_destination);
    if color_source < 0.5 {
        return mul;
    }
    return scr;
}

fn linear_light(color_source: f32, color_destination: f32) -> f32 {
    let burn = max(0.0, 2.0 * color_source + color_destination - 1.0);
    let dodge = min(1.0, 2.0 * (color_source - 0.5) + color_destination);
    if color_source < 0.5 {
        return burn;
    }
    return dodge;
}

fn get_max(rgb: vec3<f32>) -> f32 {
    return max(rgb.r, max(rgb.g, rgb.b));
}

fn get_min(rgb: vec3<f32>) -> f32 {
    return min(rgb.r, min(rgb.g, rgb.b));
}

fn saturation(rgb: vec3<f32>) -> f32 {
    return get_max(rgb) - get_min(rgb);
}

fn luma(rgb: vec3<f32>) -> f32 {
    return 0.30 * rgb.r + 0.59 * rgb.g + 0.11 * rgb.b;
}

fn clip_color(rgb: vec3<f32>) -> vec3<f32> {
    let lum = luma(rgb);
    let maxv = get_max(rgb);
    let minv = get_min(rgb);
    var output_color = rgb;
    if minv < 0.0 {
        output_color = lum + (output_color - vec3<f32>(lum)) * lum / (lum - minv);
    }
    if maxv > 1.0 {
        output_color = lum + (output_color - vec3<f32>(lum)) * (1.0 - lum) / (maxv - lum);
    }
    return output_color;
}

fn set_luma(rgb: vec3<f32>, lum: f32) -> vec3<f32> {
    return clip_color(rgb + vec3<f32>(lum - luma(rgb)));
}

fn set_saturation(rgb: vec3<f32>, target_saturation: f32) -> vec3<f32> {
    let maxv = get_max(rgb);
    let minv = get_min(rgb);
    let medv = rgb.r + rgb.g + rgb.b - maxv - minv;
    let output_max = select(0.0, target_saturation, minv < maxv);
    let output_med = select(0.0, (medv - minv) * target_saturation / (maxv - minv), minv < maxv);

    if rgb.r == maxv {
        if rgb.b < rgb.g {
            return vec3<f32>(output_max, output_med, 0.0);
        }
        return vec3<f32>(output_max, 0.0, output_med);
    }
    if rgb.g == maxv {
        if rgb.r < rgb.b {
            return vec3<f32>(0.0, output_max, output_med);
        }
        return vec3<f32>(output_med, output_max, 0.0);
    }
    if rgb.g < rgb.r {
        return vec3<f32>(output_med, 0.0, output_max);
    }
    return vec3<f32>(0.0, output_med, output_max);
}

fn color_blend(color_source: vec3<f32>, color_destination: vec3<f32>) -> vec3<f32> {
    switch uniforms.color_blend_type {
        case 0u {
            return color_source;
        }
        case 3u {
            return min(color_source + color_destination, vec3<f32>(1.0));
        }
        case 4u {
            return color_source + color_destination;
        }
        case 5u {
            return min(color_source, color_destination);
        }
        case 6u {
            return color_source * color_destination;
        }
        case 7u {
            return vec3<f32>(
                color_burn(color_source.r, color_destination.r),
                color_burn(color_source.g, color_destination.g),
                color_burn(color_source.b, color_destination.b),
            );
        }
        case 8u {
            return max(vec3<f32>(0.0), color_source + color_destination - vec3<f32>(1.0));
        }
        case 9u {
            return max(color_source, color_destination);
        }
        case 10u {
            return color_source + color_destination - color_source * color_destination;
        }
        case 11u {
            return vec3<f32>(
                color_dodge(color_source.r, color_destination.r),
                color_dodge(color_source.g, color_destination.g),
                color_dodge(color_source.b, color_destination.b),
            );
        }
        case 12u {
            return vec3<f32>(
                overlay(color_source.r, color_destination.r),
                overlay(color_source.g, color_destination.g),
                overlay(color_source.b, color_destination.b),
            );
        }
        case 13u {
            return vec3<f32>(
                soft_light(color_source.r, color_destination.r),
                soft_light(color_source.g, color_destination.g),
                soft_light(color_source.b, color_destination.b),
            );
        }
        case 14u {
            return vec3<f32>(
                hard_light(color_source.r, color_destination.r),
                hard_light(color_source.g, color_destination.g),
                hard_light(color_source.b, color_destination.b),
            );
        }
        case 15u {
            return vec3<f32>(
                linear_light(color_source.r, color_destination.r),
                linear_light(color_source.g, color_destination.g),
                linear_light(color_source.b, color_destination.b),
            );
        }
        case 16u {
            return set_luma(set_saturation(color_source, saturation(color_destination)), luma(color_destination));
        }
        case 17u {
            return set_luma(color_source, luma(color_destination));
        }
        default {
            return color_source;
        }
    }
}

fn overlap_rgba(
    color: vec3<f32>,
    color_source: vec3<f32>,
    color_destination: vec3<f32>,
    parameter: vec3<f32>,
) -> vec4<f32> {
    let rgb =
        color * parameter.x + color_source * parameter.y + color_destination * parameter.z;
    let alpha = parameter.x + parameter.y + parameter.z;
    return vec4<f32>(rgb, alpha);
}

fn alpha_blend(color: vec3<f32>, color_source: vec4<f32>, color_destination: vec4<f32>) -> vec4<f32> {
    switch uniforms.alpha_blend_type {
        case 1u {
            return overlap_rgba(
                color,
                color_source.rgb,
                color_destination.rgb,
                vec3<f32>(
                    color_source.a * color_destination.a,
                    0.0,
                    color_destination.a * (1.0 - color_source.a),
                ),
            );
        }
        case 2u {
            return overlap_rgba(
                color,
                color_source.rgb,
                color_destination.rgb,
                vec3<f32>(0.0, 0.0, color_destination.a * (1.0 - color_source.a)),
            );
        }
        case 3u {
            return overlap_rgba(
                color,
                color_source.rgb,
                color_destination.rgb,
                vec3<f32>(
                    min(color_source.a, color_destination.a),
                    max(color_source.a - color_destination.a, 0.0),
                    max(color_destination.a - color_source.a, 0.0),
                ),
            );
        }
        case 4u {
            return overlap_rgba(
                color,
                color_source.rgb,
                color_destination.rgb,
                vec3<f32>(
                    max(color_source.a + color_destination.a - 1.0, 0.0),
                    min(color_source.a, 1.0 - color_destination.a),
                    min(color_destination.a, 1.0 - color_source.a),
                ),
            );
        }
        default {
            return overlap_rgba(
                color,
                color_source.rgb,
                color_destination.rgb,
                vec3<f32>(
                    color_source.a * color_destination.a,
                    color_source.a * (1.0 - color_destination.a),
                    color_destination.a * (1.0 - color_source.a),
                ),
            );
        }
    }
}

@fragment
fn overlap_fs(input: VertexOutput) -> @location(0) vec4<f32> {
    let mask_value = sample_mask(input);
    let color_source = convert_premultiplied_to_straight(apply_compatible_source(input));
    let masked_source = vec4<f32>(color_source.rgb, color_source.a * mask_value);
    let color_destination =
        convert_premultiplied_to_straight(textureSample(destination_texture, tex_sampler, input.uv));

    return alpha_blend(
        color_blend(masked_source.rgb, color_destination.rgb),
        masked_source,
        color_destination,
    );
}
