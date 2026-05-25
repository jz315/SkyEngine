//! Port of `EUI-NEO/components/progress.h`.

use crate::Color;

use super::super::{AnimProperty, Response, Transition, Ui};
use super::theme::{self, ThemeColorTokens};

#[derive(Debug, Clone, Copy)]
pub struct ProgressStyle {
    pub track: Color,
    pub fill: Color,
}

impl ProgressStyle {
    pub fn new(tokens: ThemeColorTokens) -> Self {
        Self {
            track: tokens.surface_hover,
            fill: tokens.primary,
        }
    }
}

impl Default for ProgressStyle {
    fn default() -> Self {
        Self::new(theme::dark_theme_colors())
    }
}

pub struct ProgressBuilder<'ui> {
    ui: &'ui mut Ui,
    id: String,
    style: ProgressStyle,
    transition: Transition,
    width: f32,
    height: f32,
    value: f32,
}

impl<'ui> ProgressBuilder<'ui> {
    pub fn new(ui: &'ui mut Ui, id: impl Into<String>) -> Self {
        Self {
            ui,
            id: id.into(),
            style: ProgressStyle::default(),
            transition: Transition::smooth(),
            width: 300.0,
            height: 15.0,
            value: 0.0,
        }
    }

    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    pub fn value(mut self, value: f32) -> Self {
        self.value = value.clamp(0.0, 1.0);
        self
    }

    pub fn style(mut self, value: ProgressStyle) -> Self {
        self.style = value;
        self
    }

    pub fn theme(mut self, tokens: ThemeColorTokens) -> Self {
        self.style = ProgressStyle::new(tokens);
        self
    }

    pub fn transition(mut self, value: Transition) -> Self {
        self.transition = value;
        self
    }

    pub fn build(self) -> Response {
        let id = self.id.clone();
        self.ui
            .stack(id.clone())
            .size(self.width, self.height)
            .content(|ui| {
                ui.rect(format!("{id}.track"))
                    .size(self.width, self.height)
                    .color(self.style.track)
                    .radius(self.height * 0.5)
                    .build();

                ui.rect(format!("{id}.fill"))
                    .size(self.width * self.value, self.height)
                    .color(self.style.fill)
                    .radius(self.height * 0.5)
                    .transition(self.transition)
                    .animate(AnimProperty::FRAME | AnimProperty::COLOR)
                    .build();
            })
    }
}

pub fn progress(ui: &mut Ui, id: impl Into<String>) -> ProgressBuilder<'_> {
    ProgressBuilder::new(ui, id)
}
