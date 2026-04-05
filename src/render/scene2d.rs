//! Manual scene description API for the high-level 2D renderer.

use crate::render::{Camera2D, PointLight2D, RenderSettings2D, Sprite2D, Transform2D};

#[derive(Clone)]
pub(crate) struct SceneSpriteItem {
    pub(crate) transform: Transform2D,
    pub(crate) sprite: Sprite2D,
    pub(crate) sort_key: u64,
}

#[derive(Clone, Copy)]
pub(crate) struct SceneLightItem {
    pub(crate) transform: Transform2D,
    pub(crate) light: PointLight2D,
}

/// Reusable frame scene description for [`crate::render::Renderer2D`].
pub struct Scene2D {
    pub(crate) camera: Option<Camera2D>,
    pub(crate) settings: RenderSettings2D,
    pub(crate) sprites: Vec<SceneSpriteItem>,
    pub(crate) lights: Vec<SceneLightItem>,
}

impl Scene2D {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    pub fn reset(&mut self) {
        self.camera = None;
        self.settings = RenderSettings2D::default();
        self.sprites.clear();
        self.lights.clear();
    }

    #[inline]
    pub fn set_camera(&mut self, camera: Camera2D) -> &mut Self {
        self.camera = Some(camera);
        self
    }

    #[inline]
    pub fn camera(&self) -> Option<&Camera2D> {
        self.camera.as_ref()
    }

    #[inline]
    pub fn set_settings(&mut self, settings: RenderSettings2D) -> &mut Self {
        self.settings = settings;
        self
    }

    #[inline]
    pub fn settings(&self) -> &RenderSettings2D {
        &self.settings
    }

    #[inline]
    pub fn settings_mut(&mut self) -> &mut RenderSettings2D {
        &mut self.settings
    }

    #[inline]
    pub fn add_sprite(&mut self, transform: Transform2D, sprite: Sprite2D) -> &mut Self {
        let sort_key = self.sprites.len() as u64;
        self.sprites.push(SceneSpriteItem {
            transform,
            sprite,
            sort_key,
        });
        self
    }

    #[inline]
    pub fn add_light(&mut self, transform: Transform2D, light: PointLight2D) -> &mut Self {
        self.lights.push(SceneLightItem { transform, light });
        self
    }

    #[inline]
    pub fn sprite_count(&self) -> usize {
        self.sprites.len()
    }

    #[inline]
    pub fn light_count(&self) -> usize {
        self.lights.len()
    }
}

impl Default for Scene2D {
    fn default() -> Self {
        Self {
            camera: None,
            settings: RenderSettings2D::default(),
            sprites: Vec::with_capacity(256),
            lights: Vec::with_capacity(64),
        }
    }
}
