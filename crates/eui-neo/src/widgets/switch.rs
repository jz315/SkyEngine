//! Port of `EUI-NEO/components/switch.h`.

use std::cell::RefCell;
use std::rc::Rc;

use crate::Color;

use super::super::{
    AnimProperty, Binding, HorizontalAlign, Response, Transition, Ui, VerticalAlign,
};
use super::text::measure_text_width;
use super::theme::{self, ThemeColorTokens};

type BoolCallback = Rc<RefCell<Box<dyn FnMut(bool)>>>;

#[derive(Debug, Clone, Copy)]
pub struct SwitchStyle {
    pub off: Color,
    pub on: Color,
    pub knob: Color,
    pub text: Color,
    pub row_hover: Color,
    pub row_pressed: Color,
}

impl SwitchStyle {
    pub fn new(tokens: ThemeColorTokens) -> Self {
        Self {
            off: theme::mix_color(tokens.surface_hover, tokens.surface_active, 0.30),
            on: tokens.primary,
            knob: theme::mix_color(tokens.surface, theme::color(1.0, 1.0, 1.0, 1.0), 0.75),
            text: tokens.text,
            row_hover: theme::with_alpha(tokens.text, if tokens.dark { 0.06 } else { 0.05 }),
            row_pressed: theme::with_alpha(tokens.text, if tokens.dark { 0.10 } else { 0.08 }),
        }
    }
}

impl Default for SwitchStyle {
    fn default() -> Self {
        Self::new(theme::dark_theme_colors())
    }
}

pub struct SwitchBuilder<'ui> {
    ui: &'ui mut Ui,
    id: String,
    style: SwitchStyle,
    transition: Transition,
    on_change: Option<BoolCallback>,
    label: String,
    checked: bool,
    width: f32,
    height: f32,
    track_width: f32,
    track_height: f32,
    gap: f32,
    font_size: f32,
}

impl<'ui> SwitchBuilder<'ui> {
    pub fn new(ui: &'ui mut Ui, id: impl Into<String>) -> Self {
        Self {
            ui,
            id: id.into(),
            style: SwitchStyle::default(),
            transition: Transition::smooth(),
            on_change: None,
            label: String::new(),
            checked: false,
            width: 180.0,
            height: 30.0,
            track_width: 46.0,
            track_height: 24.0,
            gap: 10.0,
            font_size: 18.0,
        }
    }

    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    pub fn checked(mut self, value: bool) -> Self {
        self.checked = value;
        self
    }

    pub fn checked_bind<T: 'static>(self, binding: Binding<T, bool>) -> Self {
        let value = binding.get();
        self.checked(value).on_change(move |next| binding.set(next))
    }

    pub fn label(mut self, value: impl Into<String>) -> Self {
        self.label = value.into();
        self
    }

    pub fn text(self, value: impl Into<String>) -> Self {
        self.label(value)
    }

    pub fn font_size(mut self, value: f32) -> Self {
        self.font_size = value.max(1.0);
        self
    }

    pub fn track_size(mut self, width: f32, height: f32) -> Self {
        self.track_width = width.max(20.0);
        self.track_height = height.max(12.0);
        self
    }

    pub fn style(mut self, value: SwitchStyle) -> Self {
        self.style = value;
        self
    }

    pub fn theme(mut self, tokens: ThemeColorTokens) -> Self {
        self.style = SwitchStyle::new(tokens);
        self
    }

    pub fn transition(mut self, value: Transition) -> Self {
        self.transition = value;
        self
    }

    pub fn on_change<F>(mut self, callback: F) -> Self
    where
        F: FnMut(bool) + 'static,
    {
        let next: BoolCallback = Rc::new(RefCell::new(Box::new(callback)));
        self.on_change = Some(if let Some(existing) = self.on_change.take() {
            Rc::new(RefCell::new(Box::new(move |value| {
                (existing.borrow_mut())(value);
                (next.borrow_mut())(value);
            })))
        } else {
            next
        });
        self
    }

    pub fn build(self) -> Response {
        let id = self.id.clone();
        let hit_id = format!("{id}.hit");
        let track_y = (self.height - self.track_height) * 0.5;
        let margin = 2.0_f32.max(self.track_height * 0.125);
        let knob_size = (self.track_height - margin * 2.0).max(0.0);
        let knob_travel = (self.track_width - margin * 2.0 - knob_size).max(0.0);
        let knob_x = margin + if self.checked { knob_travel } else { 0.0 };
        let label_x = self.track_width + self.gap;
        let label_width = (self.width - label_x).max(0.0);
        let label_line_height = self.font_size;
        let label_y = ((self.height - label_line_height) * 0.5).max(0.0);
        let hit_width = if self.label.is_empty() {
            self.track_width
        } else {
            self.width
                .min(label_x + measure_text_width(&self.label, "", self.font_size, 400))
        };
        let next_checked = !self.checked;
        let on_change = self.on_change.clone();
        let label = self.label.clone();

        self.ui
            .stack(id.clone())
            .size(self.width, self.height)
            .content(|ui| {
                ui.rect(hit_id)
                    .size(hit_width, self.height)
                    .states(
                        theme::color(0.0, 0.0, 0.0, 0.0),
                        self.style.row_hover,
                        self.style.row_pressed,
                    )
                    .radius(6.0_f32.max(self.height * 0.20))
                    .transition(self.transition)
                    .on_click(move || {
                        if let Some(callback) = &on_change {
                            (callback.borrow_mut())(next_checked);
                        }
                    })
                    .build();

                ui.rect(format!("{id}.track"))
                    .y(track_y)
                    .size(self.track_width, self.track_height)
                    .color(if self.checked {
                        self.style.on
                    } else {
                        self.style.off
                    })
                    .radius(self.track_height * 0.5)
                    .transition(self.transition)
                    .animate(AnimProperty::COLOR)
                    .build();

                ui.rect(format!("{id}.knob"))
                    .x(knob_x)
                    .y(track_y + margin)
                    .size(knob_size, knob_size)
                    .color(self.style.knob)
                    .radius(knob_size * 0.5)
                    .transition(self.transition)
                    .animate(AnimProperty::FRAME | AnimProperty::COLOR)
                    .build();

                if !label.is_empty() {
                    ui.text(format!("{id}.label"))
                        .x(label_x)
                        .y(label_y)
                        .size(label_width, label_line_height)
                        .text(label)
                        .font_size(self.font_size)
                        .line_height(label_line_height)
                        .color(self.style.text)
                        .horizontal_align(HorizontalAlign::Left)
                        .vertical_align(VerticalAlign::Top)
                        .build();
                }
            });

        self.ui.response(&format!("{id}.hit"))
    }
}

pub fn switch(ui: &mut Ui, id: impl Into<String>) -> SwitchBuilder<'_> {
    SwitchBuilder::new(ui, id)
}
