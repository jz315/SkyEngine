struct ShadowPassUniform {
    raster_view_proj: mat4x4<f32>,
    depth_view_proj: mat4x4<f32>,
};

@group(0) @binding(0)
var<uniform> shadow_pass: ShadowPassUniform;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(8) model_col0: vec4<f32>,
    @location(9) model_col1: vec4<f32>,
    @location(10) model_col2: vec4<f32>,
    @location(11) model_col3: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) depth_clip: vec4<f32>,
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
    output.clip_position = shadow_pass.raster_view_proj * world_position;
    output.depth_clip = shadow_pass.depth_view_proj * world_position;
    return output;
}

fn depth_from_clip(clip_position: vec4<f32>) -> f32 {
    let inv_w = select(1.0, 1.0 / clip_position.w, abs(clip_position.w) > 0.000001);
    return clamp(clip_position.z * inv_w, 0.0, 1.0);
}

@fragment
fn fs_main(input: VertexOutput) -> @builtin(frag_depth) f32 {
    return depth_from_clip(input.depth_clip);
}
