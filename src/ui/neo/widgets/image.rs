//! Port of `EUI-NEO/components/image.h`.

use crate::render::Color;

use super::super::{ElementBuilder, Ui};
use super::theme::{self, ThemeColorTokens};

#[derive(Debug, Clone, Copy)]
pub struct ImageStyle {
    pub tint: Color,
    pub radius: f32,
    pub opacity: f32,
}

impl ImageStyle {
    pub fn new(tokens: ThemeColorTokens) -> Self {
        Self {
            tint: theme::color(1.0, 1.0, 1.0, 1.0),
            radius: theme::page_visuals(tokens).section_rounding,
            opacity: 1.0,
        }
    }
}

impl Default for ImageStyle {
    fn default() -> Self {
        Self::new(theme::dark_theme_colors())
    }
}

pub fn image<'ui>(ui: &'ui mut Ui, id: impl Into<String>) -> ElementBuilder<'ui> {
    image_with_style(ui, id, ImageStyle::default())
}

pub fn image_with_theme<'ui>(
    ui: &'ui mut Ui,
    id: impl Into<String>,
    tokens: ThemeColorTokens,
) -> ElementBuilder<'ui> {
    image_with_style(ui, id, ImageStyle::new(tokens))
}

pub fn imageWithTheme<'ui>(
    ui: &'ui mut Ui,
    id: impl Into<String>,
    tokens: ThemeColorTokens,
) -> ElementBuilder<'ui> {
    image_with_theme(ui, id, tokens)
}

pub fn image_with_style<'ui>(
    ui: &'ui mut Ui,
    id: impl Into<String>,
    style: ImageStyle,
) -> ElementBuilder<'ui> {
    ui.image(id)
        .tint(style.tint)
        .radius(style.radius)
        .opacity(style.opacity)
}

pub fn imageWithStyle<'ui>(
    ui: &'ui mut Ui,
    id: impl Into<String>,
    style: ImageStyle,
) -> ElementBuilder<'ui> {
    image_with_style(ui, id, style)
}
