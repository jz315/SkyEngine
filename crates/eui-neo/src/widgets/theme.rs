//! Port of `EUI-NEO/components/theme.h`.

use crate::Color;

use super::super::{Border, Shadow};

#[derive(Debug, Clone, Copy)]
pub struct ThemeColorTokens {
    pub background: Color,
    pub primary: Color,
    pub surface: Color,
    pub surface_hover: Color,
    pub surface_active: Color,
    pub text: Color,
    pub border: Color,
    pub dark: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct PageVisualTokens {
    pub title_color: Color,
    pub subtitle_color: Color,
    pub body_color: Color,
    pub card_color: Color,
    pub muted_card_color: Color,
    pub soft_accent_color: Color,
    pub header_top_inset: f32,
    pub header_title_gap: f32,
    pub header_content_gap: f32,
    pub header_title_size: f32,
    pub header_subtitle_size: f32,
    pub section_gap: f32,
    pub section_inset: f32,
    pub section_rounding: f32,
    pub label_size: f32,
    pub field_height: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct FieldVisualTokens {
    pub rounding: f32,
    pub horizontal_inset: f32,
    pub focus_line_height: f32,
    pub border_line_height: f32,
    pub popup_rounding: f32,
    pub popup_overlap: f32,
    pub popup_shadow_color: Color,
    pub popup_shadow_blur: f32,
    pub popup_shadow_offset_y: f32,
}

pub type UiFieldVisualTokens = FieldVisualTokens;

#[derive(Debug, Clone, Copy, Default)]
pub struct PageHeaderLayout {
    pub title_y: f32,
    pub subtitle_y: f32,
    pub content_y: f32,
}

pub fn color(r: f32, g: f32, b: f32, a: f32) -> Color {
    Color::new(r, g, b, a)
}

pub fn default_primary(a: f32) -> Color {
    color(56.0 / 255.0, 113.0 / 255.0, 224.0 / 255.0, a)
}

pub fn with_alpha(mut value: Color, alpha: f32) -> Color {
    value.a = alpha.clamp(0.0, 1.0);
    value
}

pub fn with_opacity(mut value: Color, opacity: f32) -> Color {
    value.a *= opacity.clamp(0.0, 1.0);
    value
}

pub fn light() -> ThemeColorTokens {
    ThemeColorTokens {
        background: color(0.95, 0.95, 0.97, 1.0),
        primary: default_primary(1.0),
        surface: color(1.0, 1.0, 1.0, 1.0),
        surface_hover: color(0.90, 0.90, 0.90, 1.0),
        surface_active: color(0.80, 0.80, 0.80, 1.0),
        text: color(0.0, 0.0, 0.0, 1.0),
        border: color(0.80, 0.80, 0.80, 1.0),
        dark: false,
    }
}

pub fn dark() -> ThemeColorTokens {
    ThemeColorTokens {
        background: color(0.10, 0.10, 0.12, 1.0),
        primary: default_primary(1.0),
        surface: color(0.15, 0.15, 0.18, 1.0),
        surface_hover: color(0.25, 0.25, 0.28, 1.0),
        surface_active: color(0.35, 0.35, 0.38, 1.0),
        text: color(1.0, 1.0, 1.0, 1.0),
        border: color(0.30, 0.30, 0.30, 1.0),
        dark: true,
    }
}

pub fn light_theme_colors() -> ThemeColorTokens {
    light()
}

pub fn dark_theme_colors() -> ThemeColorTokens {
    dark()
}

pub fn page_visuals(tokens: ThemeColorTokens) -> PageVisualTokens {
    PageVisualTokens {
        title_color: with_alpha(tokens.text, 0.98),
        subtitle_color: with_alpha(tokens.text, 0.72),
        body_color: with_alpha(tokens.text, 0.68),
        card_color: tokens.surface,
        muted_card_color: tokens.surface_hover,
        soft_accent_color: with_alpha(tokens.primary, 0.16),
        header_top_inset: 24.0,
        header_title_gap: 30.0,
        header_content_gap: 40.0,
        header_title_size: 31.0,
        header_subtitle_size: 20.0,
        section_gap: 16.0,
        section_inset: 20.0,
        section_rounding: 18.0,
        label_size: 17.0,
        field_height: 35.0,
    }
}

pub fn field_visuals(tokens: ThemeColorTokens) -> FieldVisualTokens {
    FieldVisualTokens {
        rounding: 6.0,
        horizontal_inset: 10.0,
        focus_line_height: 2.0,
        border_line_height: 1.0,
        popup_rounding: 10.0,
        popup_overlap: 1.0,
        popup_shadow_color: if tokens.dark {
            color(0.0, 0.0, 0.0, 0.28)
        } else {
            color(0.10, 0.14, 0.22, 0.14)
        },
        popup_shadow_blur: if tokens.dark { 18.0 } else { 12.0 },
        popup_shadow_offset_y: if tokens.dark { 8.0 } else { 5.0 },
    }
}

pub fn current_page_visuals(tokens: ThemeColorTokens) -> PageVisualTokens {
    page_visuals(tokens)
}

pub fn current_field_visuals(tokens: ThemeColorTokens) -> UiFieldVisualTokens {
    field_visuals(tokens)
}

pub fn resolve_field_fill(
    tokens: ThemeColorTokens,
    base_color: Color,
    hover_amount: f32,
    active_amount: f32,
) -> Color {
    let hover = hover_amount.clamp(0.0, 1.0);
    let active = active_amount.clamp(0.0, 1.0);
    let base = if base_color.a > 0.0 {
        base_color
    } else {
        tokens.surface
    };
    let hover_color = if base_color.a > 0.0 {
        mix_color(base, tokens.surface_hover, 0.65)
    } else {
        tokens.surface_hover
    };
    mix_color(
        mix_color(base, tokens.surface_active, active),
        hover_color,
        hover,
    )
}

pub fn button_hover(tokens: ThemeColorTokens, base: Color) -> Color {
    mix_color(
        base,
        if tokens.dark {
            color(1.0, 1.0, 1.0, base.a)
        } else {
            tokens.primary
        },
        if tokens.dark { 0.16 } else { 0.10 },
    )
}

pub fn button_pressed(tokens: ThemeColorTokens, base: Color) -> Color {
    mix_color(
        base,
        if tokens.dark {
            color(0.0, 0.0, 0.0, base.a)
        } else {
            tokens.surface_active
        },
        if tokens.dark { 0.34 } else { 0.22 },
    )
}

pub fn border(tokens: ThemeColorTokens, width: f32, opacity: f32) -> Border {
    Border {
        width,
        color: with_opacity(tokens.border, opacity),
    }
}

pub fn button_border(tokens: ThemeColorTokens, primary: bool) -> Border {
    Border {
        width: 1.0,
        color: if primary {
            with_alpha(tokens.primary, 0.58)
        } else {
            with_opacity(tokens.border, 0.70)
        },
    }
}

pub fn shadow(
    tokens: ThemeColorTokens,
    blur: f32,
    offset_y: f32,
    dark_alpha: f32,
    light_alpha: f32,
) -> Shadow {
    Shadow {
        enabled: true,
        offset: [0.0, offset_y],
        blur,
        spread: 0.0,
        color: if tokens.dark {
            color(0.0, 0.0, 0.0, dark_alpha)
        } else {
            color(0.10, 0.14, 0.22, light_alpha)
        },
    }
}

pub fn button_shadow(tokens: ThemeColorTokens) -> Shadow {
    shadow(tokens, 14.0, 4.0, 0.22, 0.10)
}

pub fn panel_shadow(tokens: ThemeColorTokens) -> Shadow {
    shadow(tokens, 24.0, 8.0, 0.28, 0.12)
}

pub fn popup_shadow(tokens: ThemeColorTokens) -> Shadow {
    let field = field_visuals(tokens);
    Shadow {
        enabled: true,
        offset: [0.0, field.popup_shadow_offset_y],
        blur: field.popup_shadow_blur,
        spread: 0.0,
        color: field.popup_shadow_color,
    }
}

pub fn mix_color(from: Color, to: Color, amount: f32) -> Color {
    let amount = amount.clamp(0.0, 1.0);
    let inverse = 1.0 - amount;
    Color {
        r: from.r * inverse + to.r * amount,
        g: from.g * inverse + to.g * amount,
        b: from.b * inverse + to.b * amount,
        a: from.a * inverse + to.a * amount,
    }
}

pub fn defaultPrimary(a: f32) -> Color {
    default_primary(a)
}

pub fn withAlpha(value: Color, alpha: f32) -> Color {
    with_alpha(value, alpha)
}

pub fn withOpacity(value: Color, opacity: f32) -> Color {
    with_opacity(value, opacity)
}

pub fn LightThemeColors() -> ThemeColorTokens {
    light_theme_colors()
}

pub fn DarkThemeColors() -> ThemeColorTokens {
    dark_theme_colors()
}

pub fn pageVisuals(tokens: ThemeColorTokens) -> PageVisualTokens {
    page_visuals(tokens)
}

pub fn fieldVisuals(tokens: ThemeColorTokens) -> FieldVisualTokens {
    field_visuals(tokens)
}

pub fn CurrentPageVisuals(tokens: ThemeColorTokens) -> PageVisualTokens {
    current_page_visuals(tokens)
}

pub fn CurrentFieldVisuals(tokens: ThemeColorTokens) -> UiFieldVisualTokens {
    current_field_visuals(tokens)
}

pub fn resolveFieldFill(
    tokens: ThemeColorTokens,
    base_color: Color,
    hover_amount: f32,
    active_amount: f32,
) -> Color {
    resolve_field_fill(tokens, base_color, hover_amount, active_amount)
}

pub fn ResolveFieldFill(
    tokens: ThemeColorTokens,
    base_color: Color,
    hover_amount: f32,
    active_amount: f32,
) -> Color {
    resolve_field_fill(tokens, base_color, hover_amount, active_amount)
}

pub fn buttonHover(tokens: ThemeColorTokens, base: Color) -> Color {
    button_hover(tokens, base)
}

pub fn buttonPressed(tokens: ThemeColorTokens, base: Color) -> Color {
    button_pressed(tokens, base)
}

pub fn buttonBorder(tokens: ThemeColorTokens, primary: bool) -> Border {
    button_border(tokens, primary)
}

pub fn buttonShadow(tokens: ThemeColorTokens) -> Shadow {
    button_shadow(tokens)
}

pub fn panelShadow(tokens: ThemeColorTokens) -> Shadow {
    panel_shadow(tokens)
}

pub fn popupShadow(tokens: ThemeColorTokens) -> Shadow {
    popup_shadow(tokens)
}

pub fn mixColor(from: Color, to: Color, amount: f32) -> Color {
    mix_color(from, to, amount)
}
