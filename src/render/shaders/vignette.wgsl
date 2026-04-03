struct VignetteUniform {
    params: vec4<f32>,
};

@group(0) @binding(0)
var input_tex: texture_2d<f32>;
@group(0) @binding(1)
var input_sampler: sampler;
@group(1) @binding(0)
var<uniform> vignette: VignetteUniform;

@fragment
fn fs_main(in: FullscreenOutput) -> @location(0) vec4<f32> {
    let intensity = vignette.params.x;
    let smoothness = vignette.params.y;
    let color = textureSample(input_tex, input_sampler, in.uv);
    let dist = length(in.uv - vec2<f32>(0.5, 0.5));
    let vig = 1.0 - smoothstep(0.5 - smoothness, 0.5, dist * (1.0 + intensity));
    return vec4<f32>(color.rgb * vig, color.a);
}
