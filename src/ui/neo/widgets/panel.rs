//! Port of `EUI-NEO/components/panel.h`.

use crate::render::Color;

use super::super::{Border, ElementBuilder, Gradient, Shadow, Ui};
use super::theme::{self, ThemeColorTokens};

#[derive(Debug, Clone, Copy)]
pub struct PanelStyle {
    pub color: Color,
    pub gradient: Gradient,
    pub border: Border,
    pub shadow: Shadow,
    pub radius: f32,
    pub opacity: f32,
}

impl PanelStyle {
    pub fn new(tokens: ThemeColorTokens) -> Self {
        Self {
            color: tokens.surface,
            gradient: Gradient::default(),
            border: theme::border(tokens, 1.0, 1.0),
            shadow: theme::panel_shadow(tokens),
            radius: theme::page_visuals(tokens).section_rounding,
            opacity: 1.0,
        }
    }
}

impl Default for PanelStyle {
    fn default() -> Self {
        Self::new(theme::dark_theme_colors())
    }
}

pub fn panel<'ui>(ui: &'ui mut Ui, id: impl Into<String>) -> ElementBuilder<'ui> {
    panel_with_style(ui, id, PanelStyle::default())
}

pub fn panel_with_theme<'ui>(
    ui: &'ui mut Ui,
    id: impl Into<String>,
    tokens: ThemeColorTokens,
) -> ElementBuilder<'ui> {
    panel_with_style(ui, id, PanelStyle::new(tokens))
}

pub fn panelWithTheme<'ui>(
    ui: &'ui mut Ui,
    id: impl Into<String>,
    tokens: ThemeColorTokens,
) -> ElementBuilder<'ui> {
    panel_with_theme(ui, id, tokens)
}

pub fn panel_with_style<'ui>(
    ui: &'ui mut Ui,
    id: impl Into<String>,
    style: PanelStyle,
) -> ElementBuilder<'ui> {
    ui.rect(id)
        .color(style.color)
        .gradient_style(style.gradient)
        .border_style(style.border)
        .shadow_style(style.shadow)
        .radius(style.radius)
        .opacity(style.opacity)
}

pub fn panelWithStyle<'ui>(
    ui: &'ui mut Ui,
    id: impl Into<String>,
    style: PanelStyle,
) -> ElementBuilder<'ui> {
    panel_with_style(ui, id, style)
}
