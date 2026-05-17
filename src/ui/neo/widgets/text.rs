//! Port of `EUI-NEO/components/text.h`.

use crate::render::Color;

pub use super::super::text_measure::measure_text_width;
use super::super::{ElementBuilder, HorizontalAlign, Ui, VerticalAlign};
use super::theme::{self, ThemeColorTokens};

#[derive(Debug, Clone)]
pub struct TextStyle {
    pub text: String,
    pub font_family: String,
    pub font_size: f32,
    pub font_weight: i32,
    pub color: Color,
    pub max_width: f32,
    pub wrap: bool,
    pub horizontal_align: HorizontalAlign,
    pub vertical_align: VerticalAlign,
    pub line_height: f32,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            text: String::new(),
            font_family: String::new(),
            font_size: 16.0,
            font_weight: 400,
            color: Color::WHITE,
            max_width: 0.0,
            wrap: false,
            horizontal_align: HorizontalAlign::Left,
            vertical_align: VerticalAlign::Top,
            line_height: 0.0,
        }
    }
}

pub fn body_text_style(tokens: ThemeColorTokens, value: impl Into<String>) -> TextStyle {
    let visuals = theme::page_visuals(tokens);
    TextStyle {
        text: value.into(),
        color: visuals.body_color,
        font_size: visuals.label_size,
        ..TextStyle::default()
    }
}

pub fn title_text_style(tokens: ThemeColorTokens, value: impl Into<String>) -> TextStyle {
    let visuals = theme::page_visuals(tokens);
    TextStyle {
        text: value.into(),
        color: visuals.title_color,
        font_size: visuals.header_title_size,
        ..TextStyle::default()
    }
}

pub fn subtitle_text_style(tokens: ThemeColorTokens, value: impl Into<String>) -> TextStyle {
    let visuals = theme::page_visuals(tokens);
    TextStyle {
        text: value.into(),
        color: visuals.subtitle_color,
        font_size: visuals.header_subtitle_size,
        ..TextStyle::default()
    }
}

pub fn bodyTextStyle(tokens: ThemeColorTokens, value: impl Into<String>) -> TextStyle {
    body_text_style(tokens, value)
}

pub fn titleTextStyle(tokens: ThemeColorTokens, value: impl Into<String>) -> TextStyle {
    title_text_style(tokens, value)
}

pub fn subtitleTextStyle(tokens: ThemeColorTokens, value: impl Into<String>) -> TextStyle {
    subtitle_text_style(tokens, value)
}

pub fn text<'ui>(ui: &'ui mut Ui, id: impl Into<String>) -> ElementBuilder<'ui> {
    let tokens = theme::dark_theme_colors();
    ui.text(id).color(tokens.text)
}

pub fn text_with_theme<'ui>(
    ui: &'ui mut Ui,
    id: impl Into<String>,
    tokens: ThemeColorTokens,
) -> ElementBuilder<'ui> {
    ui.text(id).color(tokens.text)
}

pub fn textWithTheme<'ui>(
    ui: &'ui mut Ui,
    id: impl Into<String>,
    tokens: ThemeColorTokens,
) -> ElementBuilder<'ui> {
    text_with_theme(ui, id, tokens)
}

pub fn label<'ui>(ui: &'ui mut Ui, id: impl Into<String>) -> ElementBuilder<'ui> {
    let tokens = theme::dark_theme_colors();
    ui.label(id).color(tokens.text)
}

pub fn label_with_theme<'ui>(
    ui: &'ui mut Ui,
    id: impl Into<String>,
    tokens: ThemeColorTokens,
) -> ElementBuilder<'ui> {
    ui.label(id).color(tokens.text)
}

pub fn labelWithTheme<'ui>(
    ui: &'ui mut Ui,
    id: impl Into<String>,
    tokens: ThemeColorTokens,
) -> ElementBuilder<'ui> {
    label_with_theme(ui, id, tokens)
}

pub fn text_with_style<'ui>(
    ui: &'ui mut Ui,
    id: impl Into<String>,
    style: TextStyle,
) -> ElementBuilder<'ui> {
    ui.text(id)
        .text(style.text)
        .font_family(style.font_family)
        .font_size(style.font_size)
        .font_weight(style.font_weight)
        .color(style.color)
        .max_width(style.max_width)
        .wrap(style.wrap)
        .horizontal_align(style.horizontal_align)
        .vertical_align(style.vertical_align)
        .line_height(style.line_height)
}

pub fn textWithStyle<'ui>(
    ui: &'ui mut Ui,
    id: impl Into<String>,
    style: TextStyle,
) -> ElementBuilder<'ui> {
    text_with_style(ui, id, style)
}

pub fn measureTextWidth(value: &str, font_family: &str, font_size: f32, font_weight: i32) -> f32 {
    measure_text_width(value, font_family, font_size, font_weight)
}

#[cfg(test)]
mod tests {
    use super::{measureTextWidth, measure_text_width};

    #[test]
    fn measure_text_width_empty_is_zero() {
        assert_eq!(measure_text_width("", "", 16.0, 400), 0.0);
    }

    #[test]
    fn measure_text_width_clamps_font_size_to_one() {
        let zero = measure_text_width("A", "", 0.0, 400);
        let one = measure_text_width("A", "", 1.0, 400);
        assert!((zero - one).abs() < 0.001);
    }

    #[test]
    fn measure_text_width_sums_spaces_like_eui() {
        let base = measure_text_width("A", "", 16.0, 400);
        let spaced = measure_text_width("A ", "", 16.0, 400);
        assert!(spaced > base);
    }

    #[test]
    fn measure_text_width_tracks_renderer_default_text_scale() {
        let small = measure_text_width("Sky", "", 16.0, 400);
        let large = measure_text_width("Sky", "", 32.0, 400);
        assert!(large > small * 1.9);
        assert!(large < small * 2.1);
    }

    #[test]
    fn eui_source_name_alias_matches_snake_case() {
        assert_eq!(
            measureTextWidth("Sky", "", 16.0, 400),
            measure_text_width("Sky", "", 16.0, 400)
        );
    }
}
