// Live2D WGSL shader for SkyEngine.
//
// Ported from SakuraEngine's live2d.cxx (HLSL-like cross-platform shader).
// Two fragment entry points: mask_fs (mask generation) and model_fs (model rendering).

// ── Uniforms ────────────────────────────────────────────────────────────

struct Live2DUniforms {
    projection_matrix: mat4x4<f32>,
    clip_matrix: mat4x4<f32>,
    base_color: vec4<f32>,
    multiply_color: vec4<f32>,
    screen_color: vec4<f32>,
    channel_flag: vec4<f32>,
    use_mask: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
};

@group(0) @binding(0) var<uniform> uniforms: Live2DUniforms;
@group(1) @binding(0) var color_texture: texture_2d<f32>;
@group(1) @binding(1) var mask_texture: texture_2d<f32>;
@group(1) @binding(2) var tex_sampler: sampler;

// ── Vertex ──────────────────────────────────────────────────────────────

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) sv_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) clip_pos: vec4<f32>,
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    let pos4 = vec4<f32>(input.position, 0.0, 1.0);
    output.sv_position = uniforms.projection_matrix * pos4;
    output.clip_pos = uniforms.clip_matrix * pos4;
    output.uv = vec2<f32>(input.uv.x, 1.0 - input.uv.y);
    return output;
}

// ── Mask Fragment ───────────────────────────────────────────────────────

// Generates mask values into the mask texture.
// base_color holds the clip bounds (x_min, y_min, x_max, y_max).
@fragment
fn mask_fs(input: VertexOutput) -> @location(0) vec4<f32> {
    let clip_uv = input.clip_pos.xy / input.clip_pos.w;

    let is_inside = step(uniforms.base_color.x, clip_uv.x)
                  * step(uniforms.base_color.y, clip_uv.y)
                  * step(clip_uv.x, uniforms.base_color.z)
                  * step(clip_uv.y, uniforms.base_color.w);

    let tex_alpha = textureSample(color_texture, tex_sampler, input.uv).a;
    return uniforms.channel_flag * tex_alpha * is_inside;
}

// ── Model Fragment ──────────────────────────────────────────────────────

// Forward rendering of Live2D drawables with optional mask sampling.
@fragment
fn model_fs(input: VertexOutput) -> @location(0) vec4<f32> {
    var tex_color = textureSample(color_texture, tex_sampler, input.uv);

    // Apply multiply color
    tex_color = vec4<f32>(tex_color.rgb * uniforms.multiply_color.rgb, tex_color.a);

    // Apply screen color: result = a + b - a*b
    let screen_sum = tex_color.rgb + uniforms.screen_color.rgb;
    let screen_product = tex_color.rgb * uniforms.screen_color.rgb;
    tex_color = vec4<f32>(screen_sum - screen_product, tex_color.a);

    // Apply base color (opacity etc.)
    var output_color = tex_color * uniforms.base_color;

    // Premultiply alpha
    output_color = vec4<f32>(output_color.rgb * output_color.a, output_color.a);

    // Mask sampling
    if uniforms.use_mask > 0.5 {
        let mask_uv = input.clip_pos.xy / input.clip_pos.w;
        let clip_mask = (vec4<f32>(1.0) - textureSample(mask_texture, tex_sampler, mask_uv)) * uniforms.channel_flag;
        let mask_value = clip_mask.r + clip_mask.g + clip_mask.b + clip_mask.a;
        output_color = output_color * mask_value;
    }

    return output_color;
}
