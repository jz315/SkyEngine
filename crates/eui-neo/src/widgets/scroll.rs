//! Scrollbar and scroll-container composition helpers.

use std::cell::RefCell;
use std::rc::Rc;

use crate::Color;

use super::super::{
    Align, AnimProperty, CursorShape, DragEvent, EdgeInsets, Response, Signal, Size, Transition, Ui,
};
use super::layout::WidgetLayout;
use super::theme::{self, ThemeColorTokens};

type ChangeCallback = Rc<RefCell<Box<dyn FnMut(f32)>>>;
type Change2Callback = Rc<RefCell<Box<dyn FnMut(f32, f32)>>>;

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
    x: Option<f32>,
    y: Option<f32>,
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
            x: None,
            y: None,
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

    pub(super) fn position(mut self, x: f32, y: f32) -> Self {
        self.x = Some(x);
        self.y = Some(y);
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

        let mut root = self
            .ui
            .stack(id.clone())
            .size(self.width, self.height)
            .z_index(self.z_index);
        if let Some(x) = self.x {
            root = root.x(x);
        }
        if let Some(y) = self.y {
            root = root.y(y);
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
        let viewport_extent = self
            .ui
            .previous_frame(&viewport_id)
            .map(|frame| match axis {
                ScrollAxis::X => frame.width,
                ScrollAxis::Y => frame.height,
            })
            .or_else(|| fixed_viewport_extent(self.layout, self.inset, axis))
            .unwrap_or(0.0);
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
        let scrollable = max_offset > 0.0 && viewport_extent > 0.0;
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

        pub fn offset_signal<T: 'static>(self, signal: Signal<T, f32>) -> Self {
            let owner = self.inner.id.clone();
            let value = self
                .inner
                .ui
                .with_dependency_owner(owner, |ui| signal.watch(ui));
            self.offset(value).on_change(move |next| signal.set(next))
        }

        pub fn value(self, value: f32) -> Self {
            self.offset(value)
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

macro_rules! scroll_xy_common_methods {
    () => {
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

        pub fn fill(mut self) -> Self {
            self.layout = self.layout.size(Size::fill(), Size::fill());
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

        /// Inset the clipped viewport and both scrollbars from the outer container.
        pub fn inset(mut self, value: f32) -> Self {
            self.inset = EdgeInsets::all(value);
            self
        }

        pub fn inset_xy(mut self, horizontal: f32, vertical: f32) -> Self {
            self.inset = EdgeInsets::symmetric(horizontal, vertical);
            self
        }

        pub fn inset_each(mut self, left: f32, top: f32, right: f32, bottom: f32) -> Self {
            self.inset = EdgeInsets::new(left, top, right, bottom);
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

        pub fn content_padding(self, value: f32) -> Self {
            self.padding(value)
        }

        pub fn content_padding_xy(self, horizontal: f32, vertical: f32) -> Self {
            self.padding_xy(horizontal, vertical)
        }

        pub fn content_padding_each(self, left: f32, top: f32, right: f32, bottom: f32) -> Self {
            self.padding_each(left, top, right, bottom)
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

fn fixed_viewport_extent(layout: WidgetLayout, inset: EdgeInsets, axis: ScrollAxis) -> Option<f32> {
    match axis {
        ScrollAxis::X => fixed_viewport_width(layout, inset),
        ScrollAxis::Y => fixed_viewport_height(layout, inset),
    }
}

fn fixed_viewport_width(layout: WidgetLayout, inset: EdgeInsets) -> Option<f32> {
    match layout.width {
        Size::Fixed(width) => Some((width - inset.horizontal()).max(0.0)),
        Size::WrapContent | Size::Fill => None,
    }
}

fn fixed_viewport_height(layout: WidgetLayout, inset: EdgeInsets) -> Option<f32> {
    match layout.height {
        Size::Fixed(height) => Some((height - inset.vertical()).max(0.0)),
        Size::WrapContent | Size::Fill => None,
    }
}

#[derive(Debug, Clone, Copy)]
struct ScrollXYMetrics {
    viewport_w: f32,
    viewport_h: f32,
    content_w: f32,
    content_h: f32,
    max_x: f32,
    max_y: f32,
    offset_x: f32,
    offset_y: f32,
    scrollable_x: bool,
    scrollable_y: bool,
    reserve_right: f32,
    reserve_bottom: f32,
    horizontal_track_w: f32,
    vertical_track_h: f32,
}

impl ScrollXYMetrics {
    fn resolve(builder: &ScrollXYBuilder<'_>, viewport_id: &str, content_id: &str) -> Self {
        let viewport = builder.ui.previous_frame(viewport_id);
        let viewport_w = viewport
            .map(|frame| frame.width)
            .or_else(|| fixed_viewport_width(builder.layout, builder.inset))
            .unwrap_or(0.0);
        let viewport_h = viewport
            .map(|frame| frame.height)
            .or_else(|| fixed_viewport_height(builder.layout, builder.inset))
            .unwrap_or(0.0);
        let measured_content = builder.ui.previous_frame(content_id);
        let content_w = builder
            .content_width
            .or_else(|| measured_content.map(|frame| frame.width))
            .unwrap_or(viewport_w)
            .max(viewport_w);
        let content_h = builder
            .content_height
            .or_else(|| measured_content.map(|frame| frame.height))
            .unwrap_or(viewport_h)
            .max(viewport_h);
        let max_x = (content_w - viewport_w).max(0.0);
        let max_y = (content_h - viewport_h).max(0.0);
        let offset_x = builder.offset_x.clamp(0.0, max_x);
        let offset_y = builder.offset_y.clamp(0.0, max_y);
        let scrollable_x = max_x > 0.0 && viewport_w > 0.0;
        let scrollable_y = max_y > 0.0 && viewport_h > 0.0;
        let reserve_right = if scrollable_y {
            builder.scrollbar_width + builder.scrollbar_gap
        } else {
            0.0
        };
        let reserve_bottom = if scrollable_x {
            builder.scrollbar_height + builder.scrollbar_gap
        } else {
            0.0
        };
        let vertical_track_h = if scrollable_x {
            (viewport_h - builder.scrollbar_height - builder.scrollbar_gap).max(0.0)
        } else {
            viewport_h
        };
        let horizontal_track_w = if scrollable_y {
            (viewport_w - builder.scrollbar_width - builder.scrollbar_gap).max(0.0)
        } else {
            viewport_w
        };

        Self {
            viewport_w,
            viewport_h,
            content_w,
            content_h,
            max_x,
            max_y,
            offset_x,
            offset_y,
            scrollable_x,
            scrollable_y,
            reserve_right,
            reserve_bottom,
            horizontal_track_w,
            vertical_track_h,
        }
    }

    fn any_scrollable(self) -> bool {
        self.scrollable_x || self.scrollable_y
    }

    fn wheel_offset(
        self,
        event: crate::ScrollEvent,
        step_x: f32,
        step_y: f32,
    ) -> Option<(f32, f32)> {
        let next_x = (self.offset_x - event.x * step_x).clamp(0.0, self.max_x);
        let next_y = (self.offset_y - event.y * step_y).clamp(0.0, self.max_y);
        (next_x != self.offset_x || next_y != self.offset_y).then_some((next_x, next_y))
    }
}

pub struct ScrollXYBuilder<'ui> {
    ui: &'ui mut Ui,
    id: String,
    layout: WidgetLayout,
    style: ScrollbarStyle,
    offset_x: f32,
    offset_y: f32,
    content_width: Option<f32>,
    content_height: Option<f32>,
    step_x: f32,
    step_y: f32,
    inset: EdgeInsets,
    padding: EdgeInsets,
    scrollbar_width: f32,
    scrollbar_height: f32,
    scrollbar_gap: f32,
    on_change: Option<Change2Callback>,
}

impl<'ui> ScrollXYBuilder<'ui> {
    pub fn new(ui: &'ui mut Ui, id: impl Into<String>) -> Self {
        Self {
            ui,
            id: id.into(),
            layout: WidgetLayout::new(320.0, 240.0),
            style: ScrollbarStyle::default(),
            offset_x: 0.0,
            offset_y: 0.0,
            content_width: None,
            content_height: None,
            step_x: 48.0,
            step_y: 48.0,
            inset: EdgeInsets::ZERO,
            padding: EdgeInsets::ZERO,
            scrollbar_width: 8.0,
            scrollbar_height: 8.0,
            scrollbar_gap: 8.0,
            on_change: None,
        }
    }

    scroll_xy_common_methods!();

    pub fn content_width(mut self, value: f32) -> Self {
        self.content_width = Some(value.max(0.0));
        self
    }

    pub fn content_height(mut self, value: f32) -> Self {
        self.content_height = Some(value.max(0.0));
        self
    }

    pub fn content_size(mut self, width: f32, height: f32) -> Self {
        self.content_width = Some(width.max(0.0));
        self.content_height = Some(height.max(0.0));
        self
    }

    pub fn auto_content_width(mut self) -> Self {
        self.content_width = None;
        self
    }

    pub fn auto_content_height(mut self) -> Self {
        self.content_height = None;
        self
    }

    pub fn auto_content_size(mut self) -> Self {
        self.content_width = None;
        self.content_height = None;
        self
    }

    pub fn offset(mut self, x: f32, y: f32) -> Self {
        self.offset_x = x.max(0.0);
        self.offset_y = y.max(0.0);
        self
    }

    pub fn offset_x(mut self, value: f32) -> Self {
        self.offset_x = value.max(0.0);
        self
    }

    pub fn offset_y(mut self, value: f32) -> Self {
        self.offset_y = value.max(0.0);
        self
    }

    pub fn offset_signal<T: 'static>(self, signal: Signal<T, (f32, f32)>) -> Self {
        let owner = self.id.clone();
        let (x, y) = self.ui.with_dependency_owner(owner, |ui| signal.watch(ui));
        self.offset(x, y)
            .on_change(move |next_x, next_y| signal.set((next_x, next_y)))
    }

    pub fn value(self, x: f32, y: f32) -> Self {
        self.offset(x, y)
    }

    pub fn step(mut self, value: f32) -> Self {
        let value = value.max(1.0);
        self.step_x = value;
        self.step_y = value;
        self
    }

    pub fn step_xy(mut self, x: f32, y: f32) -> Self {
        self.step_x = x.max(1.0);
        self.step_y = y.max(1.0);
        self
    }

    pub fn scrollbar_width(mut self, value: f32) -> Self {
        self.scrollbar_width = value.max(0.0);
        self
    }

    pub fn scrollbar_height(mut self, value: f32) -> Self {
        self.scrollbar_height = value.max(0.0);
        self
    }

    pub fn scrollbar_size(mut self, value: f32) -> Self {
        let value = value.max(0.0);
        self.scrollbar_width = value;
        self.scrollbar_height = value;
        self
    }

    pub fn on_change<F>(mut self, callback: F) -> Self
    where
        F: FnMut(f32, f32) + 'static,
    {
        let next: Change2Callback = Rc::new(RefCell::new(Box::new(callback)));
        self.on_change = Some(if let Some(existing) = self.on_change.take() {
            Rc::new(RefCell::new(Box::new(move |x, y| {
                (existing.borrow_mut())(x, y);
                (next.borrow_mut())(x, y);
            })))
        } else {
            next
        });
        self
    }

    pub fn content(self, content: impl FnOnce(&mut Ui)) -> Response {
        let id = self.id.clone();
        let viewport_id = format!("{id}.viewport");
        let content_id = format!("{id}.content");
        let metrics = ScrollXYMetrics::resolve(&self, &viewport_id, &content_id);
        let on_wheel_change = self.on_change.clone();
        let on_horizontal_change = self.on_change.clone();
        let on_vertical_change = self.on_change.clone();
        let padding = self.padding;
        let style = self.style;
        let step_x = self.step_x;
        let step_y = self.step_y;
        let scrollbar_width = self.scrollbar_width;
        let scrollbar_height = self.scrollbar_height;
        let content_width = self
            .content_width
            .map(Size::fixed)
            .unwrap_or_else(Size::wrap_content);
        let content_height = self
            .content_height
            .map(Size::fixed)
            .unwrap_or_else(Size::wrap_content);

        self.layout
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
            )
            .content(|ui| {
                let mut viewport = ui.stack(viewport_id).fill().clip();
                if metrics.any_scrollable() {
                    viewport = viewport.on_scroll(move |event| {
                        if let Some(callback) = &on_wheel_change {
                            if let Some((next_x, next_y)) =
                                metrics.wheel_offset(event, step_x, step_y)
                            {
                                (callback.borrow_mut())(next_x, next_y);
                            }
                        }
                    });
                }

                viewport.content(|ui| {
                    ui.stack(content_id)
                        .position(-metrics.offset_x, -metrics.offset_y)
                        .size(content_width, content_height)
                        .padding_each(
                            padding.left,
                            padding.top,
                            padding.right + metrics.reserve_right,
                            padding.bottom + metrics.reserve_bottom,
                        )
                        .content(content);
                });

                build_xy_scrollbars(
                    ui,
                    &id,
                    metrics,
                    ScrollXYChrome {
                        style,
                        step_x,
                        step_y,
                        scrollbar_width,
                        scrollbar_height,
                    },
                    on_horizontal_change,
                    on_vertical_change,
                );
            })
    }
}

#[derive(Debug, Clone, Copy)]
struct ScrollXYChrome {
    style: ScrollbarStyle,
    step_x: f32,
    step_y: f32,
    scrollbar_width: f32,
    scrollbar_height: f32,
}

fn build_xy_scrollbars(
    ui: &mut Ui,
    id: &str,
    metrics: ScrollXYMetrics,
    chrome: ScrollXYChrome,
    on_horizontal_change: Option<Change2Callback>,
    on_vertical_change: Option<Change2Callback>,
) {
    if metrics.scrollable_x && chrome.scrollbar_height > 0.0 {
        scrollbar(ui, format!("{id}.scrollbar.x"))
            .horizontal()
            .position(0.0, (metrics.viewport_h - chrome.scrollbar_height).max(0.0))
            .size(metrics.horizontal_track_w, chrome.scrollbar_height)
            .viewport(metrics.viewport_w)
            .content(metrics.content_w)
            .offset(metrics.offset_x)
            .step(chrome.step_x)
            .style(chrome.style)
            .on_change(move |next_x| {
                if let Some(callback) = &on_horizontal_change {
                    (callback.borrow_mut())(next_x, metrics.offset_y);
                }
            })
            .build();
    }

    if metrics.scrollable_y && chrome.scrollbar_width > 0.0 {
        scrollbar(ui, format!("{id}.scrollbar.y"))
            .position((metrics.viewport_w - chrome.scrollbar_width).max(0.0), 0.0)
            .size(chrome.scrollbar_width, metrics.vertical_track_h)
            .viewport(metrics.viewport_h)
            .content(metrics.content_h)
            .offset(metrics.offset_y)
            .step(chrome.step_y)
            .style(chrome.style)
            .on_change(move |next_y| {
                if let Some(callback) = &on_vertical_change {
                    (callback.borrow_mut())(metrics.offset_x, next_y);
                }
            })
            .build();
    }

    if metrics.scrollable_x
        && metrics.scrollable_y
        && chrome.scrollbar_width > 0.0
        && chrome.scrollbar_height > 0.0
    {
        ui.rect(format!("{id}.scrollbar.corner"))
            .position(
                (metrics.viewport_w - chrome.scrollbar_width).max(0.0),
                (metrics.viewport_h - chrome.scrollbar_height).max(0.0),
            )
            .size(chrome.scrollbar_width, chrome.scrollbar_height)
            .color(chrome.style.track)
            .radius(chrome.style.radius)
            .build();
    }
}

pub fn scroll_xy(ui: &mut Ui, id: impl Into<String>) -> ScrollXYBuilder<'_> {
    ScrollXYBuilder::new(ui, id)
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
