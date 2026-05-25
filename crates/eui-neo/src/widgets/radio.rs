//! Port of `EUI-NEO/components/radio.h`.

use std::cell::RefCell;
use std::rc::Rc;

use crate::Color;

use super::super::{AnimProperty, Binding, Response, Transition, Ui, VerticalAlign};
use super::text::measure_text_width;
use super::theme::{self, ThemeColorTokens};

type SelectCallback = Rc<RefCell<Box<dyn FnMut()>>>;
type BoolCallback = Rc<RefCell<Box<dyn FnMut(bool)>>>;

#[derive(Debug, Clone, Copy)]
pub struct RadioStyle {
    pub outer: Color,
    pub outer_hover: Color,
    pub selected: Color,
    pub border: Color,
    pub text: Color,
    pub row_hover: Color,
    pub row_pressed: Color,
}

impl RadioStyle {
    pub fn new(tokens: ThemeColorTokens) -> Self {
        Self {
            outer: tokens.surface,
            outer_hover: tokens.surface_hover,
            selected: tokens.primary,
            border: tokens.border,
            text: tokens.text,
            row_hover: theme::with_alpha(tokens.text, if tokens.dark { 0.06 } else { 0.05 }),
            row_pressed: theme::with_alpha(tokens.text, if tokens.dark { 0.10 } else { 0.08 }),
        }
    }
}

impl Default for RadioStyle {
    fn default() -> Self {
        Self::new(theme::dark_theme_colors())
    }
}

pub struct RadioBuilder<'ui> {
    ui: &'ui mut Ui,
    id: String,
    style: RadioStyle,
    transition: Transition,
    on_select: Option<SelectCallback>,
    on_change: Option<BoolCallback>,
    text: String,
    selected: bool,
    width: f32,
    height: f32,
    dot_size: f32,
    gap: f32,
    font_size: f32,
}

impl<'ui> RadioBuilder<'ui> {
    pub fn new(ui: &'ui mut Ui, id: impl Into<String>) -> Self {
        Self {
            ui,
            id: id.into(),
            style: RadioStyle::default(),
            transition: Transition::snappy(),
            on_select: None,
            on_change: None,
            text: String::new(),
            selected: false,
            width: 180.0,
            height: 28.0,
            dot_size: 22.0,
            gap: 10.0,
            font_size: 18.0,
        }
    }

    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    pub fn selected(mut self, value: bool) -> Self {
        self.selected = value;
        self
    }

    pub fn selected_bind<T: 'static>(self, binding: Binding<T, bool>) -> Self {
        let selected = binding.get();
        self.selected(selected)
            .on_change(move |next| binding.set(next))
    }

    pub fn checked(self, value: bool) -> Self {
        self.selected(value)
    }

    pub fn checked_bind<T: 'static>(self, binding: Binding<T, bool>) -> Self {
        self.selected_bind(binding)
    }

    pub fn text(mut self, value: impl Into<String>) -> Self {
        self.text = value.into();
        self
    }

    pub fn font_size(mut self, value: f32) -> Self {
        self.font_size = value.max(1.0);
        self
    }

    pub fn dot_size(mut self, value: f32) -> Self {
        self.dot_size = value.max(10.0);
        self
    }

    pub fn style(mut self, value: RadioStyle) -> Self {
        self.style = value;
        self
    }

    pub fn theme(mut self, tokens: ThemeColorTokens) -> Self {
        self.style = RadioStyle::new(tokens);
        self
    }

    pub fn transition(mut self, value: Transition) -> Self {
        self.transition = value;
        self
    }

    pub fn on_select<F>(mut self, callback: F) -> Self
    where
        F: FnMut() + 'static,
    {
        let next: SelectCallback = Rc::new(RefCell::new(Box::new(callback)));
        self.on_select = Some(if let Some(existing) = self.on_select.take() {
            Rc::new(RefCell::new(Box::new(move || {
                (existing.borrow_mut())();
                (next.borrow_mut())();
            })))
        } else {
            next
        });
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
        let outer = self.dot_size.min(self.height);
        let inner = outer * 0.48;
        let visible_inner = if self.selected { inner } else { 0.0 };
        let outer_y = (self.height - outer) * 0.5;
        let inner_offset = (outer - visible_inner) * 0.5;
        let label_x = outer + self.gap;
        let label_width = (self.width - label_x).max(0.0);
        let label_line_height = self.font_size;
        let label_y = ((self.height - label_line_height) * 0.5).max(0.0);
        let hit_width = if self.text.is_empty() {
            outer
        } else {
            self.width
                .min(label_x + measure_text_width(&self.text, "", self.font_size, 400))
        };
        let dot_transition = if self.selected {
            Transition::responsive()
        } else {
            Transition::spring(0.14, 1.0)
        };
        let on_select = self.on_select.clone();
        let on_change = self.on_change.clone();
        let text = self.text.clone();

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
                        if let Some(callback) = &on_select {
                            (callback.borrow_mut())();
                        }
                        if let Some(callback) = &on_change {
                            (callback.borrow_mut())(true);
                        }
                    })
                    .build();

                ui.rect(format!("{id}.outer"))
                    .y(outer_y)
                    .size(outer, outer)
                    .color(if self.selected {
                        theme::with_alpha(self.style.selected, 0.18)
                    } else {
                        self.style.outer
                    })
                    .radius(outer * 0.5)
                    .border(
                        1.5,
                        if self.selected {
                            self.style.selected
                        } else {
                            self.style.border
                        },
                    )
                    .transition(self.transition)
                    .animate(AnimProperty::COLOR | AnimProperty::BORDER)
                    .build();

                ui.rect(format!("{id}.inner"))
                    .x(inner_offset)
                    .y(outer_y + inner_offset)
                    .size(visible_inner, visible_inner)
                    .color(self.style.selected)
                    .radius(visible_inner * 0.5)
                    .opacity(if self.selected { 1.0 } else { 0.0 })
                    .transition(dot_transition)
                    .animate(AnimProperty::FRAME | AnimProperty::RADIUS | AnimProperty::OPACITY)
                    .build();

                if !text.is_empty() {
                    ui.text(format!("{id}.label"))
                        .x(label_x)
                        .y(label_y)
                        .size(label_width, label_line_height)
                        .text(text)
                        .font_size(self.font_size)
                        .line_height(label_line_height)
                        .color(self.style.text)
                        .vertical_align(VerticalAlign::Top)
                        .build();
                }
            });

        self.ui.response(&format!("{id}.hit"))
    }
}

pub fn radio(ui: &mut Ui, id: impl Into<String>) -> RadioBuilder<'_> {
    RadioBuilder::new(ui, id)
}
