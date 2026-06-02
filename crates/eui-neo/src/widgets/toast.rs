//! Port of `EUI-NEO/components/toast.h`.

use std::cell::RefCell;
use std::rc::Rc;

use crate::runtime::{LayerId, LayerIntent, LayerKind, LayerPlacement, LayerSize, NodeId};
use crate::Color;

use super::super::{
    AnimProperty, HorizontalAlign, LayoutRect, OutsideClickPolicy, Response, Shadow, Signal,
    Transition, Ui, VerticalAlign,
};
use super::theme::{self, ThemeColorTokens};

type ClickCallback = Rc<RefCell<Box<dyn FnMut()>>>;

#[derive(Debug, Clone, Copy)]
pub struct ToastStyle {
    pub background: Color,
    pub border: Color,
    pub text: Color,
    pub muted_text: Color,
    pub accent: Color,
    pub shadow: Shadow,
    pub radius: f32,
}

impl ToastStyle {
    pub fn new(tokens: ThemeColorTokens) -> Self {
        Self {
            background: if tokens.dark {
                theme::mix_color(tokens.surface, theme::color(0.0, 0.0, 0.0, 1.0), 0.18)
            } else {
                tokens.surface
            },
            border: theme::with_opacity(tokens.border, 0.82),
            text: tokens.text,
            muted_text: theme::with_opacity(tokens.text, 0.68),
            accent: tokens.primary,
            shadow: theme::popup_shadow(tokens),
            radius: 14.0,
        }
    }
}

impl Default for ToastStyle {
    fn default() -> Self {
        Self::new(theme::dark_theme_colors())
    }
}

pub struct ToastBuilder<'ui> {
    ui: &'ui mut Ui,
    id: String,
    style: ToastStyle,
    transition: Transition,
    on_dismiss: Option<ClickCallback>,
    on_auto_dismiss: Option<ClickCallback>,
    title: String,
    message: String,
    icon: String,
    visible: bool,
    screen_width: f32,
    screen_height: f32,
    width: f32,
    height: f32,
    auto_dismiss_seconds: f32,
    z_index: i32,
}

impl<'ui> ToastBuilder<'ui> {
    pub fn new(ui: &'ui mut Ui, id: impl Into<String>) -> Self {
        Self {
            ui,
            id: id.into(),
            style: ToastStyle::default(),
            transition: Transition::smooth(),
            on_dismiss: None,
            on_auto_dismiss: None,
            title: "Toast".to_string(),
            message: "Short status message for non-blocking feedback.".to_string(),
            icon: utf8(0xF058),
            visible: false,
            screen_width: 800.0,
            screen_height: 600.0,
            width: 360.0,
            height: 88.0,
            auto_dismiss_seconds: 0.0,
            z_index: 1100,
        }
    }

    pub fn visible(mut self, value: bool) -> Self {
        self.visible = value;
        self
    }

    pub fn visible_signal<T: 'static>(self, signal: Signal<T, bool>) -> Self {
        let owner = self.id.clone();
        let value = self.ui.with_dependency_owner(owner, |ui| signal.watch(ui));
        let dismiss_signal = signal.clone();
        self.visible(value)
            .on_dismiss(move || dismiss_signal.set(false))
            .on_auto_dismiss(move || signal.set(false))
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

    pub fn icon(mut self, value: impl Into<String>) -> Self {
        self.icon = value.into();
        self
    }

    pub fn icon_codepoint(mut self, codepoint: u32) -> Self {
        self.icon = utf8(codepoint);
        self
    }

    pub fn style(mut self, value: ToastStyle) -> Self {
        self.style = value;
        self
    }

    pub fn theme(mut self, tokens: ThemeColorTokens) -> Self {
        self.style = ToastStyle::new(tokens);
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

    pub fn duration(mut self, seconds: f32) -> Self {
        self.auto_dismiss_seconds = seconds.max(0.0);
        self
    }

    pub fn auto_dismiss(self, seconds: f32) -> Self {
        self.duration(seconds)
    }

    pub fn on_auto_dismiss<F>(mut self, callback: F) -> Self
    where
        F: FnMut() + 'static,
    {
        let next: ClickCallback = Rc::new(RefCell::new(Box::new(callback)));
        self.on_auto_dismiss = Some(if let Some(existing) = self.on_auto_dismiss.take() {
            Rc::new(RefCell::new(Box::new(move || {
                (existing.borrow_mut())();
                (next.borrow_mut())();
            })))
        } else {
            next
        });
        self
    }

    pub fn on_dismiss<F>(mut self, callback: F) -> Self
    where
        F: FnMut() + 'static,
    {
        let next: ClickCallback = Rc::new(RefCell::new(Box::new(callback)));
        self.on_dismiss = Some(if let Some(existing) = self.on_dismiss.take() {
            Rc::new(RefCell::new(Box::new(move || {
                (existing.borrow_mut())();
                (next.borrow_mut())();
            })))
        } else {
            next
        });
        self
    }

    pub fn build(self) -> Response {
        let id = self.id.clone();
        let width = self.width.min((self.screen_width - 32.0).max(0.0));
        let height = self.height.min((self.screen_height - 32.0).max(0.0));
        let x = 16.0_f32.max(self.screen_width - width - 28.0);
        let y = 16.0_f32.max(self.screen_height - height - 28.0);
        let icon_size = 22.0;
        let text_x = 54.0;
        let close_size = 28.0;
        let text_width = (width - text_x - close_size - 22.0).max(0.0);
        let visible = if self.visible { 1.0 } else { 0.0 };
        let toast_offset_x = if self.visible { 0.0 } else { 18.0 };
        let toast_offset_y = if self.visible { 0.0 } else { 10.0 };
        let on_dismiss = self.on_dismiss.clone();
        let on_auto_dismiss = self
            .on_auto_dismiss
            .clone()
            .or_else(|| self.on_dismiss.clone());
        let resolved_id = self.ui.resolve_id(&id);

        self.ui.register_layer_intent(LayerIntent {
            id: LayerId::new(resolved_id.clone()),
            owner: NodeId::new(resolved_id.clone()),
            root: NodeId::new(resolved_id),
            anchor: None,
            fallback_anchor: Some(LayoutRect::new(
                0.0,
                0.0,
                self.screen_width,
                self.screen_height,
            )),
            boundary: None,
            open: self.visible,
            kind: LayerKind::Toast,
            placement: LayerPlacement::BottomEnd,
            size: LayerSize::new(width.into(), height.into()),
            gap: 0.0,
            offset: [0.0, 0.0],
            collision: Default::default(),
            z_index: self.z_index,
            outside_click: OutsideClickPolicy::Ignore,
        });

        self.ui.with_root_layer(|ui| {
            ui.stack(id.clone())
                .x(x)
                .y(y)
                .size(width, height)
                .z_index(self.z_index)
                .opacity(visible)
                .translate(toast_offset_x, toast_offset_y)
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

                    ui.text(format!("{id}.icon"))
                        .x(20.0)
                        .y(20.0)
                        .size(icon_size, icon_size)
                        .icon(self.icon.clone())
                        .font_size(icon_size)
                        .line_height(icon_size)
                        .color(self.style.accent)
                        .horizontal_align(HorizontalAlign::Center)
                        .build();

                    ui.text(format!("{id}.title"))
                        .x(text_x)
                        .y(16.0)
                        .size(text_width, 24.0)
                        .text(self.title.clone())
                        .font_size(18.0)
                        .line_height(22.0)
                        .color(self.style.text)
                        .build();

                    ui.text(format!("{id}.message"))
                        .x(text_x)
                        .y(42.0)
                        .size(text_width, (height - 50.0).max(0.0))
                        .text(self.message.clone())
                        .font_size(14.0)
                        .line_height(18.0)
                        .max_width(text_width)
                        .wrap(true)
                        .color(self.style.muted_text)
                        .build();

                    let dismiss = on_dismiss.clone();
                    ui.rect(format!("{id}.close.hit"))
                        .x((width - close_size - 12.0).max(0.0))
                        .y(12.0)
                        .size(close_size, close_size)
                        .states(
                            theme::color(0.0, 0.0, 0.0, 0.0),
                            theme::with_opacity(self.style.border, 0.36),
                            theme::with_opacity(self.style.border, 0.56),
                        )
                        .radius(8.0)
                        .disabled(!self.visible)
                        .on_click(move || call_click(&dismiss))
                        .build();

                    ui.text(format!("{id}.close"))
                        .x((width - close_size - 12.0).max(0.0))
                        .y(17.0)
                        .size(close_size, close_size)
                        .icon_codepoint(0xF00D)
                        .font_size(15.0)
                        .line_height(18.0)
                        .color(self.style.muted_text)
                        .horizontal_align(HorizontalAlign::Center)
                        .vertical_align(VerticalAlign::Top)
                        .build();

                    let timer = ui.stack(format!("{id}.timer")).size(0.0, 0.0);
                    if self.visible && self.auto_dismiss_seconds > 0.0 && on_auto_dismiss.is_some()
                    {
                        timer
                            .on_timer(self.auto_dismiss_seconds, move || {
                                call_click(&on_auto_dismiss)
                            })
                            .build();
                    } else {
                        timer.build();
                    }
                });
        });

        self.ui.response(&id)
    }
}

pub fn toast(ui: &mut Ui, id: impl Into<String>) -> ToastBuilder<'_> {
    ToastBuilder::new(ui, id)
}

fn call_click(callback: &Option<ClickCallback>) {
    if let Some(callback) = callback {
        (callback.borrow_mut())();
    }
}

fn utf8(codepoint: u32) -> String {
    char::from_u32(codepoint)
        .map(|value| value.to_string())
        .unwrap_or_default()
}
