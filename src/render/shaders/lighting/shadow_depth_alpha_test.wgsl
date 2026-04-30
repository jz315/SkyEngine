struct ShadowPassUniform {
    light_view_proj: mat4x4<f32>,
};

@group(0) @binding(0)
var<uniform> shadow_pass: ShadowPassUniform;

struct StandardUniform {
    albedo: vec4<f32>,
    emissive: vec4<f32>,
    params: vec4<f32>,
    // x: receive shadows flag, y: alpha cutoff, z: alpha-test flag.
    shadow: vec4<f32>,
};

@group(1) @binding(0)
var<uniform> material: StandardUniform;
@group(1) @binding(1)
var t_albedo: texture_2d<f32>;
@group(1) @binding(2)
var s_albedo: sampler;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(8) model_col0: vec4<f32>,
    @location(9) model_col1: vec4<f32>,
    @location(10) model_col2: vec4<f32>,
    @location(11) model_col3: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
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
    let world_position = model * vec4<f32>(input.position, 1.0);
    output.clip_position = shadow_pass.light_view_proj * world_position;
    output.uv = input.uv;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) {
    let base = textureSample(t_albedo, s_albedo, input.uv) * material.albedo;
    if (material.shadow.z > 0.5 && base.a < material.shadow.y) {
        discard;
    }
}
