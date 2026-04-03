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

@fragment
fn fs_bright(in: FullscreenOutput) -> @location(0) vec4<f32> {
    let threshold = bloom.params.x;
    let color = textureSample(input_tex, input_sampler, in.uv);
    let brightness = dot(color.rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
    let soft = max(brightness - threshold, 0.0);
    let contribution = soft / (soft + 1.0);
    return vec4<f32>(color.rgb * contribution, 1.0);
}

@fragment
fn fs_downsample(in: FullscreenOutput) -> @location(0) vec4<f32> {
    let texel = bloom.texel_dir.xy;
    let a = textureSample(input_tex, input_sampler, in.uv + texel * vec2<f32>(-0.5, -0.5));
    let b = textureSample(input_tex, input_sampler, in.uv + texel * vec2<f32>(0.5, -0.5));
    let c = textureSample(input_tex, input_sampler, in.uv + texel * vec2<f32>(-0.5, 0.5));
    let d = textureSample(input_tex, input_sampler, in.uv + texel * vec2<f32>(0.5, 0.5));
    return (a + b + c + d) * 0.25;
}

@fragment
fn fs_blur(in: FullscreenOutput) -> @location(0) vec4<f32> {
    let texel = bloom.texel_dir.xy * bloom.params.z;
    let dir = bloom.texel_dir.zw;
    let w0 = 0.227027;
    let w1 = 0.1945946;
    let w2 = 0.1216216;
    let w3 = 0.054054;
    let w4 = 0.016216;

    let o1 = dir * texel * 1.0;
    let o2 = dir * texel * 2.0;
    let o3 = dir * texel * 3.0;
    let o4 = dir * texel * 4.0;

    var result = textureSample(input_tex, input_sampler, in.uv) * w0;
    result += textureSample(input_tex, input_sampler, in.uv + o1) * w1;
    result += textureSample(input_tex, input_sampler, in.uv - o1) * w1;
    result += textureSample(input_tex, input_sampler, in.uv + o2) * w2;
    result += textureSample(input_tex, input_sampler, in.uv - o2) * w2;
    result += textureSample(input_tex, input_sampler, in.uv + o3) * w3;
    result += textureSample(input_tex, input_sampler, in.uv - o3) * w3;
    result += textureSample(input_tex, input_sampler, in.uv + o4) * w4;
    result += textureSample(input_tex, input_sampler, in.uv - o4) * w4;
    return result;
}

@fragment
fn fs_upsample(in: FullscreenOutput) -> @location(0) vec4<f32> {
    return textureSample(input_tex, input_sampler, in.uv);
}

@group(0) @binding(2)
var bloom_tex: texture_2d<f32>;
@group(0) @binding(3)
var bloom_sampler: sampler;

@fragment
fn fs_combine(in: FullscreenOutput) -> @location(0) vec4<f32> {
    let intensity = bloom.params.y;
    let base = textureSample(input_tex, input_sampler, in.uv);
    let blur = textureSample(bloom_tex, bloom_sampler, in.uv);
    return vec4<f32>(base.rgb + blur.rgb * intensity, base.a);
}
