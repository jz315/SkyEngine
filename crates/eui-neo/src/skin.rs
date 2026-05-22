use rustc_hash::FxHashMap;

use crate::{Color, EdgeInsets, FontRef, ImageRef, Insets, Slice};

#[derive(Debug, Clone)]
pub struct ButtonSkin {
    pub normal: ImageRef,
    pub hover: Option<ImageRef>,
    pub pressed: Option<ImageRef>,
    pub disabled: Option<ImageRef>,
    pub font: FontRef,
    pub text_color: Color,
    pub disabled_text_color: Color,
    pub tint: Color,
    pub hover_tint: Color,
    pub pressed_tint: Color,
    pub disabled_tint: Color,
    pub slice: Slice,
    pub content_inset: Insets,
    pub press_scale: f32,
}

impl Default for ButtonSkin {
    fn default() -> Self {
        Self {
            normal: ImageRef::default(),
            hover: None,
            pressed: None,
            disabled: None,
            font: FontRef::DefaultText,
            text_color: Color::WHITE,
            disabled_text_color: Color::rgba(0.72, 0.76, 0.80, 1.0),
            tint: Color::WHITE,
            hover_tint: Color::rgba(1.0, 1.0, 0.90, 1.0),
            pressed_tint: Color::rgba(0.78, 0.88, 0.96, 1.0),
            disabled_tint: Color::rgba(0.58, 0.62, 0.68, 0.82),
            slice: Slice::ZERO,
            content_inset: EdgeInsets::symmetric(24.0, 10.0),
            press_scale: 0.96,
        }
    }
}

impl ButtonSkin {
    pub fn image_for(&self, response: crate::Response, disabled: bool) -> ImageRef {
        if disabled {
            return self.disabled.clone().unwrap_or_else(|| self.normal.clone());
        }
        if response.pressed {
            return self
                .pressed
                .clone()
                .or_else(|| self.hover.clone())
                .unwrap_or_else(|| self.normal.clone());
        }
        if response.hovered {
            return self.hover.clone().unwrap_or_else(|| self.normal.clone());
        }
        self.normal.clone()
    }
}

#[derive(Debug, Clone)]
pub struct PanelSkin {
    pub image: ImageRef,
    pub tint: Color,
    pub slice: Slice,
    pub content_inset: Insets,
}

impl Default for PanelSkin {
    fn default() -> Self {
        Self {
            image: ImageRef::default(),
            tint: Color::WHITE,
            slice: Slice::ZERO,
            content_inset: EdgeInsets::all(16.0),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CheckboxSkin {
    pub checked: ImageRef,
    pub unchecked: ImageRef,
    pub font: FontRef,
    pub text_color: Color,
    pub box_size: f32,
    pub gap: f32,
    pub hover_tint: Color,
    pub pressed_tint: Color,
}

impl Default for CheckboxSkin {
    fn default() -> Self {
        Self {
            checked: ImageRef::default(),
            unchecked: ImageRef::default(),
            font: FontRef::DefaultText,
            text_color: Color::WHITE,
            box_size: 42.0,
            gap: 16.0,
            hover_tint: Color::rgba(1.0, 1.0, 0.90, 1.0),
            pressed_tint: Color::rgba(0.82, 0.92, 1.0, 1.0),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SliderSkin {
    pub track: ImageRef,
    pub track_tint: Color,
    pub track_height: f32,
    pub track_opacity: f32,
    pub fill: Color,
    pub knob: Color,
    pub rail: Color,
}

impl Default for SliderSkin {
    fn default() -> Self {
        Self {
            track: ImageRef::default(),
            track_tint: Color::WHITE,
            track_height: 11.0,
            track_opacity: 0.65,
            fill: Color::rgba(0.32, 0.67, 0.96, 1.0),
            knob: Color::WHITE,
            rail: Color::rgba(0.14, 0.20, 0.25, 1.0),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct NeoSkin {
    name: String,
    images: FxHashMap<String, ImageRef>,
    fonts: FxHashMap<String, FontRef>,
    buttons: FxHashMap<String, ButtonSkin>,
    panels: FxHashMap<String, PanelSkin>,
    checkboxes: FxHashMap<String, CheckboxSkin>,
    sliders: FxHashMap<String, SliderSkin>,
}

impl NeoSkin {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Self::default()
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn image(mut self, key: impl Into<String>, image: impl Into<ImageRef>) -> Self {
        self.images.insert(key.into(), image.into());
        self
    }

    pub fn font(mut self, key: impl Into<String>, font: impl Into<FontRef>) -> Self {
        self.fonts.insert(key.into(), font.into());
        self
    }

    pub fn button(mut self, key: impl Into<String>, skin: ButtonSkin) -> Self {
        self.buttons.insert(key.into(), skin);
        self
    }

    pub fn panel(mut self, key: impl Into<String>, skin: PanelSkin) -> Self {
        self.panels.insert(key.into(), skin);
        self
    }

    pub fn checkbox(mut self, key: impl Into<String>, skin: CheckboxSkin) -> Self {
        self.checkboxes.insert(key.into(), skin);
        self
    }

    pub fn slider(mut self, key: impl Into<String>, skin: SliderSkin) -> Self {
        self.sliders.insert(key.into(), skin);
        self
    }
}

#[derive(Debug, Clone, Default)]
pub struct SkinRegistry {
    images: FxHashMap<String, ImageRef>,
    fonts: FxHashMap<String, FontRef>,
    buttons: FxHashMap<String, ButtonSkin>,
    panels: FxHashMap<String, PanelSkin>,
    checkboxes: FxHashMap<String, CheckboxSkin>,
    sliders: FxHashMap<String, SliderSkin>,
}

impl SkinRegistry {
    pub fn register(&mut self, skin: NeoSkin) {
        let prefix = skin.name;
        for (key, value) in skin.images {
            self.images.insert(namespaced(&prefix, &key), value);
        }
        for (key, value) in skin.fonts {
            self.fonts.insert(namespaced(&prefix, &key), value);
        }
        for (key, value) in skin.buttons {
            self.buttons.insert(namespaced(&prefix, &key), value);
        }
        for (key, value) in skin.panels {
            self.panels.insert(namespaced(&prefix, &key), value);
        }
        for (key, value) in skin.checkboxes {
            self.checkboxes.insert(namespaced(&prefix, &key), value);
        }
        for (key, value) in skin.sliders {
            self.sliders.insert(namespaced(&prefix, &key), value);
        }
    }

    pub fn insert_image(&mut self, key: impl Into<String>, value: impl Into<ImageRef>) {
        self.images.insert(key.into(), value.into());
    }

    pub fn insert_font(&mut self, key: impl Into<String>, value: impl Into<FontRef>) {
        self.fonts.insert(key.into(), value.into());
    }

    pub fn insert_button(&mut self, key: impl Into<String>, value: ButtonSkin) {
        self.buttons.insert(key.into(), value);
    }

    pub fn insert_panel(&mut self, key: impl Into<String>, value: PanelSkin) {
        self.panels.insert(key.into(), value);
    }

    pub fn insert_checkbox(&mut self, key: impl Into<String>, value: CheckboxSkin) {
        self.checkboxes.insert(key.into(), value);
    }

    pub fn insert_slider(&mut self, key: impl Into<String>, value: SliderSkin) {
        self.sliders.insert(key.into(), value);
    }

    pub fn image(&self, key: &str) -> Option<&ImageRef> {
        self.images.get(key)
    }

    pub fn font(&self, key: &str) -> Option<&FontRef> {
        self.fonts.get(key)
    }

    pub fn button(&self, key: &str) -> Option<&ButtonSkin> {
        self.buttons.get(key)
    }

    pub fn panel(&self, key: &str) -> Option<&PanelSkin> {
        self.panels.get(key)
    }

    pub fn checkbox(&self, key: &str) -> Option<&CheckboxSkin> {
        self.checkboxes.get(key)
    }

    pub fn slider(&self, key: &str) -> Option<&SliderSkin> {
        self.sliders.get(key)
    }

    pub fn resolve_image(&self, value: &ImageRef) -> ImageRef {
        if let Some(key) = value.is_key().then(|| value.source()) {
            if let Some(resolved) = self.image(key) {
                return resolved
                    .clone()
                    .with_flip_vertically(resolved.flip_vertically() || value.flip_vertically());
            }
        }
        value.clone()
    }

    pub fn resolve_font(&self, value: &FontRef) -> FontRef {
        if let Some(key) = value.as_key() {
            if let Some(resolved) = self.font(key) {
                return resolved.clone();
            }
        }
        value.clone()
    }
}

fn namespaced(prefix: &str, key: &str) -> String {
    if prefix.is_empty() {
        key.to_string()
    } else if key.is_empty() {
        prefix.to_string()
    } else {
        format!("{prefix}.{key}")
    }
}
