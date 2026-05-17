//! Port of `EUI-NEO/components/tabs.h`.

use std::cell::RefCell;
use std::rc::Rc;

use crate::render::Color;

use super::super::{
    AnimProperty, Binding, Ease, HorizontalAlign, Response, Transition, Ui, VerticalAlign,
};
use super::theme::{self, ThemeColorTokens};

type ChangeCallback = Rc<RefCell<Box<dyn FnMut(i32)>>>;

#[derive(Debug, Clone, Copy)]
pub struct TabsStyle {
    pub text: Color,
    pub hover: Color,
    pub selected_text: Color,
    pub indicator: Color,
    pub border: Color,
}

impl TabsStyle {
    pub fn new(tokens: ThemeColorTokens) -> Self {
        Self {
            text: theme::with_opacity(tokens.text, 0.66),
            hover: tokens.surface_hover,
            selected_text: tokens.primary,
            indicator: tokens.primary,
            border: theme::with_opacity(tokens.border, 0.70),
        }
    }
}

impl Default for TabsStyle {
    fn default() -> Self {
        Self::new(theme::dark_theme_colors())
    }
}

pub struct TabsBuilder<'ui> {
    ui: &'ui mut Ui,
    id: String,
    items: Vec<String>,
    style: TabsStyle,
    transition: Transition,
    on_change: Option<ChangeCallback>,
    selected: i32,
    width: f32,
    height: f32,
    font_size: f32,
}

impl<'ui> TabsBuilder<'ui> {
    pub fn new(ui: &'ui mut Ui, id: impl Into<String>) -> Self {
        Self {
            ui,
            id: id.into(),
            items: Vec::new(),
            style: TabsStyle::default(),
            transition: Transition::make(0.16, Ease::OutCubic),
            on_change: None,
            selected: 0,
            width: 360.0,
            height: 42.0,
            font_size: 17.0,
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

    pub fn selected_bind<T: 'static>(self, binding: Binding<T, i32>) -> Self {
        let value = binding.get();
        self.selected(value)
            .on_change(move |next| binding.set(next))
    }

    pub fn font_size(mut self, value: f32) -> Self {
        self.font_size = value.max(1.0);
        self
    }

    pub fn style(mut self, value: TabsStyle) -> Self {
        self.style = value;
        self
    }

    pub fn theme(mut self, tokens: ThemeColorTokens) -> Self {
        self.style = TabsStyle::new(tokens);
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

    pub fn fontSize(self, value: f32) -> Self {
        self.font_size(value)
    }

    pub fn selectedBind<T: 'static>(self, binding: Binding<T, i32>) -> Self {
        self.selected_bind(binding)
    }

    pub fn transitionSeconds(self, duration: f32, ease: Ease) -> Self {
        self.transition_seconds(duration, ease)
    }

    pub fn onChange<F>(self, callback: F) -> Self
    where
        F: FnMut(i32) + 'static,
    {
        self.on_change(callback)
    }

    pub fn build(self) -> Response {
        let id = self.id.clone();
        let count = self.items.len() as i32;
        let selected = if count > 0 {
            self.selected.clamp(0, count - 1)
        } else {
            0
        };
        let tab_width = if count > 0 {
            self.width / count as f32
        } else {
            self.width
        };
        let label_line_height = self.font_size;
        let label_y = ((self.height - label_line_height) * 0.5).max(0.0) - 2.0;
        let on_change = self.on_change.clone();

        self.ui
            .stack(id.clone())
            .size(self.width, self.height)
            .content(|ui| {
                ui.rect(format!("{id}.line"))
                    .y((self.height - 1.0).max(0.0))
                    .size(self.width, 1.0)
                    .color(self.style.border)
                    .build();

                for index in 0..count {
                    let x = index as f32 * tab_width;
                    let active = index == selected;
                    let hit_callback = on_change.clone();

                    ui.rect(format!("{id}.hit.{index}"))
                        .x(x)
                        .size(tab_width, self.height)
                        .states(
                            theme::color(0.0, 0.0, 0.0, 0.0),
                            theme::color(0.0, 0.0, 0.0, 0.0),
                            theme::color(0.0, 0.0, 0.0, 0.0),
                        )
                        .radius(8.0)
                        .on_click(move || {
                            if let Some(callback) = &hit_callback {
                                (callback.borrow_mut())(index);
                            }
                        })
                        .build();

                    ui.text(format!("{id}.label.{index}"))
                        .x(x)
                        .y(label_y)
                        .size(tab_width, label_line_height)
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

                if count > 0 {
                    ui.rect(format!("{id}.indicator"))
                        .x(tab_width * selected as f32 + 10.0)
                        .y((self.height - 3.0).max(0.0))
                        .size((tab_width - 20.0).max(0.0), 3.0)
                        .color(self.style.indicator)
                        .radius(1.5)
                        .transition(self.transition)
                        .animate(AnimProperty::FRAME | AnimProperty::COLOR)
                        .build();
                }
            });

        self.ui.response(&id)
    }
}

pub fn tabs(ui: &mut Ui, id: impl Into<String>) -> TabsBuilder<'_> {
    TabsBuilder::new(ui, id)
}
