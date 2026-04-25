struct ViewUniform {
    view_proj: mat4x4<f32>,
    camera: vec4<f32>,
    viewport: vec4<f32>,
    view: mat4x4<f32>,
};

struct PreviousViewProjUniform {
    prev_view_proj: mat4x4<f32>,
};

@group(0) @binding(0)
var<uniform> camera: ViewUniform;

@group(1) @binding(0)
var<uniform> previous_view: PreviousViewProjUniform;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(8) model_col0: vec4<f32>,
    @location(9) model_col1: vec4<f32>,
    @location(10) model_col2: vec4<f32>,
    @location(11) model_col3: vec4<f32>,
    @location(12) prev_model_col0: vec4<f32>,
    @location(13) prev_model_col1: vec4<f32>,
    @location(14) prev_model_col2: vec4<f32>,
    @location(15) prev_model_col3: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) view_normal: vec3<f32>,
    @location(1) prev_clip: vec4<f32>,
};

struct FragmentOutput {
    @location(0) encoded_normal: vec4<f32>,
    @location(1) velocity: vec4<f32>,
};

fn safe_normalize(v: vec3<f32>) -> vec3<f32> {
    let len_sq = dot(v, v);
    if (len_sq <= 0.000001) {
        return vec3<f32>(0.0, 0.0, 1.0);
    }
    return v * inverseSqrt(len_sq);
}

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;

    let model = mat4x4<f32>(
        input.model_col0,
        input.model_col1,
        input.model_col2,
        input.model_col3,
    );
    let prev_model = mat4x4<f32>(
        input.prev_model_col0,
        input.prev_model_col1,
        input.prev_model_col2,
        input.prev_model_col3,
    );

    let world_position = model * vec4<f32>(input.position, 1.0);
    let prev_world_position = prev_model * vec4<f32>(input.position, 1.0);
    let world_normal = (model * vec4<f32>(input.normal, 0.0)).xyz;
    let view_normal = (camera.view * vec4<f32>(world_normal, 0.0)).xyz;

    output.clip_position = camera.view_proj * world_position;
    output.view_normal = safe_normalize(view_normal);
    output.prev_clip = previous_view.prev_view_proj * prev_world_position;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> FragmentOutput {
    var output: FragmentOutput;

    let encoded = input.view_normal * 0.5 + vec3<f32>(0.5, 0.5, 0.5);
    let current_uv = input.clip_position.xy * camera.viewport.zw;

    var velocity = vec2<f32>(0.0, 0.0);
    var prev_depth = 1.0;
    if (abs(input.prev_clip.w) > 0.00001) {
        let prev_ndc = input.prev_clip.xyz / input.prev_clip.w;
        let prev_uv = vec2<f32>(prev_ndc.x * 0.5 + 0.5, prev_ndc.y * -0.5 + 0.5);
        velocity = prev_uv - current_uv;
        prev_depth = prev_ndc.z;
    }

    output.encoded_normal = vec4<f32>(encoded, 1.0);
    output.velocity = vec4<f32>(velocity, prev_depth, 1.0);
    return output;
}
