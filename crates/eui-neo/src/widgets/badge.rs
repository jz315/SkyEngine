//! Small badge/chip widget built on the shared neo widget layout contract.

use crate::Color;

use super::super::{HorizontalAlign, Response, Size, Ui, VerticalAlign};
use super::layout::{scale_size, WidgetLayout};
use super::text::measure_text_width;
use super::theme::{self, ThemeColorTokens};

const DEFAULT_HEIGHT: f32 = 34.0;
const HORIZONTAL_PADDING: f32 = 24.0;

#[derive(Debug, Clone, Copy)]
pub struct BadgeStyle {
    pub background: Color,
    pub border: Color,
    pub text: Color,
    pub radius: f32,
    pub font_size: f32,
    pub font_weight: i32,
    pub padding_x: f32,
}

impl BadgeStyle {
    pub fn new(tokens: ThemeColorTokens, accent: Color) -> Self {
        Self {
            background: theme::with_alpha(accent, if tokens.dark { 0.18 } else { 0.10 }),
            border: theme::with_alpha(accent, 0.34),
            text: accent,
            radius: 999.0,
            font_size: 13.0,
            font_weight: 700,
            padding_x: HORIZONTAL_PADDING,
        }
    }
}

impl Default for BadgeStyle {
    fn default() -> Self {
        let tokens = theme::dark_theme_colors();
        Self::new(tokens, tokens.primary)
    }
}

pub struct BadgeBuilder<'ui> {
    ui: &'ui mut Ui,
    id: String,
    text: String,
    style: BadgeStyle,
    layout: WidgetLayout,
}

impl<'ui> BadgeBuilder<'ui> {
    pub fn new(ui: &'ui mut Ui, id: impl Into<String>) -> Self {
        Self {
            ui,
            id: id.into(),
            text: "Badge".to_string(),
            style: BadgeStyle::default(),
            layout: WidgetLayout::new(0.0, DEFAULT_HEIGHT).width(Size::WrapContent),
        }
    }

    pub fn text(mut self, value: impl Into<String>) -> Self {
        self.text = value.into();
        self
    }

    pub fn accent(mut self, color: impl Into<Color>) -> Self {
        let color = color.into();
        self.style.background = theme::with_alpha(color, 0.18);
        self.style.border = theme::with_alpha(color, 0.34);
        self.style.text = color;
        self
    }

    pub fn style(mut self, value: BadgeStyle) -> Self {
        self.style = value;
        self
    }

    pub fn theme(mut self, tokens: ThemeColorTokens, accent: impl Into<Color>) -> Self {
        self.style = BadgeStyle::new(tokens, accent.into());
        self
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

    pub fn marginXY(self, horizontal: f32, vertical: f32) -> Self {
        self.margin_xy(horizontal, vertical)
    }

    pub fn marginEach(self, left: f32, top: f32, right: f32, bottom: f32) -> Self {
        self.margin_each(left, top, right, bottom)
    }

    pub fn minWidth(self, value: f32) -> Self {
        self.min_width(value)
    }

    pub fn maxWidth(self, value: f32) -> Self {
        self.max_width(value)
    }

    pub fn minHeight(self, value: f32) -> Self {
        self.min_height(value)
    }

    pub fn maxHeight(self, value: f32) -> Self {
        self.max_height(value)
    }

    pub fn build(self) -> Response {
        let id = self.id.clone();
        let bg_id = format!("{id}.bg");
        let text_id = format!("{id}.text");
        let height = self.layout.fixed_height_or(DEFAULT_HEIGHT);
        let natural_width =
            measure_text_width(&self.text, "", self.style.font_size, self.style.font_weight)
                + self.style.padding_x.max(0.0);
        let root_width = scale_size(self.layout.width, 1.0, natural_width);
        let root_height = scale_size(self.layout.height, 1.0, DEFAULT_HEIGHT);

        self.layout
            .apply_to_size(self.ui.stack(id), root_width, root_height)
            .content(|ui| {
                ui.rect(bg_id)
                    .fill()
                    .color(self.style.background)
                    .border(1.0, self.style.border)
                    .radius(self.style.radius.min(height * 0.5))
                    .build();
                ui.text(text_id)
                    .fill()
                    .text(self.text)
                    .font_size(self.style.font_size)
                    .font_weight(self.style.font_weight)
                    .line_height(height)
                    .color(self.style.text)
                    .horizontal_align(HorizontalAlign::Center)
                    .vertical_align(VerticalAlign::Center)
                    .build();
            })
    }
}

pub fn badge(ui: &mut Ui, id: impl Into<String>) -> BadgeBuilder<'_> {
    BadgeBuilder::new(ui, id)
}
