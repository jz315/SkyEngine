use std::num::NonZeroU64;

use crate::render::resources::mesh::Mesh;

use super::super::{
    MainPassMode, MaterialBinding, MaterialInterfaceBuilder, MaterialPassSet, MaterialRenderState,
    SceneResourceRequirements, ShadowPassMode,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AlphaMode {
    #[default]
    Opaque,
    Mask,
    Blend,
    Additive,
}

impl AlphaMode {
    #[inline]
    pub const fn is_alpha_test(self) -> bool {
        matches!(self, Self::Mask)
    }

    #[inline]
    pub const fn is_transparent(self) -> bool {
        matches!(self, Self::Blend | Self::Additive)
    }

    #[inline]
    pub const fn main_pass(self) -> MainPassMode {
        match self {
            Self::Opaque => MainPassMode::Opaque,
            Self::Mask => MainPassMode::AlphaMask,
            Self::Blend => MainPassMode::Transparent,
            Self::Additive => MainPassMode::Additive,
        }
    }

    #[inline]
    pub const fn render_state(self) -> MaterialRenderState {
        match self {
            Self::Opaque | Self::Mask => MaterialRenderState::opaque(),
            Self::Blend => MaterialRenderState::transparent(),
            Self::Additive => MaterialRenderState::additive(),
        }
    }
}

pub(super) fn textured_uniform_builder(
    name: &'static str,
    shader: &'static str,
    uniform_size: u64,
) -> MaterialInterfaceBuilder {
    MaterialInterfaceBuilder::new(name)
        .shader(super::super::MaterialShaderSet::wgsl(shader))
        .vertex(Mesh::vertex_layout_position_uv())
        .scene(SceneResourceRequirements::new().camera().model())
        .binding(MaterialBinding::uniform(
            0,
            NonZeroU64::new(uniform_size).expect("uniform size is non-zero"),
        ))
        .binding(MaterialBinding::texture_2d(1))
        .binding(MaterialBinding::sampler(2))
}

pub(super) const fn passes_for_alpha(
    alpha_mode: AlphaMode,
    shadows: ShadowPassMode,
) -> MaterialPassSet {
    MaterialPassSet {
        main: alpha_mode.main_pass(),
        prepass: None,
        shadow: shadows,
    }
}
