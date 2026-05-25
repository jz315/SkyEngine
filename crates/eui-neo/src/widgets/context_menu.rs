//! Port of `EUI-NEO/components/contextmenu.h`.

use std::cell::RefCell;
use std::rc::Rc;

use crate::Color;

use super::super::{AnimProperty, Binding, Response, Shadow, Transition, Ui};
use super::theme::{self, ThemeColorTokens};

type SelectCallback = Rc<RefCell<Box<dyn FnMut(i32)>>>;
type DismissCallback = Rc<RefCell<Box<dyn FnMut()>>>;

#[derive(Debug, Clone, Copy)]
pub struct ContextMenuStyle {
    pub background: Color,
    pub hover: Color,
    pub pressed: Color,
    pub text: Color,
    pub muted_text: Color,
    pub border: Color,
    pub shadow: Shadow,
    pub radius: f32,
}

impl ContextMenuStyle {
    pub fn new(tokens: ThemeColorTokens) -> Self {
        Self {
            background: if tokens.dark {
                theme::mix_color(tokens.surface, theme::color(0.0, 0.0, 0.0, 1.0), 0.16)
            } else {
                tokens.surface
            },
            hover: tokens.surface_hover,
            pressed: tokens.surface_active,
            text: tokens.text,
            muted_text: theme::with_opacity(tokens.text, 0.54),
            border: theme::with_opacity(tokens.border, 0.82),
            shadow: theme::popup_shadow(tokens),
            radius: 12.0,
        }
    }
}

impl Default for ContextMenuStyle {
    fn default() -> Self {
        Self::new(theme::dark_theme_colors())
    }
}

pub struct ContextMenuBuilder<'ui> {
    ui: &'ui mut Ui,
    id: String,
    items: Vec<String>,
    style: ContextMenuStyle,
    transition: Transition,
    on_select: Option<SelectCallback>,
    on_dismiss: Option<DismissCallback>,
    open: bool,
    screen_width: f32,
    screen_height: f32,
    x: f32,
    y: f32,
    width: f32,
    item_height: f32,
    z_index: i32,
}

impl<'ui> ContextMenuBuilder<'ui> {
    pub fn new(ui: &'ui mut Ui, id: impl Into<String>) -> Self {
        Self {
            ui,
            id: id.into(),
            items: Vec::new(),
            style: ContextMenuStyle::default(),
            transition: Transition::responsive(),
            on_select: None,
            on_dismiss: None,
            open: false,
            screen_width: 800.0,
            screen_height: 600.0,
            x: 0.0,
            y: 0.0,
            width: 190.0,
            item_height: 36.0,
            z_index: 1050,
        }
    }

    pub fn open(mut self, value: bool) -> Self {
        self.open = value;
        self
    }

    pub fn open_bind<T: 'static>(self, binding: Binding<T, bool>) -> Self {
        self.open(binding.get())
    }

    pub fn screen(mut self, width: f32, height: f32) -> Self {
        self.screen_width = width;
        self.screen_height = height;
        self
    }

    pub fn position(mut self, x: f32, y: f32) -> Self {
        self.x = x;
        self.y = y;
        self
    }

    pub fn size(mut self, width: f32, item_height: f32) -> Self {
        self.width = width;
        self.item_height = item_height;
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

    pub fn style(mut self, value: ContextMenuStyle) -> Self {
        self.style = value;
        self
    }

    pub fn theme(mut self, tokens: ThemeColorTokens) -> Self {
        self.style = ContextMenuStyle::new(tokens);
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

    pub fn on_select<F>(mut self, callback: F) -> Self
    where
        F: FnMut(i32) + 'static,
    {
        self.on_select = Some(Rc::new(RefCell::new(Box::new(callback))));
        self
    }

    pub fn on_dismiss<F>(mut self, callback: F) -> Self
    where
        F: FnMut() + 'static,
    {
        self.on_dismiss = Some(Rc::new(RefCell::new(Box::new(callback))));
        self
    }

    pub fn build(self) -> Response {
        if self.items.is_empty() {
            return Response::default();
        }

        let id = self.id.clone();
        let inset = 6.0;
        let width = self.width.min((self.screen_width - 16.0).max(0.0));
        let height = self.item_height * self.items.len() as f32 + inset * 2.0;
        let x = self
            .x
            .clamp(8.0, 8.0_f32.max(self.screen_width - width - 8.0));
        let y = self
            .y
            .clamp(8.0, 8.0_f32.max(self.screen_height - height - 8.0));
        let visible = if self.open { 1.0 } else { 0.0 };
        let menu_scale = if self.open { 1.0 } else { 0.94 };
        let menu_offset_y = if self.open { 0.0 } else { -4.0 };
        let on_dismiss = self.on_dismiss.clone();
        let on_select = self.on_select.clone();

        self.ui
            .stack(id.clone())
            .size(self.screen_width, self.screen_height)
            .z_index(self.z_index)
            .content(|ui| {
                let dismiss = on_dismiss.clone();
                ui.rect(format!("{id}.dismiss"))
                    .size(self.screen_width, self.screen_height)
                    .states(
                        theme::color(0.0, 0.0, 0.0, 0.0),
                        theme::color(0.0, 0.0, 0.0, 0.0),
                        theme::color(0.0, 0.0, 0.0, 0.0),
                    )
                    .disabled(!self.open)
                    .on_click(move || call_dismiss(&dismiss))
                    .on_scroll(|_| {})
                    .build();

                ui.stack(format!("{id}.menu"))
                    .x(x)
                    .y(y)
                    .size(width, height)
                    .opacity(visible)
                    .translate_y(menu_offset_y)
                    .scale(menu_scale)
                    .transform_origin(0.0, 0.0)
                    .transition(self.transition)
                    .animate(AnimProperty::OPACITY | AnimProperty::TRANSFORM)
                    .content(|ui| {
                        ui.rect(format!("{id}.bg"))
                            .size(width, height)
                            .color(self.style.background)
                            .radius(self.style.radius)
                            .border(1.0, self.style.border)
                            .shadow_style(self.style.shadow)
                            .build();

                        ui.rect(format!("{id}.hit"))
                            .size(width, height)
                            .states(
                                theme::color(0.0, 0.0, 0.0, 0.0),
                                theme::color(0.0, 0.0, 0.0, 0.0),
                                theme::color(0.0, 0.0, 0.0, 0.0),
                            )
                            .disabled(!self.open)
                            .on_click(|| {})
                            .build();

                        for (index, item) in self.items.iter().enumerate() {
                            let index_i32 = index as i32;
                            let item_y = inset + index as f32 * self.item_height;
                            let select = on_select.clone();
                            ui.rect(format!("{id}.item.{index}"))
                                .x(inset)
                                .y(item_y)
                                .size((width - inset * 2.0).max(0.0), self.item_height)
                                .states(
                                    theme::color(0.0, 0.0, 0.0, 0.0),
                                    self.style.hover,
                                    self.style.pressed,
                                )
                                .radius(4.0_f32.max(self.style.radius - 4.0))
                                .instant_states()
                                .disabled(!self.open)
                                .on_click(move || {
                                    if let Some(callback) = &select {
                                        (callback.borrow_mut())(index_i32);
                                    }
                                })
                                .build();

                            ui.text(format!("{id}.label.{index}"))
                                .x(inset + 12.0)
                                .y(item_y + ((self.item_height - 18.0) * 0.5).max(0.0))
                                .size((width - inset * 2.0 - 24.0).max(0.0), 20.0)
                                .text(item.clone())
                                .font_size(15.0)
                                .line_height(18.0)
                                .color(if index == self.items.len() - 1 {
                                    self.style.muted_text
                                } else {
                                    self.style.text
                                })
                                .build();
                        }
                    });
            });

        self.ui.response(&id)
    }
}

pub fn context_menu(ui: &mut Ui, id: impl Into<String>) -> ContextMenuBuilder<'_> {
    ContextMenuBuilder::new(ui, id)
}

fn call_dismiss(callback: &Option<DismissCallback>) {
    if let Some(callback) = callback {
        (callback.borrow_mut())();
    }
}
