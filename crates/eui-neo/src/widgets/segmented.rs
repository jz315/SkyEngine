//! Port of `EUI-NEO/components/segmented.h`.

use std::cell::RefCell;
use std::rc::Rc;

use crate::Color;

use super::super::{
    AnimProperty, HorizontalAlign, Response, Signal, Transition, Ui, VerticalAlign,
};
use super::theme::{self, ThemeColorTokens};

type ChangeCallback = Rc<RefCell<Box<dyn FnMut(i32)>>>;

#[derive(Debug, Clone, Copy)]
pub struct SegmentedStyle {
    pub background: Color,
    pub hover: Color,
    pub selected: Color,
    pub text: Color,
    pub selected_text: Color,
    pub border: Color,
}

impl SegmentedStyle {
    pub fn new(tokens: ThemeColorTokens) -> Self {
        Self {
            background: tokens.surface,
            hover: tokens.surface_hover,
            selected: tokens.primary,
            text: tokens.text,
            selected_text: if tokens.dark {
                theme::color(0.96, 0.98, 1.0, 1.0)
            } else {
                theme::color(1.0, 1.0, 1.0, 1.0)
            },
            border: theme::with_opacity(tokens.border, 0.70),
        }
    }
}

impl Default for SegmentedStyle {
    fn default() -> Self {
        Self::new(theme::dark_theme_colors())
    }
}

pub struct SegmentedBuilder<'ui> {
    ui: &'ui mut Ui,
    id: String,
    items: Vec<String>,
    style: SegmentedStyle,
    transition: Transition,
    on_change: Option<ChangeCallback>,
    selected: i32,
    width: f32,
    height: f32,
    font_size: f32,
    radius: f32,
}

impl<'ui> SegmentedBuilder<'ui> {
    pub fn new(ui: &'ui mut Ui, id: impl Into<String>) -> Self {
        Self {
            ui,
            id: id.into(),
            items: Vec::new(),
            style: SegmentedStyle::default(),
            transition: Transition::smooth(),
            on_change: None,
            selected: 0,
            width: 300.0,
            height: 36.0,
            font_size: 16.0,
            radius: 9.0,
        }
    }

    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    pub fn items<I, S>(mut self, value: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.items = value.into_iter().map(Into::into).collect();
        self
    }

    pub fn selected(mut self, value: i32) -> Self {
        self.selected = value;
        self
    }

    pub fn signal<T: 'static>(self, signal: Signal<T, i32>) -> Self {
        let value = signal.watch(self.ui);
        self.selected(value).on_change(move |next| signal.set(next))
    }

    pub fn font_size(mut self, value: f32) -> Self {
        self.font_size = value.max(1.0);
        self
    }

    pub fn style(mut self, value: SegmentedStyle) -> Self {
        self.style = value;
        self
    }

    pub fn theme(mut self, tokens: ThemeColorTokens) -> Self {
        self.style = SegmentedStyle::new(tokens);
        self
    }

    pub fn transition(mut self, value: Transition) -> Self {
        self.transition = value;
        self
    }

    pub fn on_change<F>(mut self, callback: F) -> Self
    where
        F: FnMut(i32) + 'static,
    {
        let next: ChangeCallback = Rc::new(RefCell::new(Box::new(callback)));
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
        let count = self.items.len() as i32;
        let selected = if count > 0 {
            self.selected.clamp(0, count - 1)
        } else {
            0
        };
        let segment_width = if count > 0 {
            self.width / count as f32
        } else {
            self.width
        };
        let inner_inset = 3.0;
        let label_line_height = self.font_size;
        let label_y = ((self.height - label_line_height) * 0.5).max(0.0);
        let on_change = self.on_change.clone();

        self.ui
            .stack(id.clone())
            .size(self.width, self.height)
            .clip()
            .content(|ui| {
                ui.rect(format!("{id}.bg"))
                    .size(self.width, self.height)
                    .color(self.style.background)
                    .radius(self.radius)
                    .border(1.0, self.style.border)
                    .build();

                if count > 0 {
                    ui.rect(format!("{id}.indicator"))
                        .x(segment_width * selected as f32 + inner_inset)
                        .y(inner_inset)
                        .size(
                            (segment_width - inner_inset * 2.0).max(0.0),
                            (self.height - inner_inset * 2.0).max(0.0),
                        )
                        .color(self.style.selected)
                        .radius((self.radius - 2.0).max(0.0))
                        .transition(self.transition)
                        .animate(AnimProperty::FRAME | AnimProperty::COLOR)
                        .build();
                }

                for index in 0..count {
                    let x = index as f32 * segment_width;
                    let active = index == selected;
                    let hit_callback = on_change.clone();
                    ui.rect(format!("{id}.hit.{index}"))
                        .x(x)
                        .size(segment_width, self.height)
                        .states(
                            theme::color(0.0, 0.0, 0.0, 0.0),
                            theme::color(0.0, 0.0, 0.0, 0.0),
                            theme::color(0.0, 0.0, 0.0, 0.0),
                        )
                        .radius(self.radius)
                        .on_click(move || {
                            if let Some(callback) = &hit_callback {
                                (callback.borrow_mut())(index);
                            }
                        })
                        .build();

                    ui.text(format!("{id}.label.{index}"))
                        .x(x)
                        .y(label_y)
                        .size(segment_width, label_line_height)
                        .text(self.items[index as usize].clone())
                        .font_size(self.font_size)
                        .line_height(label_line_height)
                        .color(if active {
                            self.style.selected_text
                        } else {
                            self.style.text
                        })
                        .horizontal_align(HorizontalAlign::Center)
                        .vertical_align(VerticalAlign::Top)
                        .transition(self.transition)
                        .animate(AnimProperty::TEXT_COLOR)
                        .build();
                }
            });

        self.ui.response(&id)
    }
}

pub fn segmented(ui: &mut Ui, id: impl Into<String>) -> SegmentedBuilder<'_> {
    SegmentedBuilder::new(ui, id)
}
