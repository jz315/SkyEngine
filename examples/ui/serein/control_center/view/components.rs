use sky_engine::ui::serein::widgets;
use sky_engine::ui::serein::Color;
use sky_engine::ui::serein::{HorizontalAlign, Size, Ui, VerticalAlign};

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
            .fill()
            .color(app_theme.panel)
            .border(1.0, app_theme.shell_edge)
            .shadow(20.0, 0.0, 8.0, theme::alpha(Color::BLACK, 0.12))
            .radius(20.0)
            .build();

        ui.column(format!("{id}.content"))
            .fill()
            .padding_each(22.0, 20.0, 22.0, 14.0)
            .gap(6.0)
            .content(|ui| {
                ui.rect(format!("{id}.accent"))
                    .size(42.0, 6.0)
                    .color(accent)
                    .radius(3.0)
                    .build();

                ui.text(format!("{id}.label"))
                    .size(Size::fill(), 18.0)
                    .text(label)
                    .font_size(14.0)
                    .line_height(18.0)
                    .color(app_theme.text_muted)
                    .build();

                ui.text(format!("{id}.value"))
                    .size(Size::fill(), 38.0)
                    .text(value)
                    .font_size(30.0)
                    .line_height(34.0)
                    .color(app_theme.tokens.text)
                    .build();

                ui.text(format!("{id}.meta"))
                    .size(Size::fill(), 18.0)
                    .text(meta)
                    .font_size(13.0)
                    .line_height(18.0)
                    .color(app_theme.text_soft)
                    .build();
            });
    });
}

pub fn section_frame(
    ui: &mut Ui,
    id: &str,
    width: impl Into<Size>,
    height: impl Into<Size>,
    title: &str,
    subtitle: &str,
    app_theme: AppTheme,
    content: impl FnOnce(&mut Ui, f32, f32),
) {
    let width = width.into();
    let height = height.into();
    let fixed_width = match width {
        Size::Fixed(value) => value,
        Size::WrapContent | Size::Fill => 0.0,
    };
    let fixed_height = match height {
        Size::Fixed(value) => value,
        Size::WrapContent | Size::Fill => 0.0,
    };
    let body_top = if subtitle.is_empty() { 56.0 } else { 84.0 };
    let body_width = (fixed_width - 40.0).max(0.0);
    let body_height = (fixed_height - body_top - 18.0).max(0.0);
    let body_size = if body_height > 0.0 {
        Size::Fixed(body_height)
    } else {
        Size::Fill
    };

    ui.stack(id.to_string())
        .size(width, height)
        .content(move |ui| {
            widgets::panel(ui, format!("{id}.bg"))
                .fill()
                .color(app_theme.panel)
                .border(1.0, app_theme.shell_edge)
                .shadow(24.0, 0.0, 8.0, theme::alpha(Color::BLACK, 0.12))
                .radius(22.0)
                .build();

            ui.column(format!("{id}.layout"))
                .fill()
                .padding_each(20.0, 18.0, 20.0, 18.0)
                .gap(if subtitle.is_empty() { 10.0 } else { 8.0 })
                .content(|ui| {
                    ui.text(format!("{id}.title"))
                        .size(Size::fill(), 28.0)
                        .text(title)
                        .font_size(22.0)
                        .line_height(28.0)
                        .color(app_theme.tokens.text)
                        .build();

                    if !subtitle.is_empty() {
                        ui.text(format!("{id}.subtitle"))
                            .size(Size::fill(), 22.0)
                            .text(subtitle)
                            .font_size(14.0)
                            .line_height(20.0)
                            .color(app_theme.text_muted)
                            .build();
                    }

                    ui.column(format!("{id}.body"))
                        .size(Size::fill(), body_size)
                        .grow(1.0)
                        .gap(8.0)
                        .content(|ui| content(ui, body_width, body_height));
                });
        });
}

pub fn badge(ui: &mut Ui, id: &str, width: f32, text: &str, accent: Color, app_theme: AppTheme) {
    ui.stack(id.to_string()).size(width, 34.0).content(|ui| {
        ui.rect(format!("{id}.bg"))
            .fill()
            .color(theme::alpha(
                accent,
                if app_theme.tokens.dark { 0.18 } else { 0.10 },
            ))
            .border(1.0, theme::alpha(accent, 0.34))
            .radius(17.0)
            .build();

        ui.text(format!("{id}.text"))
            .fill()
            .text(text)
            .font_size(13.0)
            .line_height(16.0)
            .color(accent)
            .vertical_align(VerticalAlign::Center)
            .horizontal_align(HorizontalAlign::Center)
            .build();
    });
}

pub fn progress_bar(ui: &mut Ui, id: &str, value: f32, app_theme: AppTheme) {
    let value = value.clamp(0.0, 1.0);
    ui.stack(id.to_string())
        .size(Size::fill(), 14.0)
        .content(|ui| {
            ui.rect(format!("{id}.track"))
                .fill()
                .color(app_theme.tokens.surface_hover)
                .radius(7.0)
                .build();

            ui.row(format!("{id}.fill.row"))
                .fill()
                .clip()
                .content(|ui| {
                    ui.rect(format!("{id}.fill"))
                        .size(0.0, Size::fill())
                        .grow(value)
                        .color(app_theme.tokens.primary)
                        .radius(7.0)
                        .build();
                    spacer_with_grow(ui, &format!("{id}.remaining"), 1.0 - value);
                });
        });
}

pub fn spacer(ui: &mut Ui, id: &str) {
    spacer_with_grow(ui, id, 1.0);
}

pub fn spacer_with_grow(ui: &mut Ui, id: &str, grow: f32) {
    ui.stack(id.to_string())
        .size(Size::fill(), 0.0)
        .grow(grow.max(0.0))
        .content(|_| {});
}
