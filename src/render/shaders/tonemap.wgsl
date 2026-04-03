struct ToneMapUniform {
    params: vec4<f32>,
};

@group(0) @binding(0)
var input_tex: texture_2d<f32>;
@group(0) @binding(1)
var input_sampler: sampler;
@group(1) @binding(0)
var<uniform> tonemap: ToneMapUniform;

fn aces_tonemap(x: vec3<f32>) -> vec3<f32> {
    let a = 2.51;
    let b = 0.03;
    let c = 2.43;
    let d = 0.59;
    let e = 0.14;
    return clamp((x * (a * x + b)) / (x * (c * x + d) + e), vec3<f32>(0.0), vec3<f32>(1.0));
}

@fragment
fn fs_main(in: FullscreenOutput) -> @location(0) vec4<f32> {
    let exposure = tonemap.params.x;
    let gamma = tonemap.params.y;

    var color = textureSample(input_tex, input_sampler, in.uv).rgb * exposure;
    color = aces_tonemap(color);
    color = pow(color, vec3<f32>(1.0 / gamma));
    return vec4<f32>(color, 1.0);
}
