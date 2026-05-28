//! Port of `EUI-NEO/components/button.h`.

use crate::Color;

use super::super::{
    Align, Border, HorizontalAlign, LayoutRect, PointerEvent, Response, Shadow, Size, Transition,
    Ui, VerticalAlign,
};
use super::layout::{scale_size, WidgetLayout};
use super::text::measure_text_width;
use super::theme::{self, ThemeColorTokens};

const DEFAULT_WIDTH: f32 = 240.0;
const DEFAULT_HEIGHT: f32 = 70.0;
const HORIZONTAL_PADDING: f32 = 32.0;

#[derive(Debug, Clone, Copy)]
pub struct ButtonStyle {
    pub normal: Color,
    pub hover: Color,
    pub pressed: Color,
    pub text: Color,
    pub icon: Color,
    pub border: Border,
    pub shadow: Shadow,
    pub radius: f32,
    pub opacity: f32,
    pub press_scale: f32,
}

impl ButtonStyle {
    pub fn new(tokens: ThemeColorTokens, primary: bool) -> Self {
        let base = if primary {
            tokens.primary
        } else {
            tokens.surface
        };
        let text = if primary || tokens.dark {
            Color::new(0.94, 0.97, 1.0, 1.0)
        } else {
            tokens.text
        };
        Self {
            normal: base,
            hover: theme::button_hover(tokens, base),
            pressed: theme::button_pressed(tokens, base),
            text,
            icon: text,
            border: theme::button_border(tokens, primary),
            shadow: theme::button_shadow(tokens),
            radius: 16.0,
            opacity: 1.0,
            press_scale: 0.965,
        }
    }
}

impl Default for ButtonStyle {
    fn default() -> Self {
        Self::new(theme::dark_theme_colors(), true)
    }
}

pub struct ButtonBuilder<'ui> {
    ui: &'ui mut Ui,
    id: String,
    text: String,
    icon: String,
    style: ButtonStyle,
    theme_tokens: Option<ThemeColorTokens>,
    theme_primary: bool,
    selected: bool,
    transition: Transition,
    on_click: Option<Box<dyn FnMut()>>,
    on_context_menu: Option<Box<dyn FnMut(PointerEvent, LayoutRect)>>,
    layout: WidgetLayout,
    scale: f32,
    font_size: f32,
    icon_size: f32,
    translate_x: f32,
    translate_y: f32,
    disabled: bool,
}

impl<'ui> ButtonBuilder<'ui> {
    pub fn new(ui: &'ui mut Ui, id: impl Into<String>) -> Self {
        Self {
            ui,
            id: id.into(),
            text: "Button".to_string(),
            icon: String::new(),
            style: ButtonStyle::default(),
            theme_tokens: Some(theme::dark_theme_colors()),
            theme_primary: true,
            selected: false,
            transition: Transition::responsive(),
            on_click: None,
            on_context_menu: None,
            layout: WidgetLayout::new(DEFAULT_WIDTH, DEFAULT_HEIGHT).width(Size::WrapContent),
            scale: 1.0,
            font_size: 0.0,
            icon_size: 0.0,
            translate_x: 0.0,
            translate_y: 0.0,
            disabled: false,
        }
    }

    pub fn width(mut self, value: impl Into<Size>) -> Self {
        self.layout = self.layout.width(value);
        self
    }

    pub fn height(mut self, value: impl Into<Size>) -> Self {
        self.layout = self.layout.height(value);
        self
    }

    pub fn size(mut self, width: impl Into<Size>, height: impl Into<Size>) -> Self {
        self.layout = self.layout.size(width, height);
        self
    }

    pub fn margin(mut self, value: f32) -> Self {
        self.layout = self.layout.margin(value);
        self
    }

    pub fn margin_xy(mut self, horizontal: f32, vertical: f32) -> Self {
        self.layout = self.layout.margin_xy(horizontal, vertical);
        self
    }

    pub fn margin_each(mut self, left: f32, top: f32, right: f32, bottom: f32) -> Self {
        self.layout = self.layout.margin_each(left, top, right, bottom);
        self
    }

    pub fn min_width(mut self, value: f32) -> Self {
        self.layout = self.layout.min_width(value);
        self
    }

    pub fn max_width(mut self, value: f32) -> Self {
        self.layout = self.layout.max_width(value);
        self
    }

    pub fn min_height(mut self, value: f32) -> Self {
        self.layout = self.layout.min_height(value);
        self
    }

    pub fn max_height(mut self, value: f32) -> Self {
        self.layout = self.layout.max_height(value);
        self
    }

    pub fn grow(mut self, value: f32) -> Self {
        self.layout = self.layout.grow(value);
        self
    }

    pub fn scale(mut self, value: f32) -> Self {
        self.scale = value;
        self
    }

    pub fn text(mut self, value: impl Into<String>) -> Self {
        self.text = value.into();
        self
    }

    pub fn icon(mut self, value: impl Into<String>) -> Self {
        self.icon = value.into();
        self
    }

    pub fn icon_codepoint(mut self, codepoint: u32) -> Self {
        self.icon = utf8(codepoint);
        self
    }

    pub fn font_size(mut self, value: f32) -> Self {
        self.font_size = value;
        self
    }

    pub fn icon_size(mut self, value: f32) -> Self {
        self.icon_size = value;
        self
    }

    pub fn text_color(mut self, value: impl Into<Color>) -> Self {
        self.style.text = value.into();
        self
    }

    pub fn icon_color(mut self, value: impl Into<Color>) -> Self {
        self.style.icon = value.into();
        self
    }

    pub fn style(mut self, value: ButtonStyle) -> Self {
        self.style = value;
        self.theme_tokens = None;
        self
    }

    pub fn theme(mut self, tokens: ThemeColorTokens, primary: bool) -> Self {
        self.theme_tokens = Some(tokens);
        self.theme_primary = primary;
        self.apply_theme_style();
        self
    }

    pub fn primary_theme(self, tokens: ThemeColorTokens) -> Self {
        self.theme(tokens, true)
    }

    pub fn secondary_theme(self, tokens: ThemeColorTokens) -> Self {
        self.theme(tokens, false)
    }

    pub fn radius(mut self, value: f32) -> Self {
        self.style.radius = value;
        self
    }

    pub fn rounding(self, value: f32) -> Self {
        self.radius(value)
    }

    pub fn opacity(mut self, value: f32) -> Self {
        self.style.opacity = value.clamp(0.0, 1.0);
        self
    }

    pub fn selected(mut self, value: bool) -> Self {
        self.selected = value;
        self.apply_theme_style();
        self
    }

    pub fn disabled(mut self, value: bool) -> Self {
        self.disabled = value;
        self
    }

    pub fn enabled(self, value: bool) -> Self {
        self.disabled(!value)
    }

    pub fn translate(mut self, x: f32, y: f32) -> Self {
        self.translate_x = x;
        self.translate_y = y;
        self
    }

    pub fn translate_x(mut self, value: f32) -> Self {
        self.translate_x = value;
        self
    }

    pub fn translate_y(mut self, value: f32) -> Self {
        self.translate_y = value;
        self
    }

    pub fn press_scale(mut self, value: f32) -> Self {
        self.style.press_scale = value.clamp(0.80, 1.0);
        self
    }

    pub fn border(mut self, width: f32, color: impl Into<Color>) -> Self {
        self.style.border = Border {
            width,
            color: color.into(),
        };
        self
    }

    pub fn shadow(
        mut self,
        blur: f32,
        offset_x: f32,
        offset_y: f32,
        color: impl Into<Color>,
    ) -> Self {
        self.style.shadow = Shadow {
            enabled: true,
            offset: [offset_x, offset_y],
            blur,
            spread: 0.0,
            color: color.into(),
        };
        self
    }

    pub fn colors(
        mut self,
        normal: impl Into<Color>,
        hover: impl Into<Color>,
        pressed: impl Into<Color>,
    ) -> Self {
        self.style.normal = normal.into();
        self.style.hover = hover.into();
        self.style.pressed = pressed.into();
        self
    }

    pub fn transition(mut self, value: Transition) -> Self {
        self.transition = value;
        self
    }

    pub fn on_click<F>(mut self, callback: F) -> Self
    where
        F: FnMut() + 'static,
    {
        self.on_click = Some(Box::new(callback));
        self
    }

    pub fn on_context_menu<F>(mut self, callback: F) -> Self
    where
        F: FnMut(PointerEvent, LayoutRect) + 'static,
    {
        self.on_context_menu = Some(Box::new(callback));
        self
    }

    fn apply_theme_style(&mut self) {
        if let Some(tokens) = self.theme_tokens {
            self.style = ButtonStyle::new(tokens, self.theme_primary || self.selected);
        }
    }

    pub fn build(mut self) -> Response {
        let id = self.id.clone();
        let bg_id = format!("{id}.bg");
        let content_id = format!("{id}.content");
        let text_id = format!("{id}.text");
        let icon_id = format!("{id}.icon");
        let base_height = self.layout.fixed_height_or(DEFAULT_HEIGHT);
        let height = base_height * self.scale;
        let font = if self.font_size > 0.0 {
            self.font_size * self.scale
        } else {
            height * 0.46
        };
        let icon_font = if self.icon_size > 0.0 {
            self.icon_size * self.scale
        } else {
            font * 0.92
        };
        let has_icon = !self.icon.is_empty();
        let icon_width = if has_icon { icon_font * 1.15 } else { 0.0 };
        let gap = if has_icon {
            (6.0 * self.scale).max(height * 0.12)
        } else {
            0.0
        };
        let measured_text_width = measure_text_width(&self.text, "", font, 400);
        let natural_width = if has_icon {
            measured_text_width + icon_width + gap + HORIZONTAL_PADDING * self.scale
        } else {
            measured_text_width + HORIZONTAL_PADDING * self.scale
        };
        let root_width = scale_size(self.layout.width, self.scale, natural_width);
        let root_height = scale_size(self.layout.height, self.scale, DEFAULT_HEIGHT);
        let label_width = match root_width {
            Size::Fixed(width) if has_icon => {
                Size::Fixed((width - icon_width - gap - HORIZONTAL_PADDING * self.scale).max(0.0))
            }
            Size::Fixed(width) => Size::Fixed(width),
            Size::WrapContent | Size::Fill => Size::WrapContent,
        };
        let mut border = self.style.border;
        border.width *= self.scale;
        let mut shadow = self.style.shadow;
        shadow.offset[0] *= self.scale;
        shadow.offset[1] *= self.scale;
        shadow.blur *= self.scale;
        shadow.spread *= self.scale;
        let mut text_color = self.style.text;
        let mut icon_color = self.style.icon;
        text_color.a *= self.style.opacity;
        icon_color.a *= self.style.opacity;
        let style = self.style;
        let transition = self.transition;
        let text = self.text.clone();
        let icon = self.icon.clone();
        let translate_x = self.translate_x;
        let translate_y = self.translate_y;
        let disabled = self.disabled;
        let on_click = self.on_click.take();
        let on_context_menu = self.on_context_menu.take();

        self.layout
            .apply_to_size(self.ui.stack(id), root_width, root_height)
            .visual_state_from(&bg_id, style.press_scale)
            .content(|ui| {
                let bg = ui
                    .rect(bg_id.as_str())
                    .fill()
                    .states(style.normal, style.hover, style.pressed)
                    .radius(style.radius * self.scale)
                    .opacity(style.opacity)
                    .border_style(border)
                    .shadow_style(shadow)
                    .translate(translate_x, translate_y)
                    .transition(transition)
                    .disabled(disabled);
                let bg = if let Some(callback) = on_click {
                    bg.on_click(callback)
                } else {
                    bg
                };
                let bg = if let Some(callback) = on_context_menu {
                    bg.on_context_menu(callback)
                } else {
                    bg
                };
                bg.build();

                ui.row(content_id)
                    .fill()
                    .gap(gap)
                    .justify_content(Align::Center)
                    .align_items(Align::Center)
                    .content(|ui| {
                        if has_icon {
                            ui.text(icon_id)
                                .size(icon_width, Size::fill())
                                .text(icon)
                                .icon_font()
                                .font_size(icon_font)
                                .line_height(icon_font)
                                .color(icon_color)
                                .horizontal_align(HorizontalAlign::Center)
                                .vertical_align(VerticalAlign::Center)
                                .transition(transition)
                                .build();
                        }

                        ui.text(text_id)
                            .size(label_width, Size::fill())
                            .text(text)
                            .font_size(font)
                            .line_height(font)
                            .color(text_color)
                            .horizontal_align(if has_icon {
                                HorizontalAlign::Left
                            } else {
                                HorizontalAlign::Center
                            })
                            .vertical_align(VerticalAlign::Center)
                            .transition(transition)
                            .build();
                    });
            });

        self.ui.response(&bg_id)
    }
}

pub fn button(ui: &mut Ui, id: impl Into<String>) -> ButtonBuilder<'_> {
    ButtonBuilder::new(ui, id)
}

fn utf8(codepoint: u32) -> String {
    char::from_u32(codepoint)
        .map(|value| value.to_string())
        .unwrap_or_default()
}
