use crate::render::gpu::Texture;
use crate::render::view::Color;

use super::super::{
    MainPassMode, MaterialBinding, MaterialError, MaterialInterface, MaterialModel,
    MaterialPassSet, MaterialPrepareContext, MaterialRenderState, PreparedMaterial,
    SceneResourceRequirements, ShaderVariantKey, ShaderVariantPolicy,
};
use super::common::AlphaMode;

/// Built-in sprite material data.
#[derive(Clone)]
pub struct SpriteMaterial {
    pub position: [f32; 3],
    pub size: [f32; 2],
    pub rotation: f32,
    pub color: Color,
    pub texture: Option<Texture>,
    pub uv_rect: [f32; 4],
    pub alpha_mode: AlphaMode,
}

impl SpriteMaterial {
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
    pub fn position(mut self, x: f32, y: f32, z: f32) -> Self {
        self.position = [x, y, z];
        self
    }

    #[inline]
    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.size = [width, height];
        self
    }

    #[inline]
    pub fn rotation(mut self, radians: f32) -> Self {
        self.rotation = radians;
        self
    }

    #[inline]
    pub fn texture(mut self, texture: Texture) -> Self {
        self.texture = Some(texture);
        self
    }

    #[inline]
    pub fn clear_texture(mut self) -> Self {
        self.texture = None;
        self
    }

    #[inline]
    pub fn uv(mut self, u_min: f32, v_min: f32, u_max: f32, v_max: f32) -> Self {
        self.uv_rect = [u_min, v_min, u_max, v_max];
        self
    }
}

impl Default for SpriteMaterial {
    fn default() -> Self {
        Self {
            position: [0.0, 0.0, 0.0],
            size: [1.0, 1.0],
            rotation: 0.0,
            color: Color::WHITE,
            texture: None,
            uv_rect: [0.0, 0.0, 1.0, 1.0],
            alpha_mode: AlphaMode::Blend,
        }
    }
}

impl std::fmt::Debug for SpriteMaterial {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SpriteMaterial")
            .field("position", &self.position)
            .field("size", &self.size)
            .field("rotation", &self.rotation)
            .field("color", &self.color.to_array())
            .field("textured", &self.texture.is_some())
            .field("uv_rect", &self.uv_rect)
            .finish()
    }
}

impl MaterialModel for SpriteMaterial {
    type Data = SpriteMaterial;

    fn interface() -> MaterialInterface {
        MaterialInterface::builder("sprite")
            .shader(super::super::MaterialShaderSet::wgsl(include_str!(
                "../../../shaders/sprite/sprite_material.wgsl"
            )))
            .vertex(crate::render::resources::mesh::Mesh::vertex_layout_position_uv())
            .scene(SceneResourceRequirements::new().camera())
            .binding(MaterialBinding::uniform(
                0,
                std::num::NonZeroU64::new(std::mem::size_of::<SpriteUniform>() as u64)
                    .expect("sprite uniform is non-zero"),
            ))
            .binding(MaterialBinding::texture_2d(1))
            .binding(MaterialBinding::sampler(2))
            .passes(MaterialPassSet {
                main: MainPassMode::Transparent,
                prepass: None,
                shadow: super::super::ShadowPassMode::None,
            })
            .render_state(MaterialRenderState::transparent())
            .variants(ShaderVariantPolicy::new(["alpha_mode"]))
            .build()
    }

    fn variant(
        data: &Self::Data,
        _ctx: &super::super::MaterialVariantContext<'_>,
    ) -> ShaderVariantKey {
        ShaderVariantKey::new().with("alpha_mode", data.alpha_mode as u64)
    }

    fn render_state(data: &Self::Data) -> MaterialRenderState {
        data.alpha_mode.render_state()
    }

    fn passes(data: &Self::Data) -> MaterialPassSet {
        MaterialPassSet {
            main: data.alpha_mode.main_pass(),
            prepass: None,
            shadow: super::super::ShadowPassMode::None,
        }
    }

    fn prepare(
        data: &Self::Data,
        ctx: &mut MaterialPrepareContext<'_>,
    ) -> Result<PreparedMaterial, MaterialError> {
        let texture = ctx.texture_or_fallback(data.texture.as_ref());
        ctx.bindings()
            .uniform(
                0,
                "sprite_material_uniform",
                &SpriteUniform {
                    position_size: [
                        data.position[0],
                        data.position[1],
                        data.size[0],
                        data.size[1],
                    ],
                    rotation_z: [data.rotation, data.position[2], 0.0, 0.0],
                    color: data.color.to_array(),
                    uv_rect: data.uv_rect,
                },
            )
            .texture(1, texture)
            .sampler(2, ctx.sampler_nearest())
            .build()
    }
}

pub struct SpriteMaterialModel;

impl MaterialModel for SpriteMaterialModel {
    type Data = SpriteMaterial;

    fn interface() -> MaterialInterface {
        <SpriteMaterial as MaterialModel>::interface()
    }

    fn variant(
        data: &Self::Data,
        ctx: &super::super::MaterialVariantContext<'_>,
    ) -> ShaderVariantKey {
        <SpriteMaterial as MaterialModel>::variant(data, ctx)
    }

    fn render_state(data: &Self::Data) -> MaterialRenderState {
        <SpriteMaterial as MaterialModel>::render_state(data)
    }

    fn passes(data: &Self::Data) -> MaterialPassSet {
        <SpriteMaterial as MaterialModel>::passes(data)
    }

    fn prepare(
        data: &Self::Data,
        ctx: &mut MaterialPrepareContext<'_>,
    ) -> Result<PreparedMaterial, MaterialError> {
        <SpriteMaterial as MaterialModel>::prepare(data, ctx)
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct SpriteUniform {
    position_size: [f32; 4],
    rotation_z: [f32; 4],
    color: [f32; 4],
    uv_rect: [f32; 4],
}
