struct CameraUniform {
    view_proj: mat4x4<f32>,
    camera: vec4<f32>,
    viewport: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

struct SpriteRecord {
    transform: vec4<f32>,   // x, y, width, height
    rotation: vec4<f32>,    // sin(a), cos(a), z, _pad
    color: vec4<f32>,
    uv_rect: vec4<f32>,
};

@group(1) @binding(0)
var<storage, read> sprite_table: array<SpriteRecord>;

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
};

struct InstanceInput {
    @location(2) slot_index: u32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
};

@vertex
fn vs_main(vert: VertexInput, inst: InstanceInput) -> VertexOutput {
    var out: VertexOutput;
    let sprite = sprite_table[inst.slot_index];
    let centred = vert.position - vec2<f32>(0.5, 0.5);
    let scaled = centred * sprite.transform.zw;

    let sin_a = sprite.rotation.x;
    let cos_a = sprite.rotation.y;
    let rotated = vec2<f32>(
        scaled.x * cos_a - scaled.y * sin_a,
        scaled.x * sin_a + scaled.y * cos_a,
    );

    let world_pos = vec3<f32>(rotated + sprite.transform.xy, sprite.rotation.z);
    out.clip_position = camera.view_proj * vec4<f32>(world_pos, 1.0);
    out.color = sprite.color;
    out.uv = mix(sprite.uv_rect.xy, sprite.uv_rect.zw, vert.uv);
    return out;
}

@group(2) @binding(0)
var t_diffuse: texture_2d<f32>;

@group(2) @binding(1)
var s_diffuse: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(t_diffuse, s_diffuse, in.uv) * in.color;
}

@fragment
fn fs_color_only(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}
