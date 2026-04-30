struct ViewUniform {
    view_proj: mat4x4<f32>,
    camera: vec4<f32>,
    viewport: vec4<f32>,
    view: mat4x4<f32>,
};

@group(0) @binding(0)
var<uniform> camera: ViewUniform;

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
@group(1) @binding(3)
var t_emissive: texture_2d<f32>;
@group(1) @binding(4)
var t_normal: texture_2d<f32>;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(8) model_col0: vec4<f32>,
    @location(9) model_col1: vec4<f32>,
    @location(10) model_col2: vec4<f32>,
    @location(11) model_col3: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) view_normal: vec3<f32>,
};

struct FragmentOutput {
    @location(0) albedo: vec4<f32>,
    @location(1) material: vec4<f32>,
    @location(2) emissive: vec4<f32>,
    @location(3) encoded_normal: vec4<f32>,
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
    let world_position = model * vec4<f32>(input.position, 1.0);
    let world_normal = (model * vec4<f32>(input.normal, 0.0)).xyz;
    output.clip_position = camera.view_proj * world_position;
    output.uv = input.uv;
    output.view_normal = safe_normalize((camera.view * vec4<f32>(world_normal, 0.0)).xyz);
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> FragmentOutput {
    let base = textureSample(t_albedo, s_albedo, input.uv) * material.albedo;
    if (material.shadow.z > 0.5 && base.a < material.shadow.y) {
        discard;
    }
    let emissive_sample = textureSample(t_emissive, s_albedo, input.uv).rgb;
    let emissive = emissive_sample * material.emissive.rgb;

    var output: FragmentOutput;
    output.albedo = vec4<f32>(base.rgb, base.a);
    output.material = vec4<f32>(
        clamp(material.params.y, 0.0, 1.0),
        clamp(material.params.x, 0.0, 1.0),
        1.0,
        base.a,
    );
    output.emissive = vec4<f32>(emissive, base.a);
    output.encoded_normal = vec4<f32>(input.view_normal * 0.5 + vec3<f32>(0.5), 1.0);
    return output;
}
