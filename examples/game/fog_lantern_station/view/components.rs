use sky_engine::ui::serein::widgets;
use sky_engine::ui::serein::{Color, HorizontalAlign, Size, Ui, VerticalAlign};

use crate::theme::{self, AppTheme};

pub const VIRTUAL_LIST_OVERSCAN: f32 = 220.0;

pub struct VirtualList {
    id_prefix: String,
    cursor: f32,
    visible_top: f32,
    visible_bottom: f32,
    pending_spacer: f32,
    spacer_index: usize,
}

impl VirtualList {
    pub fn new(id_prefix: impl Into<String>, scroll_offset: f32, viewport_height: f32) -> Self {
        Self {
            id_prefix: id_prefix.into(),
            cursor: 0.0,
            visible_top: (scroll_offset - VIRTUAL_LIST_OVERSCAN).max(0.0),
            visible_bottom: scroll_offset + viewport_height + VIRTUAL_LIST_OVERSCAN,
            pending_spacer: 0.0,
            spacer_index: 0,
        }
    }

    pub fn row(
        &mut self,
        ui: &mut Ui,
        id: impl Into<String>,
        width: f32,
        height: f32,
        render: impl FnOnce(&mut Ui),
    ) {
        if height <= 0.0 {
            return;
        }
        let top = self.cursor;
        let bottom = top + height;
        self.cursor = bottom;

        if bottom < self.visible_top || top > self.visible_bottom {
            self.pending_spacer += height;
            return;
        }

        self.flush_spacer(ui, width);
        ui.stack(id.into()).size(width, height).content(render);
    }

    pub fn finish(&mut self, ui: &mut Ui, width: f32) {
        self.flush_spacer(ui, width);
    }

    pub fn spacer(&mut self, height: f32) {
        if height <= 0.0 {
            return;
        }
        self.cursor += height;
        self.pending_spacer += height;
    }

    fn flush_spacer(&mut self, ui: &mut Ui, width: f32) {
        if self.pending_spacer <= 0.0 {
            return;
        }
        ui.stack(format!("{}.spacer.{}", self.id_prefix, self.spacer_index))
            .size(width, self.pending_spacer)
            .content(|_| {});
        self.pending_spacer = 0.0;
        self.spacer_index += 1;
    }
}

pub fn panel(ui: &mut Ui, id: &str, width: f32, height: f32, app_theme: AppTheme) {
    widgets::panel(ui, id)
        .size(width, height)
        .color(app_theme.panel)
        .border(1.0, app_theme.border)
        .shadow(18.0, 0.0, 8.0, theme::alpha(Color::BLACK, 0.18))
        .radius(8.0)
        .build();
}

pub fn body_text(
    ui: &mut Ui,
    id: impl Into<String>,
    text: impl Into<String>,
    width: f32,
    height: f32,
    color: Color,
    font_size: f32,
) {
    ui.text(id)
        .size(width, height)
        .text(text)
        .font_size(font_size)
        .line_height(font_size + 8.0)
        .wrap(true)
        .max_width(width)
        .color(color)
        .build();
}

pub fn badge(
    ui: &mut Ui,
    id: impl Into<String>,
    width: f32,
    text: impl Into<String>,
    accent: Color,
    app_theme: AppTheme,
) {
    let id = id.into();
    ui.stack(id.clone()).size(width, 28.0).content(|ui| {
        ui.rect(format!("{id}.bg"))
            .size(width, 28.0)
            .color(theme::alpha(accent, 0.14))
            .border(1.0, theme::alpha(accent, 0.34))
            .radius(6.0)
            .build();
        ui.text(format!("{id}.text"))
            .size(width, 28.0)
            .text(text)
            .font_size(12.0)
            .line_height(16.0)
            .color(if app_theme.tokens.dark {
                theme::mix(accent, Color::WHITE, 0.38)
            } else {
                accent
            })
            .horizontal_align(HorizontalAlign::Center)
            .vertical_align(VerticalAlign::Center)
            .build();
    });
}

pub fn button_style(app_theme: AppTheme, primary: bool) -> widgets::ButtonStyle {
    let mut style = widgets::ButtonStyle::new(app_theme.tokens, primary);
    style.radius = 6.0;
    style.shadow.enabled = false;
    if !primary {
        style.normal = app_theme.panel_alt;
        style.hover = theme::mix(app_theme.panel_alt, app_theme.accent, 0.18);
        style.pressed = theme::mix(app_theme.panel_alt, Color::BLACK, 0.20);
        style.text = app_theme.text;
        style.border.color = app_theme.border;
    }
    style
}

pub fn text_button(
    ui: &mut Ui,
    id: impl Into<String>,
    width: impl Into<Size>,
    height: f32,
    label: impl Into<String>,
    primary: bool,
    enabled: bool,
    app_theme: AppTheme,
    on_click: impl FnMut() + 'static,
) {
    widgets::button(ui, id)
        .size(width, height)
        .min_width(88.0)
        .text(label)
        .font_size(15.0)
        .style(button_style(app_theme, primary))
        .disabled(!enabled)
        .on_click(on_click)
        .build();
}

pub fn section_label(ui: &mut Ui, id: impl Into<String>, text: impl Into<String>, width: f32) {
    ui.text(id)
        .size(width, 20.0)
        .text(text)
        .font_size(12.0)
        .line_height(16.0)
        .color(theme::station_theme().text_muted)
        .build();
}
