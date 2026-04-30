struct ViewUniform {
    view_proj: mat4x4<f32>,
    camera: vec4<f32>,
    viewport: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> camera: ViewUniform;

struct LightRecord {
    pos_radius: vec4<f32>,
    color: vec4<f32>,
    falloff: vec4<f32>,
    dir_shadow: vec4<f32>,
};

@group(1) @binding(0)
var<storage, read> light_table: array<LightRecord>;

@group(2) @binding(0)
var normal_texture: texture_2d<f32>;

@group(2) @binding(1)
var normal_sampler: sampler;

struct VertexInput {
    @location(0) corner: vec2<f32>,
    @location(1) slot_index: u32,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) local_uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) light_params: vec3<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let light = light_table[in.slot_index];
    let local = in.corner * 2.0 - 1.0;
    let world = light.pos_radius.xy + local * light.pos_radius.z;

    out.position = camera.view_proj * vec4<f32>(world, 0.0, 1.0);
    out.local_uv = local;
    out.color = light.color;
    out.light_params = vec3<f32>(light.pos_radius.z, light.falloff.x, light.falloff.y);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let dist = length(in.local_uv);
    if (dist > 1.0) {
        discard;
    }

    let radius = in.light_params.x;
    let falloff_power = in.light_params.y;
    let height = in.light_params.z;

    let attenuation = pow(1.0 - dist, falloff_power);
    let screen_uv = in.position.xy / camera.viewport.xy;
    let normal_val = textureSampleLevel(normal_texture, normal_sampler, screen_uv, 0.0).xyz;
    let normal = normalize(normal_val * 2.0 - 1.0);

    let delta = vec3<f32>(-in.local_uv * radius, height);
    let light_dir = normalize(delta);

    let half_lambert = dot(normal, light_dir) * 0.5 + 0.5;
    let ndotl = half_lambert * half_lambert;
    return vec4<f32>(in.color.rgb * attenuation * ndotl, in.color.a * attenuation);
}
