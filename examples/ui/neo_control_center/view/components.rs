use sky_engine::ui::neo::widgets;
use sky_engine::ui::neo::Color;
use sky_engine::ui::neo::{HorizontalAlign, Ui, VerticalAlign};

use crate::theme::{self, AppTheme};

pub fn stat_card(
    ui: &mut Ui,
    id: &str,
    width: f32,
    height: f32,
    label: &str,
    value: &str,
    meta: &str,
    accent: Color,
    app_theme: AppTheme,
) {
    ui.stack(id.to_string()).size(width, height).content(|ui| {
        widgets::panel(ui, format!("{id}.bg"))
            .size(width, height)
            .color(app_theme.panel)
            .border(1.0, app_theme.shell_edge)
            .shadow(20.0, 0.0, 8.0, theme::alpha(Color::BLACK, 0.12))
            .radius(20.0)
            .build();

        ui.rect(format!("{id}.accent"))
            .x(22.0)
            .y(20.0)
            .size(42.0, 6.0)
            .color(accent)
            .radius(3.0)
            .build();

        ui.text(format!("{id}.label"))
            .x(22.0)
            .y(36.0)
            .size(width - 44.0, 18.0)
            .text(label)
            .font_size(14.0)
            .line_height(18.0)
            .color(app_theme.text_muted)
            .build();

        ui.text(format!("{id}.value"))
            .x(22.0)
            .y(60.0)
            .size(width - 44.0, 38.0)
            .text(value)
            .font_size(30.0)
            .line_height(34.0)
            .color(app_theme.tokens.text)
            .build();

        ui.text(format!("{id}.meta"))
            .x(22.0)
            .y(height - 32.0)
            .size(width - 44.0, 18.0)
            .text(meta)
            .font_size(13.0)
            .line_height(18.0)
            .color(app_theme.text_soft)
            .build();
    });
}

pub fn section_frame(
    ui: &mut Ui,
    id: &str,
    width: f32,
    height: f32,
    title: &str,
    subtitle: &str,
    app_theme: AppTheme,
    content: impl FnOnce(&mut Ui, f32, f32),
) {
    let body_top = if subtitle.is_empty() { 70.0 } else { 90.0 };
    let body_width = (width - 40.0).max(0.0);
    let body_height = (height - body_top - 18.0).max(0.0);

    ui.stack(id.to_string())
        .size(width, height)
        .content(move |ui| {
            widgets::panel(ui, format!("{id}.bg"))
                .size(width, height)
                .color(app_theme.panel)
                .border(1.0, app_theme.shell_edge)
                .shadow(24.0, 0.0, 8.0, theme::alpha(Color::BLACK, 0.12))
                .radius(22.0)
                .build();

            ui.text(format!("{id}.title"))
                .x(20.0)
                .y(18.0)
                .size(body_width, 28.0)
                .text(title)
                .font_size(22.0)
                .line_height(28.0)
                .color(app_theme.tokens.text)
                .build();

            if !subtitle.is_empty() {
                ui.text(format!("{id}.subtitle"))
                    .x(20.0)
                    .y(50.0)
                    .size(body_width, 22.0)
                    .text(subtitle)
                    .font_size(14.0)
                    .line_height(20.0)
                    .color(app_theme.text_muted)
                    .build();
            }

            ui.stack(format!("{id}.body"))
                .x(20.0)
                .y(body_top)
                .size(body_width, body_height)
                .content(|ui| content(ui, body_width, body_height));
        });
}

pub fn badge(ui: &mut Ui, id: &str, width: f32, text: &str, accent: Color, app_theme: AppTheme) {
    ui.stack(id.to_string()).size(width, 34.0).content(|ui| {
        ui.rect(format!("{id}.bg"))
            .size(width, 34.0)
            .color(theme::alpha(
                accent,
                if app_theme.tokens.dark { 0.18 } else { 0.10 },
            ))
            .border(1.0, theme::alpha(accent, 0.34))
            .radius(17.0)
            .build();

        ui.text(format!("{id}.text"))
            .size(width, 34.0)
            .text(text)
            .font_size(13.0)
            .line_height(16.0)
            .color(accent)
            .vertical_align(VerticalAlign::Center)
            .horizontal_align(HorizontalAlign::Center)
            .build();
    });
}
