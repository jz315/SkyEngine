use sky_engine::ui::neo::widgets::theme as neo_theme;
use sky_engine::ui::neo::widgets::theme::{PageVisualTokens, ThemeColorTokens};
use sky_engine::ui::neo::Color;

use crate::model::{Priority, RunState, ThemeMode};

#[derive(Debug, Clone, Copy)]
pub struct AppTheme {
    pub tokens: ThemeColorTokens,
    pub background_top: Color,
    pub background_bottom: Color,
    pub shell: Color,
    pub shell_edge: Color,
    pub panel: Color,
    pub panel_alt: Color,
    pub text_soft: Color,
    pub text_muted: Color,
    pub success: Color,
    pub warning: Color,
    pub danger: Color,
}

pub fn resolve(mode: ThemeMode) -> AppTheme {
    let tokens = match mode {
        ThemeMode::Nocturne => ThemeColorTokens {
            background: color(0.05, 0.06, 0.09, 1.0),
            primary: color(0.28, 0.62, 0.97, 1.0),
            surface: color(0.09, 0.11, 0.15, 1.0),
            surface_hover: color(0.13, 0.16, 0.22, 1.0),
            surface_active: color(0.17, 0.21, 0.28, 1.0),
            text: color(0.95, 0.97, 1.0, 1.0),
            border: color(0.19, 0.24, 0.33, 1.0),
            dark: true,
        },
        ThemeMode::Studio => ThemeColorTokens {
            background: color(0.07, 0.08, 0.07, 1.0),
            primary: color(0.22, 0.78, 0.63, 1.0),
            surface: color(0.11, 0.13, 0.12, 1.0),
            surface_hover: color(0.15, 0.18, 0.16, 1.0),
            surface_active: color(0.20, 0.23, 0.21, 1.0),
            text: color(0.94, 0.98, 0.96, 1.0),
            border: color(0.20, 0.28, 0.25, 1.0),
            dark: true,
        },
        ThemeMode::Paper => ThemeColorTokens {
            background: color(0.94, 0.93, 0.90, 1.0),
            primary: color(0.20, 0.45, 0.82, 1.0),
            surface: color(0.985, 0.98, 0.965, 1.0),
            surface_hover: color(0.92, 0.90, 0.86, 1.0),
            surface_active: color(0.86, 0.84, 0.80, 1.0),
            text: color(0.11, 0.12, 0.15, 1.0),
            border: color(0.76, 0.73, 0.68, 1.0),
            dark: false,
        },
    };
    let _page: PageVisualTokens = neo_theme::page_visuals(tokens);

    AppTheme {
        tokens,
        background_top: mix(
            tokens.background,
            tokens.primary,
            if tokens.dark { 0.10 } else { 0.03 },
        ),
        background_bottom: mix(
            tokens.background,
            color(0.0, 0.0, 0.0, 1.0),
            if tokens.dark { 0.28 } else { 0.02 },
        ),
        shell: mix(
            tokens.surface,
            tokens.background,
            if tokens.dark { 0.22 } else { 0.08 },
        ),
        shell_edge: alpha(tokens.border, if tokens.dark { 0.90 } else { 0.75 }),
        panel: tokens.surface,
        panel_alt: mix(tokens.surface, tokens.surface_hover, 0.54),
        text_soft: neo_theme::with_opacity(tokens.text, 0.78),
        text_muted: neo_theme::with_opacity(tokens.text, 0.58),
        success: color(0.20, 0.76, 0.52, 1.0),
        warning: color(0.96, 0.72, 0.20, 1.0),
        danger: color(0.94, 0.36, 0.42, 1.0),
    }
}

pub fn run_state_color(theme: AppTheme, state: RunState) -> Color {
    match state {
        RunState::Ready => theme.tokens.primary,
        RunState::Running => theme.success,
        RunState::Paused => theme.warning,
        RunState::Shipping => mix(theme.tokens.primary, theme.warning, 0.38),
    }
}

pub fn priority_color(theme: AppTheme, priority: Priority, urgent: bool) -> Color {
    if urgent {
        return theme.danger;
    }
    match priority {
        Priority::Low => mix(theme.tokens.primary, theme.tokens.surface_hover, 0.52),
        Priority::Medium => theme.warning,
        Priority::High => theme.tokens.primary,
    }
}

pub fn alpha(color: Color, value: f32) -> Color {
    neo_theme::with_alpha(color, value)
}

pub fn mix(from: Color, to: Color, amount: f32) -> Color {
    neo_theme::mix_color(from, to, amount)
}

fn color(r: f32, g: f32, b: f32, a: f32) -> Color {
    Color::new(r, g, b, a)
}
