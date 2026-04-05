//! SkyEngine modern 2D rendering facade.

pub(crate) mod core;
mod ecs;
pub mod expert;
pub(crate) mod graph;
pub(crate) mod light;
pub(crate) mod passes;
pub(crate) mod postfx;
mod renderer2d;
pub(crate) mod resources;
mod scene2d;

#[cfg(feature = "live2d")]
pub(crate) mod live2d;

pub use core::{camera::Camera2D, color::Color, texture::Texture};
pub use ecs::{
    BloomSettings, PointLight2D, PrimaryCamera2D, RenderSettings2D, Sprite2D, ToneMapSettings,
    Transform2D, VignetteSettings,
};
pub use passes::batch::Sprite;
pub use renderer2d::{Renderer2D, Renderer2DConfig, RendererStats};
pub use scene2d::Scene2D;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curated_render_exports_are_available() {
        let _camera = Camera2D::new(16.0, 9.0);
        let _color = Color::WHITE;
        let _settings = RenderSettings2D::default();
        let _scene = Scene2D::new();
        let _sprite = Sprite::new(0.0, 0.0, 1.0, 1.0);
        let _renderer_config = Renderer2DConfig::unlit();
        let _renderer_stats = RendererStats::default();
        let _transform = Transform2D::default();
        let _sprite2d = Sprite2D::new(8.0, 8.0);
        let _light = PointLight2D::new(64.0);
        let _primary_camera = PrimaryCamera2D;
    }

    #[test]
    fn expert_namespace_exposes_low_level_render_api() {
        let _graph = expert::RenderGraph::new();
        let _target: Option<expert::RenderTarget> = None;
        let _batch: Option<expert::SpriteBatch> = None;
        let _light: Option<expert::LightPass> = None;
        let _composite: Option<expert::CompositePass> = None;
        let _bloom: Option<expert::Bloom> = None;
        let _tonemap: Option<expert::ToneMap> = None;
        let _vignette: Option<expert::Vignette> = None;
    }
}
