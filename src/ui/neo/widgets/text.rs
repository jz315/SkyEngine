//! Port of `EUI-NEO/components/text.h`.

use crate::render::Color;

use std::cell::RefCell;

use glyphon::{Attrs, Buffer, FontSystem, Metrics, Shaping, Weight, Wrap};
use rustc_hash::FxHashMap;

use super::super::fonts::{
    is_icon_family, load_default_eui_fonts, resolve_family, resolved_font_weight,
    DEFAULT_TEXT_FONT_PIXEL_HEIGHT_SCALE,
};
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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct MeasureKey {
    value: String,
    font_family: String,
    font_size_bits: u32,
    font_weight: i32,
}

thread_local! {
    static TEXT_MEASURE_CONTEXT: RefCell<TextMeasureContext> = RefCell::new(TextMeasureContext::new());
    static TEXT_MEASURE_CACHE: RefCell<FxHashMap<MeasureKey, f32>> = RefCell::new(FxHashMap::default());
}

struct TextMeasureContext {
    font_system: FontSystem,
    default_text_family: Option<String>,
    default_icon_family: Option<String>,
}

impl TextMeasureContext {
    fn new() -> Self {
        let mut font_system = FontSystem::new();
        let loaded = load_default_eui_fonts(&mut font_system);
        if loaded.text {
            font_system.db_mut().set_sans_serif_family("JinNanJunJunTi");
            font_system.db_mut().set_serif_family("JinNanJunJunTi");
        }
        Self {
            font_system,
            default_text_family: loaded.text.then(|| "JinNanJunJunTi".to_string()),
            default_icon_family: loaded.icon.then(|| "Font Awesome 7 Free".to_string()),
        }
    }
}

/// Measure text width with the same font family, weight resolution, shaping,
/// and default text scale used by the neo renderer.
pub fn measure_text_width(value: &str, font_family: &str, font_size: f32, font_weight: i32) -> f32 {
    if value.is_empty() {
        return 0.0;
    }
    let font_size = font_size.max(1.0);
    let key = MeasureKey {
        value: value.to_string(),
        font_family: font_family.to_string(),
        font_size_bits: font_size.to_bits(),
        font_weight,
    };
    if let Some(width) = TEXT_MEASURE_CACHE.with(|cache| cache.borrow().get(&key).copied()) {
        return width;
    }

    let width = TEXT_MEASURE_CONTEXT.with(|context| {
        let mut context = context.borrow_mut();
        measure_shaped_text_width(&mut context, value, font_family, font_size, font_weight)
    });

    TEXT_MEASURE_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.len() > 4096 {
            cache.clear();
        }
        cache.insert(key, width);
    });
    width
}

pub fn measureTextWidth(value: &str, font_family: &str, font_size: f32, font_weight: i32) -> f32 {
    measure_text_width(value, font_family, font_size, font_weight)
}

fn measure_shaped_text_width(
    context: &mut TextMeasureContext,
    value: &str,
    font_family: &str,
    font_size: f32,
    font_weight: i32,
) -> f32 {
    let icon_font = is_icon_family(font_family);
    let authored_font_size = font_size.max(1.0);
    let shaped_font_size = if icon_font {
        authored_font_size
    } else {
        authored_font_size * DEFAULT_TEXT_FONT_PIXEL_HEIGHT_SCALE
    }
    .max(1.0);
    let metrics_scale = if icon_font {
        1.0
    } else {
        DEFAULT_TEXT_FONT_PIXEL_HEIGHT_SCALE
    };
    let line_height = (authored_font_size * 1.2 * metrics_scale).max(shaped_font_size);
    let mut buffer = Buffer::new(
        &mut context.font_system,
        Metrics::new(shaped_font_size, line_height),
    );
    buffer.set_size(
        &mut context.font_system,
        Some(measurement_width_limit(value, shaped_font_size)),
        Some((line_height * value.lines().count().max(1) as f32).max(line_height)),
    );
    buffer.set_wrap(&mut context.font_system, Wrap::None);
    let attrs = Attrs::new()
        .family(resolve_family(
            font_family,
            context.default_text_family.as_deref(),
            context.default_icon_family.as_deref(),
        ))
        .weight(Weight(resolved_font_weight(
            font_family,
            font_weight,
            icon_font,
        )));
    buffer.set_text(
        &mut context.font_system,
        value,
        &attrs,
        Shaping::Advanced,
        None,
    );
    buffer.shape_until_scroll(&mut context.font_system, false);
    let width = buffer
        .layout_runs()
        .map(|run| run.line_w)
        .max_by(|left, right| left.total_cmp(right))
        .unwrap_or_default();
    if width > 0.0 {
        width
    } else {
        fallback_text_width(value, authored_font_size)
    }
}

fn measurement_width_limit(value: &str, font_size: f32) -> f32 {
    let count = value.chars().count().max(1) as f32;
    (count * font_size * 4.0)
        .max(font_size * 8.0)
        .min(1_000_000.0)
}

fn fallback_text_width(value: &str, font_size: f32) -> f32 {
    value.chars().count() as f32 * font_size * 0.5
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
