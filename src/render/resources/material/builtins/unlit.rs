use crate::render::gpu::Texture;
use crate::render::Color;

use super::super::{
    MaterialError, MaterialModel, MaterialPrepareContext, MaterialRenderState,
    MaterialVariantContext, PreparedMaterial, SceneResourceRequirements, ShaderVariantKey,
};
use super::common::{passes_for_alpha, textured_uniform_builder, AlphaMode};

/// Unlit mesh material data.
#[derive(Clone)]
pub struct UnlitMaterial {
    pub color: Color,
    pub texture: Option<Texture>,
    pub alpha_mode: AlphaMode,
}

impl UnlitMaterial {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    #[inline]
    pub fn texture(mut self, texture: Texture) -> Self {
        self.texture = Some(texture);
        self
    }

    #[inline]
    pub fn alpha_mode(mut self, alpha_mode: AlphaMode) -> Self {
        self.alpha_mode = alpha_mode;
        self
    }
}

impl Default for UnlitMaterial {
    fn default() -> Self {
        Self {
            color: Color::WHITE,
            texture: None,
            alpha_mode: AlphaMode::Opaque,
        }
    }
}

impl std::fmt::Debug for UnlitMaterial {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UnlitMaterial")
            .field("color", &self.color.to_array())
            .field("textured", &self.texture.is_some())
            .field("alpha_mode", &self.alpha_mode)
            .finish()
    }
}

impl MaterialModel for UnlitMaterial {
    type Data = UnlitMaterial;

    fn interface() -> super::super::MaterialInterface {
        textured_uniform_builder(
            "unlit",
            include_str!("../../../shaders/materials/unlit_material.wgsl"),
            std::mem::size_of::<UnlitUniform>() as u64,
        )
        .scene(SceneResourceRequirements::new().camera().model())
        .render_state(MaterialRenderState::opaque())
        .passes(passes_for_alpha(
            AlphaMode::Opaque,
            super::super::ShadowPassMode::None,
        ))
        .variants(super::super::ShaderVariantPolicy::new(["alpha_mode"]))
        .build()
    }

    fn variant(data: &Self::Data, _ctx: &MaterialVariantContext<'_>) -> ShaderVariantKey {
        ShaderVariantKey::new().with("alpha_mode", data.alpha_mode as u64)
    }

    fn render_state(data: &Self::Data) -> MaterialRenderState {
        if data.color.a < 0.999 {
            MaterialRenderState::transparent()
        } else {
            data.alpha_mode.render_state()
        }
    }

    fn passes(data: &Self::Data) -> super::super::MaterialPassSet {
        passes_for_alpha(data.alpha_mode, super::super::ShadowPassMode::None)
    }

    fn prepare(
        data: &Self::Data,
        ctx: &mut MaterialPrepareContext<'_>,
    ) -> Result<PreparedMaterial, MaterialError> {
        let texture = ctx.texture_or_fallback(data.texture.as_ref());
        ctx.bindings()
            .uniform(
                0,
                "unlit_material_uniform",
                &UnlitUniform {
                    color: data.color.to_array(),
                },
            )
            .texture(1, texture)
            .sampler(2, ctx.sampler_linear())
            .build()
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct UnlitUniform {
    color: [f32; 4],
}
