struct BloomUniform {
    params: vec4<f32>,
    texel_dir: vec4<f32>,
};

@group(0) @binding(0)
var input_tex: texture_2d<f32>;
@group(0) @binding(1)
var input_sampler: sampler;
@group(1) @binding(0)
var<uniform> bloom: BloomUniform;

fn sample_input(uv: vec2<f32>, offset: vec2<f32>) -> vec3<f32> {
    return max(
        textureSample(input_tex, input_sampler, uv + offset * bloom.texel_dir.xy).rgb,
        vec3<f32>(0.0),
    );
}

@fragment
fn fs_downsample(in: FullscreenOutput) -> @location(0) vec4<f32> {
    let uv = in.uv;

    let a = sample_input(uv, vec2<f32>(-2.0,  2.0));
    let b = sample_input(uv, vec2<f32>( 0.0,  2.0));
    let c = sample_input(uv, vec2<f32>( 2.0,  2.0));
    let d = sample_input(uv, vec2<f32>(-2.0,  0.0));
    let e = sample_input(uv, vec2<f32>( 0.0,  0.0));
    let f = sample_input(uv, vec2<f32>( 2.0,  0.0));
    let g = sample_input(uv, vec2<f32>(-2.0, -2.0));
    let h = sample_input(uv, vec2<f32>( 0.0, -2.0));
    let i = sample_input(uv, vec2<f32>( 2.0, -2.0));
    let j = sample_input(uv, vec2<f32>(-1.0,  1.0));
    let k = sample_input(uv, vec2<f32>( 1.0,  1.0));
    let l = sample_input(uv, vec2<f32>(-1.0, -1.0));
    let m = sample_input(uv, vec2<f32>( 1.0, -1.0));

    let color =
        e * 0.125 +
        (a + c + g + i) * 0.03125 +
        (b + d + f + h) * 0.0625 +
        (j + k + l + m) * 0.125;
    return vec4<f32>(color, 1.0);
}

@fragment
fn fs_blur(in: FullscreenOutput) -> @location(0) vec4<f32> {
    let spread = max(bloom.params.y, 0.0);
    let step_uv = bloom.texel_dir.xy * bloom.texel_dir.zw * spread;

    var color = textureSample(input_tex, input_sampler, in.uv - step_uv * 4.0).rgb * 0.00390625;
    color += textureSample(input_tex, input_sampler, in.uv - step_uv * 3.0).rgb * 0.03125;
    color += textureSample(input_tex, input_sampler, in.uv - step_uv * 2.0).rgb * 0.109375;
    color += textureSample(input_tex, input_sampler, in.uv - step_uv).rgb * 0.21875;
    color += textureSample(input_tex, input_sampler, in.uv).rgb * 0.2734375;
    color += textureSample(input_tex, input_sampler, in.uv + step_uv).rgb * 0.21875;
    color += textureSample(input_tex, input_sampler, in.uv + step_uv * 2.0).rgb * 0.109375;
    color += textureSample(input_tex, input_sampler, in.uv + step_uv * 3.0).rgb * 0.03125;
    color += textureSample(input_tex, input_sampler, in.uv + step_uv * 4.0).rgb * 0.00390625;
    return vec4<f32>(max(color, vec3<f32>(0.0)), 1.0);
}

@fragment
fn fs_upsample(in: FullscreenOutput) -> @location(0) vec4<f32> {
    let spread = max(bloom.params.y, 0.0);
    let texel = bloom.texel_dir.xy * max(spread, 0.001);
    let weight = mix(0.25, 0.90, clamp(0.5 * spread, 0.0, 1.0));

    var color = textureSample(input_tex, input_sampler, in.uv).rgb * 4.0;
    color += textureSample(input_tex, input_sampler, in.uv + texel * vec2<f32>( 1.0,  0.0)).rgb * 2.0;
    color += textureSample(input_tex, input_sampler, in.uv + texel * vec2<f32>(-1.0,  0.0)).rgb * 2.0;
    color += textureSample(input_tex, input_sampler, in.uv + texel * vec2<f32>( 0.0,  1.0)).rgb * 2.0;
    color += textureSample(input_tex, input_sampler, in.uv + texel * vec2<f32>( 0.0, -1.0)).rgb * 2.0;
    color += textureSample(input_tex, input_sampler, in.uv + texel * vec2<f32>( 1.0,  1.0)).rgb;
    color += textureSample(input_tex, input_sampler, in.uv + texel * vec2<f32>(-1.0,  1.0)).rgb;
    color += textureSample(input_tex, input_sampler, in.uv + texel * vec2<f32>( 1.0, -1.0)).rgb;
    color += textureSample(input_tex, input_sampler, in.uv + texel * vec2<f32>(-1.0, -1.0)).rgb;
    color = max(color * 0.0625, vec3<f32>(0.0));

    return vec4<f32>(color * weight, 1.0);
}

@group(0) @binding(2)
var bloom_tex: texture_2d<f32>;
@group(0) @binding(3)
var bloom_sampler: sampler;

@fragment
fn fs_combine(in: FullscreenOutput) -> @location(0) vec4<f32> {
    let intensity = max(bloom.params.x, 0.0);
    let base = textureSample(input_tex, input_sampler, in.uv);
    let blur = max(textureSample(bloom_tex, bloom_sampler, in.uv).rgb, vec3<f32>(0.0));
    let blend = clamp(0.12 * intensity, 0.0, 1.0);
    return vec4<f32>(mix(base.rgb, blur, blend), base.a);
}
