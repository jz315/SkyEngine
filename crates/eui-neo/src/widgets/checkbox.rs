//! Port of `EUI-NEO/components/checkbox.h`.

use std::cell::RefCell;
use std::rc::Rc;

use crate::Color;

use super::super::{AnimProperty, Binding, Ease, Response, Transition, Ui, VerticalAlign};
use super::text::measure_text_width;
use super::theme::{self, ThemeColorTokens};

type BoolCallback = Rc<RefCell<Box<dyn FnMut(bool)>>>;

#[derive(Debug, Clone, Copy)]
pub struct CheckboxStyle {
    pub box_color: Color,
    pub box_hover: Color,
    pub box_pressed: Color,
    pub checked: Color,
    pub checked_hover: Color,
    pub checked_pressed: Color,
    pub border: Color,
    pub mark: Color,
    pub text: Color,
    pub row_hover: Color,
    pub row_pressed: Color,
    pub radius: f32,
}

impl CheckboxStyle {
    pub fn new(tokens: ThemeColorTokens) -> Self {
        let checked = tokens.primary;
        let box_hover = tokens.surface_hover;
        Self {
            box_color: tokens.surface,
            box_hover,
            checked,
            checked_hover: theme::button_hover(tokens, checked),
            checked_pressed: theme::button_pressed(tokens, checked),
            box_pressed: theme::button_pressed(tokens, box_hover),
            border: tokens.border,
            mark: theme::color(1.0, 1.0, 1.0, 1.0),
            text: tokens.text,
            row_hover: theme::with_alpha(tokens.text, if tokens.dark { 0.06 } else { 0.05 }),
            row_pressed: theme::with_alpha(tokens.text, if tokens.dark { 0.10 } else { 0.08 }),
            radius: 6.0,
        }
    }
}

impl Default for CheckboxStyle {
    fn default() -> Self {
        Self::new(theme::dark_theme_colors())
    }
}

pub struct CheckboxBuilder<'ui> {
    ui: &'ui mut Ui,
    id: String,
    style: CheckboxStyle,
    transition: Transition,
    on_change: Option<BoolCallback>,
    text: String,
    checked: bool,
    width: f32,
    height: f32,
    box_size: f32,
    gap: f32,
    font_size: f32,
}

impl<'ui> CheckboxBuilder<'ui> {
    pub fn new(ui: &'ui mut Ui, id: impl Into<String>) -> Self {
        Self {
            ui,
            id: id.into(),
            style: CheckboxStyle::default(),
            transition: Transition::make(0.16, Ease::OutCubic),
            on_change: None,
            text: String::new(),
            checked: false,
            width: 180.0,
            height: 28.0,
            box_size: 22.0,
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

    pub fn text(mut self, value: impl Into<String>) -> Self {
        self.text = value.into();
        self
    }

    pub fn font_size(mut self, value: f32) -> Self {
        self.font_size = value.max(1.0);
        self
    }

    pub fn box_size(mut self, value: f32) -> Self {
        self.box_size = value.max(10.0);
        self
    }

    pub fn style(mut self, value: CheckboxStyle) -> Self {
        self.style = value;
        self
    }

    pub fn theme(mut self, tokens: ThemeColorTokens) -> Self {
        self.style = CheckboxStyle::new(tokens);
        self
    }

    pub fn transition(mut self, value: Transition) -> Self {
        self.transition = value;
        self
    }

    pub fn transition_seconds(mut self, duration: f32, ease: Ease) -> Self {
        self.transition = Transition::make(duration, ease);
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

    pub fn fontSize(self, value: f32) -> Self {
        self.font_size(value)
    }

    pub fn boxSize(self, value: f32) -> Self {
        self.box_size(value)
    }

    pub fn checkedBind<T: 'static>(self, binding: Binding<T, bool>) -> Self {
        self.checked_bind(binding)
    }

    pub fn transitionSeconds(self, duration: f32, ease: Ease) -> Self {
        self.transition_seconds(duration, ease)
    }

    pub fn onChange<F>(self, callback: F) -> Self
    where
        F: FnMut(bool) + 'static,
    {
        self.on_change(callback)
    }

    pub fn build(self) -> Response {
        let id = self.id.clone();
        let hit_id = format!("{id}.hit");
        let box_size = self.box_size.min(self.height);
        let box_y = (self.height - box_size) * 0.5;
        let label_x = box_size + self.gap;
        let label_width = (self.width - label_x).max(0.0);
        let label_line_height = self.font_size;
        let label_y = ((self.height - label_line_height) * 0.5).max(0.0);
        let mark_thickness = 2.0_f32.max(box_size * 0.12);
        let mark_angle = 0.78;
        let mark_angle_cos = 0.7109;
        let mark_angle_sin = 0.7033;
        let mark_short = if self.checked { box_size * 0.28 } else { 0.0 };
        let mark_long = if self.checked { box_size * 0.46 } else { 0.0 };
        let mark_overlap = mark_thickness * 0.70;
        let mark_start_x = box_size * 0.26;
        let mark_start_y = box_size * 0.55;
        let mark_corner_x = mark_start_x + box_size * 0.28 * mark_angle_cos;
        let mark_corner_y = mark_start_y + box_size * 0.28 * mark_angle_sin;
        let mark_long_x = mark_corner_x - mark_overlap * mark_angle_cos;
        let mark_long_y = mark_corner_y + mark_overlap * mark_angle_sin;
        let mark_short_transition = if self.checked {
            self.transition
                .duration(0.09)
                .delay(0.0)
                .easing(Ease::OutCubic)
        } else {
            Transition::none()
        };
        let mark_long_transition = if self.checked {
            self.transition
                .duration(0.12)
                .delay(0.08)
                .easing(Ease::OutCubic)
        } else {
            Transition::none()
        };
        let hit_width = if self.text.is_empty() {
            box_size
        } else {
            self.width
                .min(label_x + measure_text_width(&self.text, "", self.font_size, 400))
        };
        let idle = if self.checked {
            self.style.checked
        } else {
            self.style.box_color
        };
        let next_checked = !self.checked;
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
                        if let Some(callback) = &on_change {
                            (callback.borrow_mut())(next_checked);
                        }
                    })
                    .build();

                ui.rect(format!("{id}.box"))
                    .y(box_y)
                    .size(box_size, box_size)
                    .color(idle)
                    .radius(self.style.radius)
                    .border(
                        1.5,
                        if self.checked {
                            self.style.checked
                        } else {
                            self.style.border
                        },
                    )
                    .transition(self.transition)
                    .animate(AnimProperty::COLOR | AnimProperty::BORDER)
                    .build();

                ui.stack(format!("{id}.mark.clip"))
                    .y(box_y)
                    .size(box_size, box_size)
                    .clip()
                    .content(|ui| {
                        ui.rect(format!("{id}.mark.short"))
                            .x(mark_start_x)
                            .y(mark_start_y - mark_thickness * 0.5)
                            .size(mark_short, mark_thickness)
                            .color(self.style.mark)
                            .radius(mark_thickness * 0.5)
                            .opacity(if self.checked { 1.0 } else { 0.0 })
                            .rotate(mark_angle)
                            .transform_origin(0.0, 0.5)
                            .transition(mark_short_transition)
                            .animate(AnimProperty::FRAME | AnimProperty::OPACITY)
                            .build();

                        ui.rect(format!("{id}.mark.long"))
                            .x(mark_long_x)
                            .y(mark_long_y - mark_thickness * 0.5)
                            .size(mark_long + mark_overlap, mark_thickness)
                            .color(self.style.mark)
                            .radius(mark_thickness * 0.5)
                            .opacity(if self.checked { 1.0 } else { 0.0 })
                            .rotate(-mark_angle)
                            .transform_origin(0.0, 0.5)
                            .transition(mark_long_transition)
                            .animate(AnimProperty::FRAME | AnimProperty::OPACITY)
                            .build();
                    });

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

pub fn checkbox(ui: &mut Ui, id: impl Into<String>) -> CheckboxBuilder<'_> {
    CheckboxBuilder::new(ui, id)
}
