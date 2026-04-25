struct ViewUniform {
    view_proj: mat4x4<f32>,
    camera: vec4<f32>,
    viewport: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> camera: ViewUniform;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(8) model_col0: vec4<f32>,
    @location(9) model_col1: vec4<f32>,
    @location(10) model_col2: vec4<f32>,
    @location(11) model_col3: vec4<f32>,
};

struct SpriteMaterialUniform {
    color: vec4<f32>,
    uv_rect: vec4<f32>,
};

@group(1) @binding(0)
var<uniform> sprite: SpriteMaterialUniform;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    let model = mat4x4<f32>(
        input.model_col0,
        input.model_col1,
        input.model_col2,
        input.model_col3,
    );

    let centered = vec4<f32>(input.position.xy - vec2<f32>(0.5, 0.5), input.position.z, 1.0);
    let world_pos = model * centered;

    output.clip_position = camera.view_proj * world_pos;
    output.color = sprite.color;
    output.uv = mix(sprite.uv_rect.xy, sprite.uv_rect.zw, input.uv);
    return output;
}

@group(1) @binding(1)
var t_diffuse: texture_2d<f32>;
@group(1) @binding(2)
var s_diffuse: sampler;

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(t_diffuse, s_diffuse, input.uv) * input.color;
}
