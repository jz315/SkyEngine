struct CameraUniform {
    view_proj: mat4x4<f32>,
    camera: vec4<f32>,
    viewport: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

@group(1) @binding(0)
var normal_texture: texture_2d<f32>;
@group(1) @binding(1)
var normal_sampler: sampler;

struct VertexInput {
    @location(0) corner: vec2<f32>,
    @location(1) pos_radius: vec4<f32>,
    @location(2) color: vec4<f32>,
    @location(3) falloff: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) local_uv: vec2<f32>,
    @location(1) screen_uv: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) radius: f32,
    @location(4) falloff: f32,
    @location(5) height: f32,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;

    let local = in.corner * 2.0 - vec2<f32>(1.0, 1.0);
    let world = in.pos_radius.xy + local * in.pos_radius.z;
    let clip = camera.view_proj * vec4<f32>(world, 0.0, 1.0);

    out.position = clip;
    out.local_uv = local;
    out.screen_uv = vec2<f32>(clip.x * 0.5 + 0.5, 1.0 - (clip.y * 0.5 + 0.5));
    out.color = in.color;
    out.radius = in.pos_radius.z;
    out.falloff = in.falloff.x;
    out.height = in.falloff.y;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let dist = length(in.local_uv);
    if (dist > 1.0) {
        discard;
    }

    let attenuation = pow(max(1.0 - dist * dist, 0.0), in.falloff);
    let normal = normalize(textureSample(normal_texture, normal_sampler, in.screen_uv).xyz * 2.0 - 1.0);
    let delta = vec3<f32>(-in.local_uv * in.radius, in.height);
    let light_dir = normalize(delta);
    let ndotl = max(dot(normal, light_dir), 0.0);
    let diffuse = mix(0.3, 1.0, ndotl);

    return vec4<f32>(in.color.rgb * attenuation * diffuse, in.color.a * attenuation);
}
