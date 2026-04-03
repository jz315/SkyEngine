@group(0) @binding(0)
var scene_tex: texture_2d<f32>;
@group(0) @binding(1)
var scene_sampler: sampler;
@group(0) @binding(2)
var light_tex: texture_2d<f32>;
@group(0) @binding(3)
var light_sampler: sampler;

@fragment
fn fs_main(in: FullscreenOutput) -> @location(0) vec4<f32> {
    let scene = textureSample(scene_tex, scene_sampler, in.uv);
    let light = textureSample(light_tex, light_sampler, in.uv);
    return vec4<f32>(scene.rgb * light.rgb, scene.a);
}
