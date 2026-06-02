//! Port of `EUI-NEO/components/dialog.h`.

use std::cell::RefCell;
use std::rc::Rc;

use crate::runtime::{LayerId, LayerIntent, LayerKind, LayerPlacement, LayerSize, NodeId};
use crate::Color;

use super::super::{
    AnimProperty, LayoutRect, OutsideClickPolicy, Response, Shadow, Signal, Transition, Ui,
};
use super::button::button;
use super::theme::{self, ThemeColorTokens};

type ClickCallback = Rc<RefCell<Box<dyn FnMut()>>>;

#[derive(Debug, Clone, Copy)]
pub struct DialogStyle {
    pub backdrop: Color,
    pub surface: Color,
    pub border: Color,
    pub title: Color,
    pub message: Color,
    pub primary: Color,
    pub secondary: Color,
    pub primary_hover: Color,
    pub primary_pressed: Color,
    pub secondary_hover: Color,
    pub secondary_pressed: Color,
    pub shadow: Shadow,
    pub radius: f32,
}

impl DialogStyle {
    pub fn new(tokens: ThemeColorTokens) -> Self {
        let primary = tokens.primary;
        let secondary = tokens.surface_hover;
        Self {
            backdrop: theme::color(0.0, 0.0, 0.0, if tokens.dark { 0.46 } else { 0.28 }),
            surface: tokens.surface,
            border: theme::with_opacity(tokens.border, 0.82),
            title: theme::page_visuals(tokens).title_color,
            message: theme::page_visuals(tokens).body_color,
            primary,
            secondary,
            primary_hover: theme::button_hover(tokens, primary),
            primary_pressed: theme::button_pressed(tokens, primary),
            secondary_hover: theme::button_hover(tokens, secondary),
            secondary_pressed: theme::button_pressed(tokens, secondary),
            shadow: theme::panel_shadow(tokens),
            radius: 18.0,
        }
    }
}

impl Default for DialogStyle {
    fn default() -> Self {
        Self::new(theme::dark_theme_colors())
    }
}

pub struct DialogBuilder<'ui> {
    ui: &'ui mut Ui,
    id: String,
    style: DialogStyle,
    transition: Transition,
    on_primary: Option<ClickCallback>,
    on_secondary: Option<ClickCallback>,
    on_close: Option<ClickCallback>,
    title: String,
    message: String,
    primary_text: String,
    secondary_text: String,
    open: bool,
    screen_width: f32,
    screen_height: f32,
    width: f32,
    height: f32,
    z_index: i32,
}

impl<'ui> DialogBuilder<'ui> {
    pub fn new(ui: &'ui mut Ui, id: impl Into<String>) -> Self {
        Self {
            ui,
            id: id.into(),
            style: DialogStyle::default(),
            transition: Transition::smooth(),
            on_primary: None,
            on_secondary: None,
            on_close: None,
            title: "Dialog".to_string(),
            message: "Use dialogs for focused confirmation or short blocking workflows."
                .to_string(),
            primary_text: "Confirm".to_string(),
            secondary_text: "Cancel".to_string(),
            open: false,
            screen_width: 800.0,
            screen_height: 600.0,
            width: 420.0,
            height: 220.0,
            z_index: 1000,
        }
    }

    pub fn open(mut self, value: bool) -> Self {
        self.open = value;
        self
    }

    pub fn open_signal<T: 'static>(self, signal: Signal<T, bool>) -> Self {
        let owner = self.id.clone();
        let value = self.ui.with_dependency_owner(owner, |ui| signal.watch(ui));
        self.open(value)
    }

    pub fn screen(mut self, width: f32, height: f32) -> Self {
        self.screen_width = width;
        self.screen_height = height;
        self
    }

    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    pub fn title(mut self, value: impl Into<String>) -> Self {
        self.title = value.into();
        self
    }

    pub fn message(mut self, value: impl Into<String>) -> Self {
        self.message = value.into();
        self
    }

    pub fn primary_text(mut self, value: impl Into<String>) -> Self {
        self.primary_text = value.into();
        self
    }

    pub fn secondary_text(mut self, value: impl Into<String>) -> Self {
        self.secondary_text = value.into();
        self
    }

    pub fn style(mut self, value: DialogStyle) -> Self {
        self.style = value;
        self
    }

    pub fn theme(mut self, tokens: ThemeColorTokens) -> Self {
        self.style = DialogStyle::new(tokens);
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

    pub fn on_primary<F>(mut self, callback: F) -> Self
    where
        F: FnMut() + 'static,
    {
        self.on_primary = Some(Rc::new(RefCell::new(Box::new(callback))));
        self
    }

    pub fn on_secondary<F>(mut self, callback: F) -> Self
    where
        F: FnMut() + 'static,
    {
        self.on_secondary = Some(Rc::new(RefCell::new(Box::new(callback))));
        self
    }

    pub fn on_close<F>(mut self, callback: F) -> Self
    where
        F: FnMut() + 'static,
    {
        self.on_close = Some(Rc::new(RefCell::new(Box::new(callback))));
        self
    }

    pub fn build(self) -> Response {
        let id = self.id.clone();
        let width = self.width.min((self.screen_width - 48.0).max(0.0));
        let height = self.height.min((self.screen_height - 48.0).max(0.0));
        let x = 24.0_f32.max((self.screen_width - width) * 0.5);
        let y = 24.0_f32.max((self.screen_height - height) * 0.5);
        let content_width = (width - 48.0).max(0.0);
        let button_width = 96.0_f32.max(150.0_f32.min((content_width - 12.0) * 0.5));
        let button_row_width = button_width * 2.0 + 12.0;
        let visible = if self.open { 1.0 } else { 0.0 };
        let panel_scale = if self.open { 1.0 } else { 0.965 };
        let panel_offset_y = if self.open { 0.0 } else { 14.0 };
        let on_close = self.on_close.clone();
        let on_primary = self.on_primary.clone();
        let on_secondary = self.on_secondary.clone().or_else(|| self.on_close.clone());
        let panel_id = format!("{id}.panel");
        let resolved_panel_id = self.ui.resolve_id(&panel_id);
        let outside_click = if on_close.is_some() {
            OutsideClickPolicy::Close
        } else {
            OutsideClickPolicy::Block
        };
        let layer_id = LayerId::new(resolved_panel_id.clone());

        self.ui.register_layer_intent(LayerIntent {
            id: layer_id.clone(),
            owner: NodeId::new(self.ui.resolve_id(&id)),
            root: NodeId::new(resolved_panel_id.clone()),
            anchor: None,
            fallback_anchor: Some(LayoutRect::new(
                0.0,
                0.0,
                self.screen_width,
                self.screen_height,
            )),
            boundary: None,
            open: self.open,
            kind: LayerKind::Modal,
            placement: LayerPlacement::Center,
            size: LayerSize::new(width.into(), height.into()),
            gap: 0.0,
            offset: [0.0, 0.0],
            collision: Default::default(),
            z_index: self.z_index + 1,
            outside_click,
        });
        if let Some(layer_close) = on_close.clone() {
            self.ui.register_on_layer_dismiss(
                layer_id,
                Box::new(move || {
                    (layer_close.borrow_mut())();
                }),
            );
        }

        self.ui.with_root_layer(|ui| {
            ui.rect(format!("{id}.backdrop"))
                .size(self.screen_width, self.screen_height)
                .z_index(self.z_index)
                .states(
                    self.style.backdrop,
                    self.style.backdrop,
                    self.style.backdrop,
                )
                .opacity(visible)
                .transition(self.transition)
                .animate(AnimProperty::OPACITY)
                .disabled(!self.open)
                .on_scroll(|_| {})
                .build();
        });

        self.ui.with_root_layer(|ui| {
            ui.stack(panel_id)
                .x(x)
                .y(y)
                .size(width, height)
                .z_index(self.z_index + 1)
                .opacity(visible)
                .translate_y(panel_offset_y)
                .scale(panel_scale)
                .transform_origin(0.5, 0.5)
                .transition(self.transition)
                .animate(AnimProperty::OPACITY | AnimProperty::TRANSFORM)
                .content(|ui| {
                    ui.rect(format!("{id}.panel.bg"))
                        .size(width, height)
                        .color(self.style.surface)
                        .radius(self.style.radius)
                        .border(1.0, self.style.border)
                        .shadow_style(self.style.shadow)
                        .build();

                    ui.rect(format!("{id}.panel.hit"))
                        .size(width, height)
                        .states(
                            theme::color(0.0, 0.0, 0.0, 0.0),
                            theme::color(0.0, 0.0, 0.0, 0.0),
                            theme::color(0.0, 0.0, 0.0, 0.0),
                        )
                        .disabled(!self.open)
                        .on_click(|| {})
                        .build();

                    ui.text(format!("{id}.title"))
                        .x(24.0)
                        .y(22.0)
                        .size(content_width, 32.0)
                        .text(self.title.clone())
                        .font_size(24.0)
                        .line_height(30.0)
                        .color(self.style.title)
                        .build();

                    ui.text(format!("{id}.message"))
                        .x(24.0)
                        .y(64.0)
                        .size(content_width, (height - 138.0).max(0.0))
                        .text(self.message.clone())
                        .font_size(17.0)
                        .line_height(24.0)
                        .max_width(content_width)
                        .wrap(true)
                        .color(self.style.message)
                        .build();

                    ui.row(format!("{id}.actions"))
                        .x(24.0_f32.max(width - button_row_width - 24.0))
                        .y(88.0_f32.max(height - 58.0))
                        .size(button_row_width, 42.0)
                        .gap(12.0)
                        .content(|ui| {
                            let secondary = on_secondary.clone();
                            button(ui, format!("{id}.secondary"))
                                .size(button_width, 42.0)
                                .text(self.secondary_text.clone())
                                .font_size(16.0)
                                .colors(
                                    self.style.secondary,
                                    self.style.secondary_hover,
                                    self.style.secondary_pressed,
                                )
                                .text_color(self.style.title)
                                .icon_color(self.style.title)
                                .radius(10.0)
                                .border(1.0, self.style.border)
                                .shadow(0.0, 0.0, 0.0, theme::color(0.0, 0.0, 0.0, 0.0))
                                .disabled(!self.open)
                                .on_click(move || call_click(&secondary))
                                .build();

                            let primary = on_primary.clone();
                            button(ui, format!("{id}.primary"))
                                .size(button_width, 42.0)
                                .text(self.primary_text.clone())
                                .font_size(16.0)
                                .colors(
                                    self.style.primary,
                                    self.style.primary_hover,
                                    self.style.primary_pressed,
                                )
                                .radius(10.0)
                                .border(1.0, theme::with_alpha(self.style.primary, 0.64))
                                .shadow(10.0, 0.0, 3.0, theme::with_alpha(self.style.primary, 0.18))
                                .disabled(!self.open)
                                .on_click(move || call_click(&primary))
                                .build();
                        });
                });
        });

        self.ui.response(&id)
    }
}

pub fn dialog(ui: &mut Ui, id: impl Into<String>) -> DialogBuilder<'_> {
    DialogBuilder::new(ui, id)
}

fn call_click(callback: &Option<ClickCallback>) {
    if let Some(callback) = callback {
        (callback.borrow_mut())();
    }
}
