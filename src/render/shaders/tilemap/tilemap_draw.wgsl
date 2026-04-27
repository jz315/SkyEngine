struct ViewUniform {
    view_proj: mat4x4<f32>,
    camera: vec4<f32>,
    viewport: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> camera: ViewUniform;

@group(1) @binding(0)
var<uniform> model: mat4x4<f32>;

@group(2) @binding(0)
var t_diffuse: texture_2d<f32>;
@group(2) @binding(1)
var s_diffuse: sampler;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
};

struct InstanceInput {
    @location(2) origin: vec4<f32>,
    @location(3) axis_x: vec4<f32>,
    @location(4) axis_y: vec4<f32>,
    @location(5) color: vec4<f32>,
    @location(6) uv_origin: vec4<f32>,
    @location(7) uv_axis_x: vec4<f32>,
    @location(8) uv_axis_y: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) uv_min: vec2<f32>,
    @location(3) uv_max: vec2<f32>,
};

@vertex
fn vs_main(vert: VertexInput, inst: InstanceInput) -> VertexOutput {
    var out: VertexOutput;
    let local_position =
        inst.origin.xyz + inst.axis_x.xyz * vert.position.x + inst.axis_y.xyz * vert.position.y;
    let world_position = (model * vec4<f32>(local_position, 1.0)).xyz;
    out.clip_position = camera.view_proj * vec4<f32>(world_position, 1.0);
    out.color = inst.color;
    out.uv = inst.uv_origin.xy + inst.uv_axis_x.xy * vert.uv.x + inst.uv_axis_y.xy * vert.uv.y;
    let p00 = inst.uv_origin.xy;
    let p10 = inst.uv_origin.xy + inst.uv_axis_x.xy;
    let p01 = inst.uv_origin.xy + inst.uv_axis_y.xy;
    let p11 = inst.uv_origin.xy + inst.uv_axis_x.xy + inst.uv_axis_y.xy;
    out.uv_min = min(min(p00, p10), min(p01, p11));
    out.uv_max = max(max(p00, p10), max(p01, p11));
    return out;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let texture_size = vec2<f32>(textureDimensions(t_diffuse));
    let half_texel = vec2<f32>(0.5, 0.5) / max(texture_size, vec2<f32>(1.0, 1.0));
    let uv_center = (input.uv_min + input.uv_max) * 0.5;
    let uv_extent = max((input.uv_max - input.uv_min) * 0.5 - half_texel, vec2<f32>(0.0, 0.0));
    let uv = clamp(input.uv, uv_center - uv_extent, uv_center + uv_extent);
    return textureSample(t_diffuse, s_diffuse, uv) * input.color;
}
