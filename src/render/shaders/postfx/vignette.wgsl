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
    
    // Original cutoff was 0.5, which hard-clipped the mid-edges forming a black oval.
    // Changing to 0.75 pushes the dark gradient to the screen corners (max dist ~0.707).
    let vig = 1.0 - smoothstep(0.75 - smoothness, 0.75, dist * (1.0 + intensity));
    return vec4<f32>(color.rgb * vig, color.a);
}
