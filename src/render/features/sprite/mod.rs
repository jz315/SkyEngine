//! Sprite rendering and animation family.

pub mod animation;
mod component;
pub mod renderer;

pub use animation::*;
pub use component::{SortingLayer, SpriteRenderer};
pub use renderer::*;

use crate::render::pipeline::{RenderFeature, RenderPipelineBuilder};
use crate::render::resources::material::{StandardMaterial, UnlitMaterial};

/// Registers the built-in sprite extraction, draw, and material paths.
pub struct SpriteFeature;

impl SpriteFeature {
    #[inline]
    pub fn new() -> Self {
        Self::lit_hdr()
    }

    #[inline]
    pub fn lit_hdr() -> Self {
        Self
    }

    #[inline]
    pub fn unlit() -> Self {
        Self
    }
}

impl Default for SpriteFeature {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderFeature for SpriteFeature {
    fn name(&self) -> &'static str {
        "sprite"
    }

    fn register(&mut self, builder: &mut RenderPipelineBuilder) {
        let current = std::mem::take(builder);
        *builder = current
            .register_material::<crate::render::SpriteMaterial>()
            .register_material::<UnlitMaterial>()
            .register_material::<StandardMaterial>();
    }
}
