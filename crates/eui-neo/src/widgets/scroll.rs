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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScrollAxis {
    X,
    Y,
}

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
    z_index: i32,
    axis: ScrollAxis,
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
            z_index: 0,
            axis: ScrollAxis::Y,
        }
    }

    pub(super) fn horizontal(mut self) -> Self {
        self.axis = ScrollAxis::X;
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

    pub fn viewport(mut self, value: f32) -> Self {
        self.viewport = value.max(0.0);
        self
    }

    pub fn content(mut self, value: f32) -> Self {
        self.content = value.max(0.0);
        self
    }

    pub fn step(mut self, value: f32) -> Self {
        self.step = value.max(1.0);
        self
    }

    pub fn style(mut self, value: ScrollbarStyle) -> Self {
        self.style = value;
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
        let track_len = match self.axis {
            ScrollAxis::X => self.width,
            ScrollAxis::Y => self.height,
        };
        let thumb_len = if scrollable {
            (track_len * (self.viewport / self.content)).clamp(track_len.min(24.0), track_len)
        } else {
            track_len
        };
        let travel = (track_len - thumb_len).max(0.0);
        let thumb_offset = travel * normalized;
        let current_offset = self.offset.clamp(0.0, max_offset);
        let scroll_step = self.step;
        let on_scroll_change = self.on_change.clone();
        let on_drag_change = self.on_change.clone();
        let axis = self.axis;

        let root = self
            .ui
            .stack(id.clone())
            .size(self.width, self.height)
            .z_index(self.z_index);

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
                        let next = (current_offset - scroll_delta(axis, event) * scroll_step)
                            .clamp(0.0, max_offset);
                        (callback.borrow_mut())(next);
                    }
                })
                .build();

            let mut thumb = ui.rect(format!("{id}.thumb"));
            thumb = match axis {
                ScrollAxis::X => thumb.x(thumb_offset).size(thumb_len, self.height),
                ScrollAxis::Y => thumb.y(thumb_offset).size(self.width, thumb_len),
            };
            thumb
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
                        let next = drag_offset(axis, event, current_offset, max_offset, travel);
                        (callback.borrow_mut())(next);
                    }
                })
                .build();
        })
    }
}

pub(super) fn scrollbar(ui: &mut Ui, id: impl Into<String>) -> ScrollbarBuilder<'_> {
    ScrollbarBuilder::new(ui, id)
}

struct ScrollAreaBuilder<'ui> {
    ui: &'ui mut Ui,
    id: String,
    axis: ScrollAxis,
    layout: WidgetLayout,
    style: ScrollbarStyle,
    offset: f32,
    content_extent: Option<f32>,
    step: f32,
    gap: f32,
    inset: EdgeInsets,
    padding: EdgeInsets,
    scrollbar_size: f32,
    scrollbar_gap: f32,
    on_change: Option<ChangeCallback>,
}

impl<'ui> ScrollAreaBuilder<'ui> {
    fn new(ui: &'ui mut Ui, id: impl Into<String>, axis: ScrollAxis) -> Self {
        let layout = match axis {
            ScrollAxis::X => WidgetLayout::new(320.0, 120.0),
            ScrollAxis::Y => WidgetLayout::new(320.0, 240.0),
        };
        Self {
            ui,
            id: id.into(),
            axis,
            layout,
            style: ScrollbarStyle::default(),
            offset: 0.0,
            content_extent: None,
            step: 48.0,
            gap: 0.0,
            inset: EdgeInsets::ZERO,
            padding: EdgeInsets::ZERO,
            scrollbar_size: 8.0,
            scrollbar_gap: 8.0,
            on_change: None,
        }
    }

    fn width(mut self, value: impl Into<Size>) -> Self {
        self.layout = self.layout.width(value);
        self
    }

    fn height(mut self, value: impl Into<Size>) -> Self {
        self.layout = self.layout.height(value);
        self
    }

    fn size(mut self, width: impl Into<Size>, height: impl Into<Size>) -> Self {
        self.layout = self.layout.size(width, height);
        self
    }

    fn fill(mut self) -> Self {
        self.layout = self.layout.size(Size::fill(), Size::fill());
        self
    }

    fn min_width(mut self, value: f32) -> Self {
        self.layout = self.layout.min_width(value);
        self
    }

    fn max_width(mut self, value: f32) -> Self {
        self.layout = self.layout.max_width(value);
        self
    }

    fn min_height(mut self, value: f32) -> Self {
        self.layout = self.layout.min_height(value);
        self
    }

    fn max_height(mut self, value: f32) -> Self {
        self.layout = self.layout.max_height(value);
        self
    }

    fn grow(mut self, value: f32) -> Self {
        self.layout = self.layout.grow(value);
        self
    }

    fn margin(mut self, value: f32) -> Self {
        self.layout = self.layout.margin(value);
        self
    }

    fn margin_xy(mut self, horizontal: f32, vertical: f32) -> Self {
        self.layout = self.layout.margin_xy(horizontal, vertical);
        self
    }

    fn margin_each(mut self, left: f32, top: f32, right: f32, bottom: f32) -> Self {
        self.layout = self.layout.margin_each(left, top, right, bottom);
        self
    }

    fn inset(mut self, value: f32) -> Self {
        self.inset = EdgeInsets::all(value);
        self
    }

    fn inset_xy(mut self, horizontal: f32, vertical: f32) -> Self {
        self.inset = EdgeInsets::symmetric(horizontal, vertical);
        self
    }

    fn inset_each(mut self, left: f32, top: f32, right: f32, bottom: f32) -> Self {
        self.inset = EdgeInsets::new(left, top, right, bottom);
        self
    }

    fn padding(mut self, value: f32) -> Self {
        self.padding = EdgeInsets::all(value);
        self
    }

    fn padding_xy(mut self, horizontal: f32, vertical: f32) -> Self {
        self.padding = EdgeInsets::symmetric(horizontal, vertical);
        self
    }

    fn padding_each(mut self, left: f32, top: f32, right: f32, bottom: f32) -> Self {
        self.padding = EdgeInsets::new(left, top, right, bottom);
        self
    }

    fn gap(mut self, value: f32) -> Self {
        self.gap = value.max(0.0);
        self
    }

    fn content_extent(mut self, value: f32) -> Self {
        self.content_extent = Some(value.max(0.0));
        self
    }

    fn auto_content_extent(mut self) -> Self {
        self.content_extent = None;
        self
    }

    fn offset(mut self, value: f32) -> Self {
        self.offset = value.max(0.0);
        self
    }

    fn step(mut self, value: f32) -> Self {
        self.step = value.max(1.0);
        self
    }

    fn scrollbar_size(mut self, value: f32) -> Self {
        self.scrollbar_size = value.max(0.0);
        self
    }

    fn scrollbar_gap(mut self, value: f32) -> Self {
        self.scrollbar_gap = value.max(0.0);
        self
    }

    fn style(mut self, value: ScrollbarStyle) -> Self {
        self.style = value;
        self
    }

    fn theme(mut self, tokens: ThemeColorTokens) -> Self {
        self.style = ScrollbarStyle::new(tokens);
        self
    }

    fn on_change<F>(mut self, callback: F) -> Self
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

    fn content(self, content: impl FnOnce(&mut Ui)) -> Response {
        let id = self.id.clone();
        let viewport_id = format!("{id}.viewport");
        let content_id = format!("{id}.content");
        let axis = self.axis;
        let fallback_viewport_extent = match axis {
            ScrollAxis::X => (self.layout.fixed_width_or(320.0) - self.inset.horizontal()).max(0.0),
            ScrollAxis::Y => (self.layout.fixed_height_or(240.0) - self.inset.vertical()).max(0.0),
        };
        let viewport_extent = self
            .ui
            .previous_frame(&viewport_id)
            .map(|frame| match axis {
                ScrollAxis::X => frame.width,
                ScrollAxis::Y => frame.height,
            })
            .unwrap_or(fallback_viewport_extent);
        let measured_content_extent = self.ui.previous_frame(&content_id).map(|frame| match axis {
            ScrollAxis::X => frame.width,
            ScrollAxis::Y => frame.height,
        });
        let content_extent = self
            .content_extent
            .or(measured_content_extent)
            .unwrap_or(viewport_extent)
            .max(viewport_extent);
        let max_offset = (content_extent - viewport_extent).max(0.0);
        let offset = self.offset.clamp(0.0, max_offset);
        let scrollable = max_offset > 0.0;
        let scroll_step = self.step;
        let on_wheel_change = self.on_change.clone();
        let on_scrollbar_change = self.on_change.clone();
        let reserved_cross = if scrollable {
            self.scrollbar_size + self.scrollbar_gap
        } else {
            0.0
        };
        let content_size = self
            .content_extent
            .map(Size::fixed)
            .unwrap_or_else(Size::wrap_content);
        let padding = self.padding;
        let gap = self.gap;
        let scrollbar_size = self.scrollbar_size;
        let style = self.style;
        let step = self.step;

        let root = self
            .layout
            .apply_to_size(
                self.ui.stack(id.clone()),
                self.layout.width,
                self.layout.height,
            )
            .padding_each(
                self.inset.left,
                self.inset.top,
                self.inset.right,
                self.inset.bottom,
            );
        let root = match axis {
            ScrollAxis::X => root.justify_content(Align::End),
            ScrollAxis::Y => root.align_items(Align::End),
        };

        root.content(|ui| {
            let mut viewport = ui.stack(viewport_id).fill().clip();
            if scrollable {
                viewport = viewport.on_scroll(move |event| {
                    if let Some(callback) = &on_wheel_change {
                        let next = (offset - scroll_delta(axis, event) * scroll_step)
                            .clamp(0.0, max_offset);
                        (callback.borrow_mut())(next);
                    }
                });
            }

            viewport.content(|ui| match axis {
                ScrollAxis::X => {
                    ui.row(content_id)
                        .x(-offset)
                        .size(content_size, Size::fill())
                        .padding_each(
                            padding.left,
                            padding.top,
                            padding.right,
                            padding.bottom + reserved_cross,
                        )
                        .gap(gap)
                        .content(content);
                }
                ScrollAxis::Y => {
                    ui.column(content_id)
                        .y(-offset)
                        .size(Size::fill(), content_size)
                        .padding_each(
                            padding.left,
                            padding.top,
                            padding.right + reserved_cross,
                            padding.bottom,
                        )
                        .gap(gap)
                        .content(content);
                }
            });

            if scrollable && scrollbar_size > 0.0 {
                let scrollbar = scrollbar(ui, format!("{id}.scrollbar"));
                let scrollbar = match axis {
                    ScrollAxis::X => scrollbar.horizontal().size(viewport_extent, scrollbar_size),
                    ScrollAxis::Y => scrollbar.size(scrollbar_size, viewport_extent),
                };
                scrollbar
                    .viewport(viewport_extent)
                    .content(content_extent)
                    .offset(offset)
                    .step(step)
                    .style(style)
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

macro_rules! scroll_area_common_methods {
    () => {
        pub fn width(mut self, value: impl Into<Size>) -> Self {
            self.inner = self.inner.width(value);
            self
        }

        pub fn height(mut self, value: impl Into<Size>) -> Self {
            self.inner = self.inner.height(value);
            self
        }

        pub fn size(mut self, width: impl Into<Size>, height: impl Into<Size>) -> Self {
            self.inner = self.inner.size(width, height);
            self
        }

        pub fn fill(mut self) -> Self {
            self.inner = self.inner.fill();
            self
        }

        pub fn min_width(mut self, value: f32) -> Self {
            self.inner = self.inner.min_width(value);
            self
        }

        pub fn max_width(mut self, value: f32) -> Self {
            self.inner = self.inner.max_width(value);
            self
        }

        pub fn min_height(mut self, value: f32) -> Self {
            self.inner = self.inner.min_height(value);
            self
        }

        pub fn max_height(mut self, value: f32) -> Self {
            self.inner = self.inner.max_height(value);
            self
        }

        pub fn grow(mut self, value: f32) -> Self {
            self.inner = self.inner.grow(value);
            self
        }

        pub fn margin(mut self, value: f32) -> Self {
            self.inner = self.inner.margin(value);
            self
        }

        pub fn margin_xy(mut self, horizontal: f32, vertical: f32) -> Self {
            self.inner = self.inner.margin_xy(horizontal, vertical);
            self
        }

        pub fn margin_each(mut self, left: f32, top: f32, right: f32, bottom: f32) -> Self {
            self.inner = self.inner.margin_each(left, top, right, bottom);
            self
        }

        /// Inset the clipped viewport and scrollbar from the outer container.
        pub fn inset(mut self, value: f32) -> Self {
            self.inner = self.inner.inset(value);
            self
        }

        pub fn inset_xy(mut self, horizontal: f32, vertical: f32) -> Self {
            self.inner = self.inner.inset_xy(horizontal, vertical);
            self
        }

        pub fn inset_each(mut self, left: f32, top: f32, right: f32, bottom: f32) -> Self {
            self.inner = self.inner.inset_each(left, top, right, bottom);
            self
        }

        pub fn viewport_inset(self, value: f32) -> Self {
            self.inset(value)
        }

        pub fn viewport_inset_xy(self, horizontal: f32, vertical: f32) -> Self {
            self.inset_xy(horizontal, vertical)
        }

        pub fn viewport_inset_each(self, left: f32, top: f32, right: f32, bottom: f32) -> Self {
            self.inset_each(left, top, right, bottom)
        }

        pub fn padding(mut self, value: f32) -> Self {
            self.inner = self.inner.padding(value);
            self
        }

        pub fn padding_xy(mut self, horizontal: f32, vertical: f32) -> Self {
            self.inner = self.inner.padding_xy(horizontal, vertical);
            self
        }

        pub fn padding_each(mut self, left: f32, top: f32, right: f32, bottom: f32) -> Self {
            self.inner = self.inner.padding_each(left, top, right, bottom);
            self
        }

        pub fn content_padding(self, value: f32) -> Self {
            self.padding(value)
        }

        pub fn content_padding_xy(self, horizontal: f32, vertical: f32) -> Self {
            self.padding_xy(horizontal, vertical)
        }

        pub fn content_padding_each(self, left: f32, top: f32, right: f32, bottom: f32) -> Self {
            self.padding_each(left, top, right, bottom)
        }

        pub fn gap(mut self, value: f32) -> Self {
            self.inner = self.inner.gap(value);
            self
        }

        pub fn spacing(self, value: f32) -> Self {
            self.gap(value)
        }

        pub fn offset(mut self, value: f32) -> Self {
            self.inner = self.inner.offset(value);
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
            self.inner = self.inner.step(value);
            self
        }

        pub fn scrollbar_gap(mut self, value: f32) -> Self {
            self.inner = self.inner.scrollbar_gap(value);
            self
        }

        pub fn style(mut self, value: ScrollbarStyle) -> Self {
            self.inner = self.inner.style(value);
            self
        }

        pub fn theme(mut self, tokens: ThemeColorTokens) -> Self {
            self.inner = self.inner.theme(tokens);
            self
        }

        pub fn on_change<F>(mut self, callback: F) -> Self
        where
            F: FnMut(f32) + 'static,
        {
            self.inner = self.inner.on_change(callback);
            self
        }
    };
}

pub struct ScrollYBuilder<'ui> {
    inner: ScrollAreaBuilder<'ui>,
}

impl<'ui> ScrollYBuilder<'ui> {
    pub fn new(ui: &'ui mut Ui, id: impl Into<String>) -> Self {
        Self {
            inner: ScrollAreaBuilder::new(ui, id, ScrollAxis::Y),
        }
    }

    scroll_area_common_methods!();

    pub fn content_height(mut self, value: f32) -> Self {
        self.inner = self.inner.content_extent(value);
        self
    }

    pub fn auto_content_height(mut self) -> Self {
        self.inner = self.inner.auto_content_extent();
        self
    }

    pub fn scrollbar_width(mut self, value: f32) -> Self {
        self.inner = self.inner.scrollbar_size(value);
        self
    }

    pub fn content(self, content: impl FnOnce(&mut Ui)) -> Response {
        self.inner.content(content)
    }
}

pub fn scroll_y(ui: &mut Ui, id: impl Into<String>) -> ScrollYBuilder<'_> {
    ScrollYBuilder::new(ui, id)
}

pub struct ScrollXBuilder<'ui> {
    inner: ScrollAreaBuilder<'ui>,
}

impl<'ui> ScrollXBuilder<'ui> {
    pub fn new(ui: &'ui mut Ui, id: impl Into<String>) -> Self {
        Self {
            inner: ScrollAreaBuilder::new(ui, id, ScrollAxis::X),
        }
    }

    scroll_area_common_methods!();

    pub fn content_width(mut self, value: f32) -> Self {
        self.inner = self.inner.content_extent(value);
        self
    }

    pub fn auto_content_width(mut self) -> Self {
        self.inner = self.inner.auto_content_extent();
        self
    }

    pub fn scrollbar_height(mut self, value: f32) -> Self {
        self.inner = self.inner.scrollbar_size(value);
        self
    }

    pub fn content(self, content: impl FnOnce(&mut Ui)) -> Response {
        self.inner.content(content)
    }
}

pub fn scroll_x(ui: &mut Ui, id: impl Into<String>) -> ScrollXBuilder<'_> {
    ScrollXBuilder::new(ui, id)
}

fn drag_offset(
    axis: ScrollAxis,
    event: DragEvent,
    current_offset: f32,
    max_offset: f32,
    travel: f32,
) -> f32 {
    (current_offset + drag_delta(axis, event) * (max_offset / travel)).clamp(0.0, max_offset)
}

fn drag_delta(axis: ScrollAxis, event: DragEvent) -> f32 {
    match axis {
        ScrollAxis::X => event.delta_x,
        ScrollAxis::Y => event.delta_y,
    }
}

fn scroll_delta(axis: ScrollAxis, event: crate::ScrollEvent) -> f32 {
    match axis {
        ScrollAxis::X if event.x != 0.0 => event.x,
        ScrollAxis::X => event.y,
        ScrollAxis::Y => event.y,
    }
}

#[cfg(test)]
mod tests {
    use super::{drag_offset, ScrollAxis};
    use crate::DragEvent;

    #[test]
    fn vertical_scrollbar_drag_offset_uses_thumb_travel_ratio() {
        let event = DragEvent {
            delta_y: 10.0,
            ..DragEvent::default()
        };

        assert_eq!(drag_offset(ScrollAxis::Y, event, 20.0, 200.0, 100.0), 40.0);
    }

    #[test]
    fn horizontal_scrollbar_drag_uses_x_delta() {
        let event = DragEvent {
            delta_x: 10.0,
            delta_y: 999.0,
            ..DragEvent::default()
        };

        assert_eq!(drag_offset(ScrollAxis::X, event, 20.0, 200.0, 100.0), 40.0);
    }
}
