//! Port of `EUI-NEO/components/scroll.h`.

use std::cell::RefCell;
use std::rc::Rc;

use crate::render::Color;

use super::super::{AnimProperty, Binding, CursorShape, DragEvent, Ease, Response, Transition, Ui};
use super::theme::{self, ThemeColorTokens};

type ChangeCallback = Rc<RefCell<Box<dyn FnMut(f32)>>>;

#[derive(Debug, Clone, Copy)]
pub struct ScrollStyle {
    pub track: Color,
    pub thumb: Color,
    pub thumb_hover: Color,
    pub thumb_pressed: Color,
    pub radius: f32,
}

impl ScrollStyle {
    pub fn new(tokens: ThemeColorTokens) -> Self {
        Self {
            track: theme::with_opacity(tokens.surface_hover, if tokens.dark { 0.34 } else { 0.46 }),
            thumb: theme::with_opacity(tokens.text, if tokens.dark { 0.34 } else { 0.28 }),
            thumb_hover: theme::with_opacity(tokens.text, if tokens.dark { 0.46 } else { 0.38 }),
            thumb_pressed: theme::with_opacity(tokens.primary, 0.76),
            radius: 999.0,
        }
    }
}

impl Default for ScrollStyle {
    fn default() -> Self {
        Self::new(theme::dark_theme_colors())
    }
}

pub struct ScrollBuilder<'ui> {
    ui: &'ui mut Ui,
    id: String,
    style: ScrollStyle,
    transition: Transition,
    on_change: Option<ChangeCallback>,
    width: f32,
    height: f32,
    viewport: f32,
    content: f32,
    offset: f32,
    step: f32,
    x: f32,
    y: f32,
    has_x: bool,
    has_y: bool,
    z_index: i32,
}

impl<'ui> ScrollBuilder<'ui> {
    pub fn new(ui: &'ui mut Ui, id: impl Into<String>) -> Self {
        Self {
            ui,
            id: id.into(),
            style: ScrollStyle::default(),
            transition: Transition::make(0.12, Ease::OutCubic),
            on_change: None,
            width: 8.0,
            height: 180.0,
            viewport: 180.0,
            content: 180.0,
            offset: 0.0,
            step: 42.0,
            x: 0.0,
            y: 0.0,
            has_x: false,
            has_y: false,
            z_index: 0,
        }
    }

    pub fn x(mut self, value: f32) -> Self {
        self.x = value;
        self.has_x = true;
        self
    }

    pub fn y(mut self, value: f32) -> Self {
        self.y = value;
        self.has_y = true;
        self
    }

    pub fn position(mut self, x: f32, y: f32) -> Self {
        self.x = x;
        self.y = y;
        self.has_x = true;
        self.has_y = true;
        self
    }

    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    pub fn offset(mut self, value: f32) -> Self {
        self.offset = value.max(0.0);
        self
    }

    pub fn offset_bind<T: 'static>(self, binding: Binding<T, f32>) -> Self {
        let value = binding.get();
        self.offset(value).on_change(move |next| binding.set(next))
    }

    pub fn value(self, value: f32) -> Self {
        self.offset(value)
    }

    pub fn value_bind<T: 'static>(self, binding: Binding<T, f32>) -> Self {
        self.offset_bind(binding)
    }

    pub fn viewport(mut self, value: f32) -> Self {
        self.viewport = value.max(0.0);
        self
    }

    pub fn viewport_height(self, value: f32) -> Self {
        self.viewport(value)
    }

    pub fn content(mut self, value: f32) -> Self {
        self.content = value.max(0.0);
        self
    }

    pub fn content_height(self, value: f32) -> Self {
        self.content(value)
    }

    pub fn step(mut self, value: f32) -> Self {
        self.step = value.max(1.0);
        self
    }

    pub fn z_index(mut self, value: i32) -> Self {
        self.z_index = value;
        self
    }

    pub fn z(self, value: i32) -> Self {
        self.z_index(value)
    }

    pub fn style(mut self, value: ScrollStyle) -> Self {
        self.style = value;
        self
    }

    pub fn theme(mut self, tokens: ThemeColorTokens) -> Self {
        self.style = ScrollStyle::new(tokens);
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
        F: FnMut(f32) + 'static,
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

    pub fn viewportHeight(self, value: f32) -> Self {
        self.viewport_height(value)
    }

    pub fn offsetBind<T: 'static>(self, binding: Binding<T, f32>) -> Self {
        self.offset_bind(binding)
    }

    pub fn valueBind<T: 'static>(self, binding: Binding<T, f32>) -> Self {
        self.value_bind(binding)
    }

    pub fn contentHeight(self, value: f32) -> Self {
        self.content_height(value)
    }

    pub fn zIndex(self, value: i32) -> Self {
        self.z_index(value)
    }

    pub fn transitionSeconds(self, duration: f32, ease: Ease) -> Self {
        self.transition_seconds(duration, ease)
    }

    pub fn onChange<F>(self, callback: F) -> Self
    where
        F: FnMut(f32) + 'static,
    {
        self.on_change(callback)
    }

    pub fn build(self) -> Response {
        let id = self.id.clone();
        let max_offset = (self.content - self.viewport).max(0.0);
        let scrollable = max_offset > 0.0 && self.viewport > 0.0 && self.content > 0.0;
        let normalized = if scrollable {
            (self.offset / max_offset).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let thumb_height = if scrollable {
            (self.height * (self.viewport / self.content)).clamp(self.height.min(24.0), self.height)
        } else {
            self.height
        };
        let travel = (self.height - thumb_height).max(0.0);
        let thumb_y = travel * normalized;
        let current_offset = self.offset.clamp(0.0, max_offset);
        let scroll_step = self.step;
        let on_scroll_change = self.on_change.clone();
        let on_drag_change = self.on_change.clone();

        let mut root = self
            .ui
            .stack(id.clone())
            .size(self.width, self.height)
            .z_index(self.z_index);
        if self.has_x {
            root = root.x(self.x);
        }
        if self.has_y {
            root = root.y(self.y);
        }

        root.content(|ui| {
            ui.rect(format!("{id}.track"))
                .size(self.width, self.height)
                .color(self.style.track)
                .radius(self.style.radius)
                .on_scroll(move |event| {
                    if !scrollable {
                        return;
                    }
                    if let Some(callback) = &on_scroll_change {
                        let next = (current_offset - event.y * scroll_step).clamp(0.0, max_offset);
                        (callback.borrow_mut())(next);
                    }
                })
                .build();

            ui.rect(format!("{id}.thumb"))
                .y(thumb_y)
                .size(self.width, thumb_height)
                .states(
                    self.style.thumb,
                    self.style.thumb_hover,
                    self.style.thumb_pressed,
                )
                .radius(self.style.radius)
                .cursor(CursorShape::Hand)
                .transition(self.transition)
                .animate(AnimProperty::COLOR)
                .on_drag(move |event| {
                    if !scrollable || travel <= 0.0 {
                        return;
                    }
                    if let Some(callback) = &on_drag_change {
                        let next = drag_offset(event, current_offset, max_offset, travel);
                        (callback.borrow_mut())(next);
                    }
                })
                .build();
        })
    }
}

pub fn scroll(ui: &mut Ui, id: impl Into<String>) -> ScrollBuilder<'_> {
    ScrollBuilder::new(ui, id)
}

fn drag_offset(event: DragEvent, current_offset: f32, max_offset: f32, travel: f32) -> f32 {
    (current_offset + event.delta_y * (max_offset / travel)).clamp(0.0, max_offset)
}

#[cfg(test)]
mod tests {
    use super::drag_offset;
    use crate::ui::neo::DragEvent;

    #[test]
    fn scroll_drag_offset_uses_thumb_travel_ratio() {
        let event = DragEvent {
            delta_y: 10.0,
            ..DragEvent::default()
        };

        assert_eq!(drag_offset(event, 20.0, 200.0, 100.0), 40.0);
    }
}
