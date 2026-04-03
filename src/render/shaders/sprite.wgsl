// ── SkyEngine 2D Instanced Sprite Shader ────────────────────────────────────
//
// Vertex shader: transform unit-quad corners by per-instance affine 2D
// transform, then apply the camera's orthographic view-projection matrix.
//
// Two fragment entry points:
//   fs_main       — textured (samples group 1 texture)
//   fs_color_only — solid colour

// ── Camera uniform (group 0, binding 0) ─────────────────────────────────────
struct CameraUniform {
    view_proj: mat4x4<f32>,
    camera: vec4<f32>,
    viewport: vec4<f32>,
};
@group(0) @binding(0)
var<uniform> camera: CameraUniform;

// ── Per-instance data (vertex buffer, instanced) ────────────────────────────
struct InstanceInput {
    @location(2) transform: vec4<f32>,   // x, y, width, height
    @location(3) rotation: vec4<f32>,    // sin(a), cos(a), _pad, _pad
    @location(4) color: vec4<f32>,       // RGBA tint
    @location(5) uv_rect: vec4<f32>,     // u_min, v_min, u_max, v_max
};

// ── Per-vertex data (quad corners) ──────────────────────────────────────────
struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
};

@vertex
fn vs_main(vert: VertexInput, inst: InstanceInput) -> VertexOutput {
    var out: VertexOutput;

    // Centre the quad: (0,0)→(1,1) becomes (-0.5,-0.5)→(0.5,0.5)
    let centred = vert.position - vec2<f32>(0.5, 0.5);

    // Scale by sprite size
    let scaled = centred * inst.transform.zw;

    // Rotate
    let sin_a = inst.rotation.x;
    let cos_a = inst.rotation.y;
    let rotated = vec2<f32>(
        scaled.x * cos_a - scaled.y * sin_a,
        scaled.x * sin_a + scaled.y * cos_a,
    );

    // Translate to world position
    let world_pos = rotated + inst.transform.xy;

    // Camera view-projection
    out.clip_position = camera.view_proj * vec4<f32>(world_pos, 0.0, 1.0);
    out.color = inst.color;
    out.uv = mix(inst.uv_rect.xy, inst.uv_rect.zw, vert.uv);

    return out;
}

// ── Texture + sampler (group 1, binding 0-1) ────────────────────────────────
@group(1) @binding(0)
var t_diffuse: texture_2d<f32>;
@group(1) @binding(1)
var s_diffuse: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let tex_color = textureSample(t_diffuse, s_diffuse, in.uv);
    return tex_color * in.color;
}

// ── Colour-only fragment (no texture binding needed) ────────────────────────
@fragment
fn fs_color_only(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}
