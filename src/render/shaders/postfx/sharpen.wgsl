struct SharpenUniform {
    params: vec4<f32>,
};

@group(0) @binding(0)
var input_tex: texture_2d<f32>;
@group(0) @binding(1)
var input_sampler: sampler;
@group(1) @binding(0)
var<uniform> sharpen: SharpenUniform;

fn sample_input(uv: vec2<f32>) -> vec3<f32> {
    return textureSample(input_tex, input_sampler, uv).rgb;
}

@fragment
fn fs_main(in: FullscreenOutput) -> @location(0) vec4<f32> {
    let dims = vec2<f32>(textureDimensions(input_tex));
    let texel = 1.0 / max(dims, vec2<f32>(1.0));
    let strength = max(sharpen.params.x, 0.0);
    let clamp_value = max(sharpen.params.y, 0.0);

    let center = sample_input(in.uv);
    let neighbor_average =
        (sample_input(in.uv + vec2<f32>(texel.x, 0.0)) +
         sample_input(in.uv - vec2<f32>(texel.x, 0.0)) +
         sample_input(in.uv + vec2<f32>(0.0, texel.y)) +
         sample_input(in.uv - vec2<f32>(0.0, texel.y))) * 0.25;

    let detail = clamp(center - neighbor_average, vec3<f32>(-clamp_value), vec3<f32>(clamp_value));
    return vec4<f32>(max(center + detail * strength, vec3<f32>(0.0)), 1.0);
}
