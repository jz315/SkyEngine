struct CameraUniform {
    view_proj: mat4x4<f32>,
    camera: vec4<f32>,
    viewport: vec4<f32>,
};

@group(0) @binding(0) var<uniform> camera: CameraUniform;
@group(1) @binding(0) var normal_texture: texture_2d<f32>;
@group(1) @binding(1) var normal_sampler: sampler; // kept for bind group layout compatibility

struct VertexInput {
    @location(0) corner: vec2<f32>,
    @location(1) pos_radius: vec4<f32>,
    @location(2) color: vec4<f32>,
    @location(3) falloff: vec4<f32>, 
};

struct VertexOutput {
    // 传到 Fragment Shader 时，xy 就是绝对的屏幕物理像素坐标
    @builtin(position) position: vec4<f32>, 
    @location(0) local_uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    // 📦 优化：将原本分散的 3 个 f32 压缩打包为 vec3，大幅减轻移动端 GPU 的管线压力
    // x: radius, y: falloff, z: height
    @location(2) light_params: vec3<f32>, 
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;

    // WGSL 支持向量和标量直接运算，精简写法
    let local = in.corner * 2.0 - 1.0; 
    let world = in.pos_radius.xy + local * in.pos_radius.z;
    
    out.position = camera.view_proj * vec4<f32>(world, 0.0, 1.0);
    out.local_uv = local;
    out.color = in.color;
    out.light_params = vec3<f32>(in.pos_radius.z, in.falloff.x, in.falloff.y);
    
    // ✂️ 删除了易引发畸变且毫无必要的 screen_uv 手动计算
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // 💡 修复 2：恢复真实的物理距离，找回极其平滑、自然发散的光晕
    let dist = length(in.local_uv);
    if (dist > 1.0) {
        discard;
    }

    let radius = in.light_params.x;
    let falloff_power = in.light_params.y;
    let height = in.light_params.z;

    // 移除生硬的 smoothstep 截断，使用连贯的自然衰减曲线
    let attenuation = pow(1.0 - dist, falloff_power);

    // 💡 修复 1：恢复 UV 平滑采样，彻底消灭“马赛克感/锯齿/低分辨率”
    // 使用 textureSampleLevel(, 0.0) 享受硬件双线性过滤，且安全免疫报错
    let screen_uv = in.position.xy / camera.viewport.zw;
    let normal_val = textureSampleLevel(normal_texture, normal_sampler, screen_uv, 0.0).xyz;
    let normal = normalize(normal_val * 2.0 - 1.0);

    // 计算光照方向
    let delta = vec3<f32>(-in.local_uv * radius, height);
    let light_dir = normalize(delta);
    
    // 💡 修复 3A：采用“半兰伯特 (Half-Lambert)”光照模型取代 max(..., 0.0)
    // 将 dot 产生的 [-1, 1] 映射到 [0, 1]，让物体背光面也能有被漫反射“包裹”的体积感
    let half_lambert = dot(normal, light_dir) * 0.5 + 0.5;
    
    // 可以将其平方增加明暗面的立体对比度
    let ndotl = half_lambert * half_lambert; 

    // 预乘 Alpha 输出格式
    return vec4<f32>(in.color.rgb * attenuation * ndotl, in.color.a * attenuation);
}