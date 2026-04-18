struct ViewUniform {
    view_proj: mat4x4<f32>,
    camera: vec4<f32>,
    viewport: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> camera: ViewUniform;

struct StandardUniform {
    albedo: vec4<f32>,
    emissive: vec4<f32>,
    params: vec4<f32>,
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

struct ShadowUniform {
    light_view_proj: mat4x4<f32>,
    light_direction: vec4<f32>,
    shadow_params: vec4<f32>,
};

struct LightRecord {
    pos_radius: vec4<f32>,
    color: vec4<f32>,
    falloff: vec4<f32>,
};

struct LightTableMeta {
    count: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
};

@group(3) @binding(0)
var<storage, read> light_table: array<LightRecord>;
@group(3) @binding(1)
var<uniform> light_meta: LightTableMeta;
@group(3) @binding(2)
var<uniform> shadow: ShadowUniform;
@group(3) @binding(3)
var t_shadow: texture_depth_2d;
@group(3) @binding(4)
var s_shadow: sampler_comparison;

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
    @location(0) world_position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

fn safe_normalize(v: vec3<f32>) -> vec3<f32> {
    let len_sq = dot(v, v);
    if (len_sq <= 0.000001) {
        return vec3<f32>(0.0, 0.0, 1.0);
    }
    return v * inverseSqrt(len_sq);
}

fn fresnel_schlick(cos_theta: f32, f0: vec3<f32>) -> vec3<f32> {
    let m = clamp(1.0 - cos_theta, 0.0, 1.0);
    let m2 = m * m;
    let m5 = m2 * m2 * m;
    return f0 + (vec3<f32>(1.0) - f0) * m5;
}

fn distribution_ggx(normal: vec3<f32>, half_dir: vec3<f32>, roughness: f32) -> f32 {
    let a = roughness * roughness;
    let a2 = a * a;
    let ndoth = max(dot(normal, half_dir), 0.0);
    let ndoth2 = ndoth * ndoth;
    let denom = ndoth2 * (a2 - 1.0) + 1.0;
    return a2 / max(3.14159265 * denom * denom, 0.0001);
}

fn geometry_schlick_ggx(ndotv: f32, roughness: f32) -> f32 {
    let r = roughness + 1.0;
    let k = (r * r) * 0.125;
    return ndotv / max(ndotv * (1.0 - k) + k, 0.0001);
}

fn geometry_smith(normal: vec3<f32>, view_dir: vec3<f32>, light_dir: vec3<f32>, roughness: f32) -> f32 {
    return geometry_schlick_ggx(max(dot(normal, view_dir), 0.0), roughness)
        * geometry_schlick_ggx(max(dot(normal, light_dir), 0.0), roughness);
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
    let world_normal = model * vec4<f32>(input.normal, 0.0);
    output.clip_position = camera.view_proj * world_position;
    output.world_position = world_position.xyz;
    output.normal = normalize(world_normal.xyz);
    output.uv = input.uv;
    return output;
}

fn shadow_visibility(world_position: vec3<f32>) -> f32 {
    if (shadow.shadow_params.w < 0.5) {
        return 1.0;
    }

    let light_clip = shadow.light_view_proj * vec4<f32>(world_position, 1.0);
    let inv_w = select(1.0, 1.0 / light_clip.w, abs(light_clip.w) > 0.00001);
    let light_ndc = light_clip.xyz * inv_w;
    let depth = light_ndc.z;
    if (depth <= 0.0 || depth >= 1.0) {
        return 1.0;
    }

    let uv = vec2<f32>(light_ndc.x * 0.5 + 0.5, light_ndc.y * -0.5 + 0.5);
    if (any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0))) {
        return 1.0;
    }

    return textureSampleCompare(t_shadow, s_shadow, uv, depth - shadow.shadow_params.x);
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let base = textureSample(t_albedo, s_albedo, input.uv) * material.albedo;
    let normal = safe_normalize(input.normal);
    let view_dir = safe_normalize(camera.camera.xyz - input.world_position);
    let metallic = clamp(material.params.x, 0.0, 1.0);
    let roughness = clamp(material.params.y, 0.05, 1.0);
    let f0 = mix(vec3<f32>(0.04), base.rgb, metallic);
    let ndotv = max(dot(normal, view_dir), 0.0);
    var lighting = vec3<f32>(0.0);

    for (var i: u32 = 0u; i < light_meta.count; i = i + 1u) {
        let light = light_table[i];
        if (light.falloff.y > 0.5) {
            let light_dir = safe_normalize(-light.pos_radius.xyz);
            let ndotl = max(dot(normal, light_dir), 0.0);
            if (ndotl <= 0.0001) {
                continue;
            }
            var shadow_term = 1.0;
            if (shadow.shadow_params.w > 0.5 &&
                dot(light_dir, normalize(-shadow.light_direction.xyz)) > 0.999) {
                shadow_term = shadow_visibility(input.world_position);
            }
            let half_dir = safe_normalize(light_dir + view_dir);
            let fresnel = fresnel_schlick(max(dot(half_dir, view_dir), 0.0), f0);
            let distribution = distribution_ggx(normal, half_dir, roughness);
            let geometry = geometry_smith(normal, view_dir, light_dir, roughness);
            let specular = fresnel * (distribution * geometry / max(4.0 * ndotv * ndotl, 0.0001));
            let diffuse = (vec3<f32>(1.0) - fresnel) * (1.0 - metallic) * base.rgb / 3.14159265;
            lighting += (diffuse + specular) * light.color.rgb * ndotl * shadow_term;
            continue;
        }

        let light_vector = light.pos_radius.xyz - input.world_position;
        let dist = length(light_vector);
        let radius = light.pos_radius.w;
        if (radius <= 0.0 || dist >= radius) {
            continue;
        }

        let attenuation = pow(
            max(1.0 - dist / radius, 0.0),
            max(light.falloff.x, 0.001),
        );
        let light_dir = light_vector / max(dist, 0.0001);
        let ndotl = max(dot(normal, light_dir), 0.0);
        if (ndotl <= 0.0001) {
            continue;
        }
        let half_dir = safe_normalize(light_dir + view_dir);
        let fresnel = fresnel_schlick(max(dot(half_dir, view_dir), 0.0), f0);
        let distribution = distribution_ggx(normal, half_dir, roughness);
        let geometry = geometry_smith(normal, view_dir, light_dir, roughness);
        let specular = fresnel * (distribution * geometry / max(4.0 * ndotv * ndotl, 0.0001));
        let diffuse = (vec3<f32>(1.0) - fresnel) * (1.0 - metallic) * base.rgb / 3.14159265;
        lighting += (diffuse + specular) * light.color.rgb * ndotl * attenuation;
    }

    let emissive_sample = textureSample(t_emissive, s_albedo, input.uv).rgb;
    let emissive = emissive_sample * material.emissive.rgb;
    let ambient = base.rgb * mix(0.0015, 0.0065, roughness);
    return vec4<f32>(lighting + ambient + emissive, base.a);
}
