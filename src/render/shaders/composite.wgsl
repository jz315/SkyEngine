@group(0) @binding(0) var scene_tex: texture_2d<f32>;
@group(0) @binding(1) var scene_sampler: sampler;
@group(0) @binding(2) var light_tex: texture_2d<f32>;
@group(0) @binding(3) var light_sampler: sampler;

@fragment
fn fs_main(in: FullscreenOutput) -> @location(0) vec4<f32> {
    let scene = textureSample(scene_tex, scene_sampler, in.uv);
    let light = textureSample(light_tex, light_sampler, in.uv);

    let emissive_threshold = 1.0;
    let emissive = max(scene.rgb - emissive_threshold, vec3<f32>(0.0));
    
    // 💡 修复 3B：引入基础全局环境光 (Global Ambient)
    // 对抗 ACES 阴影压黑，防止没有光源照到的地方变成死黑。
    // 这里的值 (0.1, 0.12, 0.15) 偏冷调暗蓝紫的天光氛围，你可以根据美术需求自行修改
    let global_ambient = vec3<f32>(0.1, 0.12, 0.15); 
    
    // 累积光源与底色环境光相加
    let total_light = light.rgb + global_ambient;
    let diffuse = scene.rgb * total_light;

    return vec4<f32>(diffuse + emissive, scene.a);
}
