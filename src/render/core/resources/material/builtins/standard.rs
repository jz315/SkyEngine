use crate::render::gpu::Texture;
use crate::render::resources::mesh::Mesh;
use crate::render::Color;

use super::super::{
    MainPassMode, MaterialBinding, MaterialError, MaterialInterface, MaterialModel,
    MaterialPassSet, MaterialPrepareContext, MaterialPrepassMode, MaterialRenderState,
    PreparedMaterial, SceneResourceRequirements, ShaderVariantKey, ShaderVariantPolicy,
    ShadowPassMode,
};
use super::common::AlphaMode;

/// Default PBR material data.
#[derive(Clone)]
pub struct StandardMaterial {
    pub albedo: Color,
    pub albedo_texture: Option<Texture>,
    pub metallic: f32,
    pub roughness: f32,
    pub normal_texture: Option<Texture>,
    pub emissive: Color,
    pub emissive_texture: Option<Texture>,
    pub alpha_mode: AlphaMode,
    pub alpha_cutoff: f32,
    pub receive_shadows: bool,
}

impl StandardMaterial {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    pub fn receive_shadows(mut self, receive_shadows: bool) -> Self {
        self.receive_shadows = receive_shadows;
        self
    }

    #[inline]
    pub fn alpha_mode(mut self, alpha_mode: AlphaMode) -> Self {
        self.alpha_mode = alpha_mode;
        self
    }

    #[inline]
    pub fn alpha_cutoff(mut self, alpha_cutoff: f32) -> Self {
        self.alpha_cutoff = alpha_cutoff;
        self
    }

    #[inline]
    pub fn alpha_mask(mut self, alpha_cutoff: f32) -> Self {
        self.alpha_mode = AlphaMode::Mask;
        self.alpha_cutoff = alpha_cutoff;
        self
    }

    #[inline]
    pub(crate) fn casts_alpha_test_shadow(&self) -> bool {
        self.alpha_mode.is_alpha_test()
    }
}

impl Default for StandardMaterial {
    fn default() -> Self {
        Self {
            albedo: Color::WHITE,
            albedo_texture: None,
            metallic: 0.0,
            roughness: 0.8,
            normal_texture: None,
            emissive: Color::BLACK,
            emissive_texture: None,
            alpha_mode: AlphaMode::Opaque,
            alpha_cutoff: 0.5,
            receive_shadows: true,
        }
    }
}

impl std::fmt::Debug for StandardMaterial {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StandardMaterial")
            .field("albedo", &self.albedo.to_array())
            .field("textured", &self.albedo_texture.is_some())
            .field("metallic", &self.metallic)
            .field("roughness", &self.roughness)
            .field("emissive", &self.emissive.to_array())
            .field("alpha_mode", &self.alpha_mode)
            .field("alpha_cutoff", &self.alpha_cutoff)
            .field("receive_shadows", &self.receive_shadows)
            .finish()
    }
}

impl MaterialModel for StandardMaterial {
    type Data = StandardMaterial;

    fn interface() -> MaterialInterface {
        MaterialInterface::builder("standard")
            .shader(super::super::MaterialShaderSet::wgsl(include_str!(
                "../../../../shaders/materials/standard_material.wgsl"
            )))
            .vertex(Mesh::vertex_layout_position_normal_uv())
            .scene(
                SceneResourceRequirements::new()
                    .camera()
                    .model()
                    .lighting()
                    .shadows_optional()
                    .gi_optional(),
            )
            .binding(MaterialBinding::uniform(
                0,
                std::num::NonZeroU64::new(std::mem::size_of::<StandardUniform>() as u64)
                    .expect("standard uniform is non-zero"),
            ))
            .binding(MaterialBinding::texture_2d(1))
            .binding(MaterialBinding::sampler(2))
            .binding(MaterialBinding::texture_2d(3))
            .binding(MaterialBinding::texture_2d(4))
            .passes(MaterialPassSet {
                main: MainPassMode::Opaque,
                prepass: Some(MaterialPrepassMode::SceneMaterial),
                shadow: ShadowPassMode::Opaque,
            })
            .render_state(MaterialRenderState::opaque())
            .variants(ShaderVariantPolicy::new([
                "alpha_mode",
                "normal_map",
                "receive_shadows",
            ]))
            .build()
    }

    fn variant(
        data: &Self::Data,
        _ctx: &super::super::MaterialVariantContext<'_>,
    ) -> ShaderVariantKey {
        ShaderVariantKey::new()
            .with("alpha_mode", data.alpha_mode as u64)
            .with("normal_map", data.normal_texture.is_some() as u64)
            .with("receive_shadows", data.receive_shadows as u64)
    }

    fn shader_source(data: &Self::Data) -> super::super::ShaderSource {
        super::super::ShaderSource::wgsl(if data.normal_texture.is_some() {
            include_str!("../../../../shaders/materials/standard_material_normal_mapped.wgsl")
        } else {
            include_str!("../../../../shaders/materials/standard_material.wgsl")
        })
    }

    fn vertex_layout(data: &Self::Data) -> crate::render::resources::mesh::VertexLayout {
        if data.normal_texture.is_some() {
            Mesh::vertex_layout_position_normal_tangent_uv()
        } else {
            Mesh::vertex_layout_position_normal_uv()
        }
    }

    fn render_state(data: &Self::Data) -> MaterialRenderState {
        data.alpha_mode.render_state()
    }

    fn passes(data: &Self::Data) -> MaterialPassSet {
        MaterialPassSet {
            main: data.alpha_mode.main_pass(),
            prepass: Some(MaterialPrepassMode::SceneMaterial),
            shadow: if data.alpha_mode.is_alpha_test() {
                ShadowPassMode::AlphaTest
            } else if data.alpha_mode.is_transparent() {
                ShadowPassMode::Transparent
            } else {
                ShadowPassMode::Opaque
            },
        }
    }

    fn prepare(
        data: &Self::Data,
        ctx: &mut MaterialPrepareContext<'_>,
    ) -> Result<PreparedMaterial, MaterialError> {
        let albedo_texture = ctx.texture_or_fallback(data.albedo_texture.as_ref());
        let emissive_texture = ctx.texture_or_fallback(data.emissive_texture.as_ref());
        let normal_texture = ctx.texture_or_fallback(data.normal_texture.as_ref());
        ctx.bindings()
            .uniform(
                0,
                "standard_material_uniform",
                &StandardUniform {
                    albedo: data.albedo.to_array(),
                    emissive: data.emissive.to_array(),
                    params: [
                        data.metallic,
                        data.roughness,
                        data.normal_texture.is_some() as u32 as f32,
                        data.emissive_texture.is_some() as u32 as f32,
                    ],
                    shadow: [
                        data.receive_shadows as u32 as f32,
                        data.alpha_cutoff,
                        data.alpha_mode.is_alpha_test() as u32 as f32,
                        0.0,
                    ],
                },
            )
            .texture(1, albedo_texture)
            .sampler(2, ctx.sampler_linear())
            .texture(3, emissive_texture)
            .texture(4, normal_texture)
            .build()
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct StandardUniform {
    albedo: [f32; 4],
    emissive: [f32; 4],
    params: [f32; 4],
    shadow: [f32; 4],
}
