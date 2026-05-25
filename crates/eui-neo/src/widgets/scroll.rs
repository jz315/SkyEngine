//! Scrollbar and scroll-container composition helpers.

use std::cell::RefCell;
use std::rc::Rc;

use crate::Color;

use super::super::{
    Align, AnimProperty, Binding, CursorShape, DragEvent, EdgeInsets, Response, Size, Transition,
    Ui,
};
use super::layout::WidgetLayout;
use super::theme::{self, ThemeColorTokens};

type ChangeCallback = Rc<RefCell<Box<dyn FnMut(f32)>>>;

#[derive(Debug, Clone, Copy)]
pub struct ScrollbarStyle {
    pub track: Color,
    pub thumb: Color,
    pub thumb_hover: Color,
    pub thumb_pressed: Color,
    pub radius: f32,
}

impl ScrollbarStyle {
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

impl Default for ScrollbarStyle {
    fn default() -> Self {
        Self::new(theme::dark_theme_colors())
    }
}

pub struct ScrollbarBuilder<'ui> {
    ui: &'ui mut Ui,
    id: String,
    style: ScrollbarStyle,
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

impl<'ui> ScrollbarBuilder<'ui> {
    pub fn new(ui: &'ui mut Ui, id: impl Into<String>) -> Self {
        Self {
            ui,
            id: id.into(),
            style: ScrollbarStyle::default(),
            transition: Transition::responsive(),
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

    pub fn style(mut self, value: ScrollbarStyle) -> Self {
        self.style = value;
        self
    }

    pub fn theme(mut self, tokens: ThemeColorTokens) -> Self {
        self.style = ScrollbarStyle::new(tokens);
        self
    }

    pub fn transition(mut self, value: Transition) -> Self {
        self.transition = value;
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

pub fn scrollbar(ui: &mut Ui, id: impl Into<String>) -> ScrollbarBuilder<'_> {
    ScrollbarBuilder::new(ui, id)
}

pub struct ScrollColumnBuilder<'ui> {
    ui: &'ui mut Ui,
    id: String,
    layout: WidgetLayout,
    style: ScrollbarStyle,
    offset: f32,
    content_height: f32,
    step: f32,
    gap: f32,
    padding: EdgeInsets,
    scrollbar_width: f32,
    scrollbar_gap: f32,
    on_change: Option<ChangeCallback>,
}

impl<'ui> ScrollColumnBuilder<'ui> {
    pub fn new(ui: &'ui mut Ui, id: impl Into<String>) -> Self {
        Self {
            ui,
            id: id.into(),
            layout: WidgetLayout::new(320.0, 240.0),
            style: ScrollbarStyle::default(),
            offset: 0.0,
            content_height: 240.0,
            step: 48.0,
            gap: 0.0,
            padding: EdgeInsets::ZERO,
            scrollbar_width: 8.0,
            scrollbar_gap: 8.0,
            on_change: None,
        }
    }

    pub fn width(mut self, value: impl Into<Size>) -> Self {
        self.layout = self.layout.width(value);
        self
    }

    pub fn height(mut self, value: impl Into<Size>) -> Self {
        self.layout = self.layout.height(value);
        self
    }

    pub fn size(mut self, width: impl Into<Size>, height: impl Into<Size>) -> Self {
        self.layout = self.layout.size(width, height);
        self
    }

    pub fn min_width(mut self, value: f32) -> Self {
        self.layout = self.layout.min_width(value);
        self
    }

    pub fn max_width(mut self, value: f32) -> Self {
        self.layout = self.layout.max_width(value);
        self
    }

    pub fn min_height(mut self, value: f32) -> Self {
        self.layout = self.layout.min_height(value);
        self
    }

    pub fn max_height(mut self, value: f32) -> Self {
        self.layout = self.layout.max_height(value);
        self
    }

    pub fn grow(mut self, value: f32) -> Self {
        self.layout = self.layout.grow(value);
        self
    }

    pub fn margin(mut self, value: f32) -> Self {
        self.layout = self.layout.margin(value);
        self
    }

    pub fn margin_xy(mut self, horizontal: f32, vertical: f32) -> Self {
        self.layout = self.layout.margin_xy(horizontal, vertical);
        self
    }

    pub fn margin_each(mut self, left: f32, top: f32, right: f32, bottom: f32) -> Self {
        self.layout = self.layout.margin_each(left, top, right, bottom);
        self
    }

    pub fn padding(mut self, value: f32) -> Self {
        self.padding = EdgeInsets::all(value);
        self
    }

    pub fn padding_xy(mut self, horizontal: f32, vertical: f32) -> Self {
        self.padding = EdgeInsets::symmetric(horizontal, vertical);
        self
    }

    pub fn padding_each(mut self, left: f32, top: f32, right: f32, bottom: f32) -> Self {
        self.padding = EdgeInsets::new(left, top, right, bottom);
        self
    }

    pub fn gap(mut self, value: f32) -> Self {
        self.gap = value.max(0.0);
        self
    }

    pub fn spacing(self, value: f32) -> Self {
        self.gap(value)
    }

    pub fn content_height(mut self, value: f32) -> Self {
        self.content_height = value.max(0.0);
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

    pub fn step(mut self, value: f32) -> Self {
        self.step = value.max(1.0);
        self
    }

    pub fn scrollbar_width(mut self, value: f32) -> Self {
        self.scrollbar_width = value.max(0.0);
        self
    }

    pub fn scrollbar_gap(mut self, value: f32) -> Self {
        self.scrollbar_gap = value.max(0.0);
        self
    }

    pub fn style(mut self, value: ScrollbarStyle) -> Self {
        self.style = value;
        self
    }

    pub fn theme(mut self, tokens: ThemeColorTokens) -> Self {
        self.style = ScrollbarStyle::new(tokens);
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

    pub fn content(self, content: impl FnOnce(&mut Ui)) -> Response {
        let id = self.id.clone();
        let viewport_h = self.layout.fixed_height_or(240.0);
        let max_offset = (self.content_height - viewport_h).max(0.0);
        let offset = self.offset.clamp(0.0, max_offset);
        let scrollable = max_offset > 0.0;
        let scroll_step = self.step;
        let on_wheel_change = self.on_change.clone();
        let on_scrollbar_change = self.on_change.clone();
        let reserved_right = if scrollable {
            self.scrollbar_width + self.scrollbar_gap
        } else {
            0.0
        };

        self.layout
            .apply_to_size(
                self.ui.stack(id.clone()),
                self.layout.width,
                self.layout.height,
            )
            .align_items(Align::End)
            .content(|ui| {
                let mut viewport = ui.stack(format!("{id}.viewport")).fill().clip();
                if scrollable {
                    viewport = viewport.on_scroll(move |event| {
                        if let Some(callback) = &on_wheel_change {
                            let next = (offset - event.y * scroll_step).clamp(0.0, max_offset);
                            (callback.borrow_mut())(next);
                        }
                    });
                }

                viewport.content(|ui| {
                    ui.column(format!("{id}.content"))
                        .y(-offset)
                        .size(Size::fill(), self.content_height)
                        .padding_each(
                            self.padding.left,
                            self.padding.top,
                            self.padding.right + reserved_right,
                            self.padding.bottom,
                        )
                        .gap(self.gap)
                        .content(content);
                });

                if scrollable && self.scrollbar_width > 0.0 {
                    scrollbar(ui, format!("{id}.scrollbar"))
                        .size(self.scrollbar_width, viewport_h)
                        .viewport(viewport_h)
                        .content(self.content_height)
                        .offset(offset)
                        .step(self.step)
                        .style(self.style)
                        .on_change(move |next| {
                            if let Some(callback) = &on_scrollbar_change {
                                (callback.borrow_mut())(next);
                            }
                        })
                        .build();
                }
            })
    }
}

pub fn scroll_column(ui: &mut Ui, id: impl Into<String>) -> ScrollColumnBuilder<'_> {
    ScrollColumnBuilder::new(ui, id)
}

fn drag_offset(event: DragEvent, current_offset: f32, max_offset: f32, travel: f32) -> f32 {
    (current_offset + event.delta_y * (max_offset / travel)).clamp(0.0, max_offset)
}

#[cfg(test)]
mod tests {
    use super::drag_offset;
    use crate::DragEvent;

    #[test]
    fn scrollbar_drag_offset_uses_thumb_travel_ratio() {
        let event = DragEvent {
            delta_y: 10.0,
            ..DragEvent::default()
        };

        assert_eq!(drag_offset(event, 20.0, 200.0, 100.0), 40.0);
    }
}
