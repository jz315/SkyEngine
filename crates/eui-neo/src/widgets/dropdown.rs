//! Port of `EUI-NEO/components/dropdown.h`.

use std::cell::RefCell;
use std::rc::Rc;

use crate::Color;

use super::super::{
    AnimProperty, HorizontalAlign, Response, Shadow, Signal, Transition, Ui, VerticalAlign,
};
use super::popover::{popover, PopoverPlacement};
use super::theme::{self, ThemeColorTokens};

type ChangeCallback = Rc<RefCell<Box<dyn FnMut(i32)>>>;
type OpenChangeCallback = Rc<RefCell<Box<dyn FnMut(bool)>>>;

#[derive(Debug, Clone, Copy)]
pub struct DropdownStyle {
    pub field: Color,
    pub field_hover: Color,
    pub field_pressed: Color,
    pub popup: Color,
    pub option_hover: Color,
    pub option_pressed: Color,
    pub selected: Color,
    pub text: Color,
    pub muted_text: Color,
    pub accent: Color,
    pub border: Color,
    pub shadow: Shadow,
    pub radius: f32,
}

impl DropdownStyle {
    pub fn new(tokens: ThemeColorTokens) -> Self {
        Self {
            field: tokens.surface,
            field_hover: tokens.surface_hover,
            field_pressed: tokens.surface_active,
            popup: if tokens.dark {
                theme::mix_color(tokens.surface, theme::color(0.0, 0.0, 0.0, 1.0), 0.12)
            } else {
                tokens.surface
            },
            option_hover: tokens.surface_hover,
            option_pressed: tokens.surface_active,
            selected: theme::with_alpha(tokens.primary, if tokens.dark { 0.24 } else { 0.14 }),
            text: tokens.text,
            muted_text: theme::with_opacity(tokens.text, 0.60),
            accent: tokens.primary,
            border: theme::with_opacity(tokens.border, 0.78),
            shadow: theme::popup_shadow(tokens),
            radius: 12.0,
        }
    }
}

impl Default for DropdownStyle {
    fn default() -> Self {
        Self::new(theme::dark_theme_colors())
    }
}

pub struct DropdownBuilder<'ui> {
    ui: &'ui mut Ui,
    id: String,
    items: Vec<String>,
    style: DropdownStyle,
    transition: Transition,
    on_change: Option<ChangeCallback>,
    on_open_change: Option<OpenChangeCallback>,
    placeholder: String,
    selected: i32,
    open: bool,
    width: f32,
    height: f32,
    item_height: f32,
    z_index: i32,
}

impl<'ui> DropdownBuilder<'ui> {
    pub fn new(ui: &'ui mut Ui, id: impl Into<String>) -> Self {
        Self {
            ui,
            id: id.into(),
            items: Vec::new(),
            style: DropdownStyle::default(),
            transition: Transition::smooth(),
            on_change: None,
            on_open_change: None,
            placeholder: "Select".to_string(),
            selected: -1,
            open: false,
            width: 260.0,
            height: 42.0,
            item_height: 34.0,
            z_index: 20,
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

    pub fn value_signal<T: 'static>(self, signal: Signal<T, i32>) -> Self {
        let owner = self.id.clone();
        let value = self.ui.with_dependency_owner(owner, |ui| signal.watch(ui));
        self.selected(value).on_change(move |next| signal.set(next))
    }

    pub fn placeholder(mut self, value: impl Into<String>) -> Self {
        self.placeholder = value.into();
        self
    }

    pub fn open(mut self, value: bool) -> Self {
        self.open = value;
        self
    }

    pub fn open_signal<T: 'static>(self, signal: Signal<T, bool>) -> Self {
        let owner = self.id.clone();
        let popup_owner = format!("{owner}.popup");
        let value = self.ui.with_dependency_owner(owner, |ui| signal.watch(ui));
        self.ui
            .with_dependency_owner(popup_owner, |ui| signal.watch(ui));
        self.open(value)
            .on_open_change(move |next| signal.set(next))
    }

    pub fn item_height(mut self, value: f32) -> Self {
        self.item_height = value.max(24.0);
        self
    }

    pub fn style(mut self, value: DropdownStyle) -> Self {
        self.style = value;
        self
    }

    pub fn theme(mut self, tokens: ThemeColorTokens) -> Self {
        self.style = DropdownStyle::new(tokens);
        self
    }

    pub fn transition(mut self, value: Transition) -> Self {
        self.transition = value;
        self
    }

    pub fn z_index(mut self, value: i32) -> Self {
        self.z_index = value;
        self
    }

    pub fn z(self, value: i32) -> Self {
        self.z_index(value)
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

    pub fn on_open_change<F>(mut self, callback: F) -> Self
    where
        F: FnMut(bool) + 'static,
    {
        let next: OpenChangeCallback = Rc::new(RefCell::new(Box::new(callback)));
        self.on_open_change = Some(if let Some(existing) = self.on_open_change.take() {
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
            -1
        };
        let label = if selected >= 0 {
            self.items[selected as usize].clone()
        } else {
            self.placeholder.clone()
        };
        let popup_gap = 8.0;
        let popup_padding = 6.0;
        let popup_height = self.item_height * 1_i32.max(count) as f32 + popup_padding * 2.0;
        let visible = if self.open { 1.0 } else { 0.0 };
        let popup_offset_y = if self.open { 0.0 } else { -6.0 };
        let popup_scale = if self.open { 1.0 } else { 0.96 };
        let on_change = self.on_change.clone();
        let on_open_change = self.on_open_change.clone();

        self.ui
            .stack(id.clone())
            .size(self.width, self.height)
            .z_index(self.z_index)
            .content(|ui| {
                let open_change = on_open_change.clone();
                let open = self.open;
                ui.rect(format!("{id}.field"))
                    .size(self.width, self.height)
                    .states(
                        self.style.field,
                        self.style.field_hover,
                        self.style.field_pressed,
                    )
                    .radius(self.style.radius)
                    .border(1.0, self.style.border)
                    .transition(self.transition)
                    .on_click(move || {
                        if let Some(callback) = &open_change {
                            (callback.borrow_mut())(!open);
                        }
                    })
                    .build();

                ui.text(format!("{id}.label"))
                    .x(14.0)
                    .size((self.width - 48.0).max(0.0), self.height)
                    .text(label)
                    .font_size(16.0)
                    .line_height(20.0)
                    .color(if selected >= 0 {
                        self.style.text
                    } else {
                        self.style.muted_text
                    })
                    .vertical_align(VerticalAlign::Center)
                    .build();

                ui.text(format!("{id}.chevron"))
                    .x((self.width - 34.0).max(0.0))
                    .size(20.0, self.height)
                    .icon_codepoint(if self.open { 0xF077 } else { 0xF078 })
                    .font_size(13.0)
                    .line_height(18.0)
                    .color(self.style.accent)
                    .horizontal_align(HorizontalAlign::Center)
                    .vertical_align(VerticalAlign::Center)
                    .transition(self.transition)
                    .animate(AnimProperty::TEXT_COLOR)
                    .build();
            });

        popover(self.ui, format!("{id}.popup"))
            .open(self.open)
            .anchor(format!("{id}.field"))
            .placement(PopoverPlacement::BottomStart)
            .gap(popup_gap)
            .size(self.width, popup_height)
            .z_index(self.z_index + 1)
            .outside_click(super::super::OutsideClickPolicy::Close)
            .on_dismiss({
                let open_change = on_open_change.clone();
                move || {
                    if let Some(callback) = &open_change {
                        (callback.borrow_mut())(false);
                    }
                }
            })
            .content(|ui| {
                ui.stack(format!("{id}.popup.surface"))
                    .size(self.width, popup_height)
                    .opacity(visible)
                    .translate_y(popup_offset_y)
                    .scale(popup_scale)
                    .transform_origin(0.5, 0.0)
                    .transition(self.transition)
                    .animate(AnimProperty::OPACITY | AnimProperty::TRANSFORM)
                    .content(|ui| {
                        ui.rect(format!("{id}.popup.bg"))
                            .size(self.width, popup_height)
                            .color(self.style.popup)
                            .radius(self.style.radius)
                            .border(1.0, self.style.border)
                            .shadow_style(self.style.shadow)
                            .build();

                        ui.rect(format!("{id}.popup.hit"))
                            .size(self.width, popup_height)
                            .states(
                                theme::color(0.0, 0.0, 0.0, 0.0),
                                theme::color(0.0, 0.0, 0.0, 0.0),
                                theme::color(0.0, 0.0, 0.0, 0.0),
                            )
                            .on_click(|| {})
                            .build();

                        for (index, item) in self.items.iter().enumerate() {
                            let index_i32 = index as i32;
                            let active = index_i32 == selected;
                            let item_y = popup_padding + index as f32 * self.item_height;
                            let change = on_change.clone();
                            let open_change = on_open_change.clone();
                            if active {
                                ui.rect(format!("{id}.item.selected.{index}"))
                                    .x(popup_padding)
                                    .y(item_y)
                                    .size(
                                        (self.width - popup_padding * 2.0).max(0.0),
                                        self.item_height,
                                    )
                                    .color(self.style.selected)
                                    .radius(4.0_f32.max(self.style.radius - 4.0))
                                    .transition(self.transition)
                                    .animate(AnimProperty::COLOR)
                                    .build();
                            }
                            ui.rect(format!("{id}.item.{index}"))
                                .x(popup_padding)
                                .y(item_y)
                                .size(
                                    (self.width - popup_padding * 2.0).max(0.0),
                                    self.item_height,
                                )
                                .states(
                                    theme::color(0.0, 0.0, 0.0, 0.0),
                                    self.style.option_hover,
                                    self.style.option_pressed,
                                )
                                .radius(4.0_f32.max(self.style.radius - 4.0))
                                .instant_states()
                                .on_click(move || {
                                    if let Some(callback) = &change {
                                        (callback.borrow_mut())(index_i32);
                                    }
                                    if let Some(callback) = &open_change {
                                        (callback.borrow_mut())(false);
                                    }
                                })
                                .build();

                            ui.text(format!("{id}.item.label.{index}"))
                                .x(popup_padding + 12.0)
                                .y(item_y + ((self.item_height - 18.0) * 0.5).max(0.0))
                                .size((self.width - popup_padding * 2.0 - 24.0).max(0.0), 20.0)
                                .text(item.clone())
                                .font_size(15.0)
                                .line_height(18.0)
                                .color(if active {
                                    self.style.accent
                                } else {
                                    self.style.text
                                })
                                .transition(self.transition)
                                .animate(AnimProperty::TEXT_COLOR)
                                .build();
                        }
                    });
            });

        self.ui.response(&id)
    }
}

pub fn dropdown(ui: &mut Ui, id: impl Into<String>) -> DropdownBuilder<'_> {
    DropdownBuilder::new(ui, id)
}
