//! Virtualized scrolling list helpers for large, fixed-height item sets.

use std::cell::RefCell;
use std::rc::Rc;

use super::layout::WidgetLayout;
use super::scroll::{scrollbar, ScrollbarStyle};
use super::theme::ThemeColorTokens;
use crate::{Align, EdgeInsets, Response, Signal, Size, Ui};

type ChangeCallback = Rc<RefCell<Box<dyn FnMut(f32)>>>;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Overscan {
    Pixels(f32),
    Items(usize),
}

impl Overscan {
    fn pixels(self, item_height: f32, gap: f32) -> f32 {
        match self {
            Overscan::Pixels(value) => value.max(0.0),
            Overscan::Items(count) => (item_height + gap).max(0.0) * count as f32,
        }
    }
}

/// Visible item span for a fixed-height virtual list.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct VirtualListRange {
    pub start: usize,
    pub end: usize,
    pub content_height: f32,
}

impl VirtualListRange {
    pub fn for_fixed_height(
        item_count: usize,
        item_height: f32,
        gap: f32,
        scroll_offset: f32,
        viewport_height: f32,
        overscan: f32,
        padding: EdgeInsets,
    ) -> Self {
        let item_height = item_height.max(0.0);
        let gap = gap.max(0.0);
        let item_area_height = fixed_height_content(item_count, item_height, gap);
        let content_height = padding.vertical() + item_area_height;
        if item_count == 0 || item_height <= 0.0 {
            return Self {
                start: 0,
                end: 0,
                content_height,
            };
        }

        let stride = item_height + gap;
        let visible_top = (scroll_offset.max(0.0) - overscan.max(0.0) - padding.top).max(0.0);
        let visible_bottom =
            (scroll_offset.max(0.0) + viewport_height.max(0.0) + overscan.max(0.0) - padding.top)
                .max(0.0);

        let mut start = (visible_top / stride).floor() as usize;
        start = start.min(item_count);
        while start < item_count && item_bottom(start, item_height, stride) < visible_top {
            start += 1;
        }

        let mut end = ((visible_bottom / stride).floor() as usize).saturating_add(1);
        end = end.min(item_count);
        while end < item_count && item_top(end, stride) <= visible_bottom {
            end += 1;
        }
        end = end.max(start).min(item_count);

        Self {
            start,
            end,
            content_height,
        }
    }

    pub fn len(self) -> usize {
        self.end.saturating_sub(self.start)
    }

    pub fn is_empty(self) -> bool {
        self.start >= self.end
    }
}

/// Item metadata passed to a virtual list row renderer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VirtualListItem {
    pub index: usize,
    pub y: f32,
    pub height: f32,
}

pub struct VirtualListBuilder<'ui> {
    ui: &'ui mut Ui,
    id: String,
    layout: WidgetLayout,
    style: ScrollbarStyle,
    item_count: usize,
    item_height: f32,
    offset: f32,
    step: f32,
    gap: f32,
    padding: EdgeInsets,
    overscan: Overscan,
    scrollbar_width: f32,
    scrollbar_gap: f32,
    on_change: Option<ChangeCallback>,
}

impl<'ui> VirtualListBuilder<'ui> {
    pub fn new(ui: &'ui mut Ui, id: impl Into<String>) -> Self {
        Self {
            ui,
            id: id.into(),
            layout: WidgetLayout::new(320.0, 240.0),
            style: ScrollbarStyle::default(),
            item_count: 0,
            item_height: 32.0,
            offset: 0.0,
            step: 48.0,
            gap: 0.0,
            padding: EdgeInsets::ZERO,
            overscan: Overscan::Pixels(160.0),
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

    pub fn item_count(mut self, value: usize) -> Self {
        self.item_count = value;
        self
    }

    pub fn item_height(mut self, value: f32) -> Self {
        self.item_height = value.max(0.0);
        self
    }

    pub fn row_height(self, value: f32) -> Self {
        self.item_height(value)
    }

    pub fn gap(mut self, value: f32) -> Self {
        self.gap = value.max(0.0);
        self
    }

    pub fn spacing(self, value: f32) -> Self {
        self.gap(value)
    }

    pub fn overscan(mut self, value: f32) -> Self {
        self.overscan = Overscan::Pixels(value.max(0.0));
        self
    }

    pub fn overscan_items(mut self, value: usize) -> Self {
        self.overscan = Overscan::Items(value);
        self
    }

    pub fn offset(mut self, value: f32) -> Self {
        self.offset = value.max(0.0);
        self
    }

    pub fn offset_signal<T: 'static>(self, signal: Signal<T, f32>) -> Self {
        let owner = self.id.clone();
        let value = self.ui.with_dependency_owner(owner, |ui| signal.watch(ui));
        self.offset(value).on_change(move |next| signal.set(next))
    }

    pub fn value(self, value: f32) -> Self {
        self.offset(value)
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

    pub fn content(self, mut render_item: impl FnMut(&mut Ui, VirtualListItem)) -> Response {
        let id = self.id.clone();
        let viewport_h = self.layout.fixed_height_or(240.0);
        let overscan_px = self.overscan.pixels(self.item_height, self.gap);
        let range = VirtualListRange::for_fixed_height(
            self.item_count,
            self.item_height,
            self.gap,
            self.offset,
            viewport_h,
            overscan_px,
            self.padding,
        );
        let max_offset = (range.content_height - viewport_h).max(0.0);
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
                    ui.stack(format!("{id}.content"))
                        .y(-offset)
                        .size(Size::fill(), range.content_height)
                        .padding_each(
                            self.padding.left,
                            self.padding.top,
                            self.padding.right + reserved_right,
                            self.padding.bottom,
                        )
                        .content(|ui| {
                            let stride = self.item_height + self.gap;
                            for index in range.start..range.end {
                                let y = item_top(index, stride);
                                let item = VirtualListItem {
                                    index,
                                    y,
                                    height: self.item_height,
                                };
                                ui.stack(format!("{id}.item.{index}"))
                                    .y(y)
                                    .size(Size::fill(), self.item_height)
                                    .content(|ui| render_item(ui, item));
                            }
                        });
                });

                if scrollable && self.scrollbar_width > 0.0 {
                    scrollbar(ui, format!("{id}.scrollbar"))
                        .size(self.scrollbar_width, viewport_h)
                        .viewport(viewport_h)
                        .content(range.content_height)
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

pub fn virtual_list(ui: &mut Ui, id: impl Into<String>) -> VirtualListBuilder<'_> {
    VirtualListBuilder::new(ui, id)
}

fn fixed_height_content(item_count: usize, item_height: f32, gap: f32) -> f32 {
    if item_count == 0 || item_height <= 0.0 {
        0.0
    } else {
        item_count as f32 * item_height + item_count.saturating_sub(1) as f32 * gap.max(0.0)
    }
}

fn item_top(index: usize, stride: f32) -> f32 {
    index as f32 * stride
}

fn item_bottom(index: usize, item_height: f32, stride: f32) -> f32 {
    item_top(index, stride) + item_height
}

#[cfg(test)]
mod tests {
    use super::{virtual_list, VirtualListRange};
    use crate::{EdgeInsets, PointerEvent, Runtime, ScrollEvent, Size, State};

    #[derive(Default)]
    struct ListState {
        offset: f32,
    }

    #[test]
    fn fixed_height_range_tracks_visible_items() {
        let range = VirtualListRange::for_fixed_height(
            10_000,
            20.0,
            4.0,
            240.0,
            100.0,
            0.0,
            EdgeInsets::ZERO,
        );

        assert_eq!(range.start, 10);
        assert_eq!(range.end, 15);
        assert_eq!(range.len(), 5);
        assert_eq!(range.content_height, 239_996.0);
    }

    #[test]
    fn virtual_list_composes_only_visible_items() {
        let mut runtime = Runtime::new("page");
        runtime.compose(240.0, 120.0, |ui, _| {
            virtual_list(ui, "list")
                .size(160.0, 100.0)
                .item_count(1_000)
                .item_height(20.0)
                .gap(4.0)
                .offset(240.0)
                .overscan(0.0)
                .content(|ui, item| {
                    ui.rect(format!("row.{}", item.index))
                        .size(Size::fill(), item.height)
                        .build();
                });
        });

        assert!(runtime.find("list.item.10").is_some());
        assert!(runtime.find("list.item.14").is_some());
        assert!(runtime.find("list.item.0").is_none());
        assert!(runtime.find("list.item.999").is_none());
        assert_eq!(runtime.find("list.item.10").unwrap().frame.y, 0.0);
        assert_eq!(runtime.find("list.content").unwrap().frame.height, 23_996.0);
        assert!(runtime.find("list.scrollbar").is_some());
    }

    #[test]
    fn virtual_list_signal_writes_wheel_offset_to_state() {
        let state = State::new(ListState { offset: 24.0 });
        let compose_state = state.clone();
        let mut runtime = Runtime::new("page");
        runtime.compose(240.0, 120.0, move |ui, _| {
            let offset = compose_state.signal(
                "test.signal",
                |state| state.offset,
                |state, value| state.offset = value,
            );
            virtual_list(ui, "list")
                .size(160.0, 80.0)
                .item_count(100)
                .item_height(20.0)
                .gap(4.0)
                .offset_signal(offset)
                .step(10.0)
                .content(|ui, item| {
                    ui.rect(format!("row.{}", item.index))
                        .size(Size::fill(), item.height)
                        .build();
                });
        });

        runtime.update_pointer(PointerEvent::at(10.0, 10.0));
        runtime.update_scroll(ScrollEvent { x: 0.0, y: -2.0 });

        assert_eq!(state.read(|state| state.offset), 44.0);
    }
}
