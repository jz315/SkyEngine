use sky_engine::ui::serein::widgets::theme::ThemeColorTokens;
use sky_engine::ui::serein::Color;

#[derive(Clone, Copy)]
pub struct AppTheme {
    pub tokens: ThemeColorTokens,
    pub background_top: Color,
    pub background_bottom: Color,
    pub panel: Color,
    pub panel_alt: Color,
    pub panel_strong: Color,
    pub border: Color,
    pub text: Color,
    pub text_soft: Color,
    pub text_muted: Color,
    pub accent: Color,
    pub accent_warm: Color,
    pub danger: Color,
}

pub fn station_theme() -> AppTheme {
    let tokens = ThemeColorTokens {
        dark: true,
        background: color(0.035, 0.047, 0.052, 1.0),
        primary: color(0.34, 0.68, 0.72, 1.0),
        surface: color(0.075, 0.095, 0.110, 0.96),
        surface_hover: color(0.115, 0.145, 0.160, 0.98),
        surface_active: color(0.050, 0.065, 0.078, 1.0),
        text: color(0.925, 0.945, 0.915, 1.0),
        border: color(0.270, 0.355, 0.360, 0.72),
    };

    AppTheme {
        tokens,
        background_top: color(0.035, 0.047, 0.052, 1.0),
        background_bottom: color(0.095, 0.078, 0.062, 1.0),
        panel: color(0.055, 0.070, 0.080, 0.94),
        panel_alt: color(0.090, 0.104, 0.105, 0.95),
        panel_strong: color(0.035, 0.044, 0.050, 0.98),
        border: color(0.350, 0.430, 0.420, 0.42),
        text: tokens.text,
        text_soft: color(0.760, 0.810, 0.775, 1.0),
        text_muted: color(0.575, 0.645, 0.640, 1.0),
        accent: tokens.primary,
        accent_warm: color(0.900, 0.620, 0.300, 1.0),
        danger: color(0.820, 0.280, 0.240, 1.0),
    }
}

pub fn color(r: f32, g: f32, b: f32, a: f32) -> Color {
    Color::new(r, g, b, a)
}

pub fn alpha(color: Color, a: f32) -> Color {
    Color::new(color.r, color.g, color.b, a)
}

pub fn mix(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    Color::new(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
        a.a + (b.a - a.a) * t,
    )
}
