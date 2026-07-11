//! Generic shader and binding contract for optional indirect lighting.

#[derive(Clone)]
pub struct IndirectLightingSampling {
    pub layout: wgpu::BindGroupLayout,
    pub bind_group: wgpu::BindGroup,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct IndirectLightingShader {
    pub key: &'static str,
    pub source: &'static str,
}

#[derive(Clone)]
pub(crate) struct FrameIndirectLighting {
    sampling: IndirectLightingSampling,
    shader: IndirectLightingShader,
}

impl FrameIndirectLighting {
    #[inline]
    pub(crate) fn new(sampling: IndirectLightingSampling, shader: IndirectLightingShader) -> Self {
        Self { sampling, shader }
    }

    #[inline]
    pub(crate) fn sampling(&self) -> &IndirectLightingSampling {
        &self.sampling
    }

    #[inline]
    pub(crate) fn shader(&self) -> &IndirectLightingShader {
        &self.shader
    }
}

pub const NULL_INDIRECT_LIGHTING_SHADER: &str = r#"
fn gi_debug_mode() -> u32 {
    return 0u;
}

fn gi_debug_color(world_position: vec3<f32>, normal: vec3<f32>) -> vec3<f32> {
    _ = world_position;
    _ = normal;
    return vec3<f32>(0.0);
}

fn gi_sample_indirect_diffuse(
    world_position: vec3<f32>,
    normal: vec3<f32>,
    base_color: vec3<f32>,
    metallic: f32,
    roughness: f32,
) -> vec3<f32> {
    _ = world_position;
    _ = normal;
    return base_color * (1.0 - metallic) * mix(0.0015, 0.0065, roughness);
}
"#;
