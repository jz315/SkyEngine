struct ShadowPassUniform {
    raster_view_proj: mat4x4<f32>,
    depth_view_proj: mat4x4<f32>,
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
    @location(1) depth_clip: vec4<f32>,
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
    output.clip_position = shadow_pass.raster_view_proj * world_position;
    output.depth_clip = shadow_pass.depth_view_proj * world_position;
    output.uv = input.uv;
    return output;
}

fn depth_from_clip(clip_position: vec4<f32>) -> f32 {
    let inv_w = select(1.0, 1.0 / clip_position.w, abs(clip_position.w) > 0.000001);
    return clamp(clip_position.z * inv_w, 0.0, 1.0);
}

struct FragmentOutput {
    @location(0) color: vec4<f32>,
    @builtin(frag_depth) depth: f32,
};

@fragment
fn fs_main(input: VertexOutput) -> FragmentOutput {
    let base = textureSample(t_albedo, s_albedo, input.uv) * material.albedo;
    let opacity = clamp(base.a, 0.0, 1.0);
    if (opacity <= 0.001) {
        discard;
    }

    let depth = depth_from_clip(input.depth_clip);
    let tint = clamp(base.rgb, vec3<f32>(0.0), vec3<f32>(1.0));
    let transmittance = tint * (1.0 - opacity);
    // Wicked keeps transparent secondary depth with a MAX alpha blend and a
    // `transparent_shadow.a > cmp` check. SkyEngine's depth atlas is
    // non-reversed, so store the reversed key here and keep the consumer shape.
    let secondary_depth_key = 1.0 - depth;
    return FragmentOutput(vec4<f32>(transmittance, secondary_depth_key), depth);
}
