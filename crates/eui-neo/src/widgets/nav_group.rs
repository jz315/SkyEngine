//! Single-selection navigation group backed by one application signal.

use crate::{Response, Signal, Size, Transition, Ui};

use super::button;
use super::layout::WidgetLayout;
use super::theme::{self, ThemeColorTokens};

const DEFAULT_WIDTH: f32 = 240.0;
const DEFAULT_HEIGHT: f32 = 210.0;
const DEFAULT_ITEM_HEIGHT: f32 = 58.0;
const DEFAULT_GAP: f32 = 12.0;
const DEFAULT_RADIUS: f32 = 18.0;

#[derive(Debug, Clone)]
struct NavItem<V> {
    value: V,
    icon: Option<u32>,
    label: String,
}

pub struct NavGroupBuilder<'ui> {
    ui: &'ui mut Ui,
    id: String,
    layout: WidgetLayout,
    item_height: f32,
    gap: f32,
    radius: f32,
    icon_size: f32,
    font_size: f32,
    tokens: ThemeColorTokens,
    transition: Transition,
}

impl<'ui> NavGroupBuilder<'ui> {
    pub fn new(ui: &'ui mut Ui, id: impl Into<String>) -> Self {
        Self {
            ui,
            id: id.into(),
            layout: WidgetLayout::new(DEFAULT_WIDTH, DEFAULT_HEIGHT).width(Size::Fill),
            item_height: DEFAULT_ITEM_HEIGHT,
            gap: DEFAULT_GAP,
            radius: DEFAULT_RADIUS,
            icon_size: 16.0,
            font_size: 16.0,
            tokens: theme::dark_theme_colors(),
            transition: Transition::responsive(),
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

    pub fn item_height(mut self, value: f32) -> Self {
        self.item_height = value.max(0.0);
        self
    }

    pub fn gap(mut self, value: f32) -> Self {
        self.gap = value.max(0.0);
        self
    }

    pub fn radius(mut self, value: f32) -> Self {
        self.radius = value.max(0.0);
        self
    }

    pub fn icon_size(mut self, value: f32) -> Self {
        self.icon_size = value.max(0.0);
        self
    }

    pub fn font_size(mut self, value: f32) -> Self {
        self.font_size = value.max(0.0);
        self
    }

    pub fn theme(mut self, tokens: ThemeColorTokens) -> Self {
        self.tokens = tokens;
        self
    }

    pub fn transition(mut self, value: Transition) -> Self {
        self.transition = value;
        self
    }

    pub fn signal<T: 'static, V: Clone + PartialEq + 'static>(
        self,
        signal: Signal<T, V>,
    ) -> BoundNavGroupBuilder<'ui, T, V> {
        BoundNavGroupBuilder {
            ui: self.ui,
            id: self.id,
            layout: self.layout,
            item_height: self.item_height,
            gap: self.gap,
            radius: self.radius,
            icon_size: self.icon_size,
            font_size: self.font_size,
            tokens: self.tokens,
            transition: self.transition,
            signal,
            items: Vec::new(),
        }
    }
}

pub struct BoundNavGroupBuilder<'ui, T, V> {
    ui: &'ui mut Ui,
    id: String,
    layout: WidgetLayout,
    item_height: f32,
    gap: f32,
    radius: f32,
    icon_size: f32,
    font_size: f32,
    tokens: ThemeColorTokens,
    transition: Transition,
    signal: Signal<T, V>,
    items: Vec<NavItem<V>>,
}

impl<'ui, T: 'static, V: Clone + PartialEq + 'static> BoundNavGroupBuilder<'ui, T, V> {
    pub fn item(mut self, value: V, label: impl Into<String>) -> Self {
        self.items.push(NavItem {
            value,
            icon: None,
            label: label.into(),
        });
        self
    }

    pub fn item_icon(mut self, value: V, icon: u32, label: impl Into<String>) -> Self {
        self.items.push(NavItem {
            value,
            icon: Some(icon),
            label: label.into(),
        });
        self
    }

    pub fn build(self) -> Response {
        let id = self.id;
        let item_id_prefix = id.clone();
        let selected = self
            .ui
            .with_dependency_owner(&id, |ui| self.signal.watch(ui));
        let signal = self.signal;
        let items = self.items;
        let item_height = self.item_height;
        let gap = self.gap;
        let radius = self.radius;
        let icon_size = self.icon_size;
        let font_size = self.font_size;
        let tokens = self.tokens;
        let transition = self.transition;

        self.layout
            .apply_to_size(
                self.ui.column(id.clone()),
                self.layout.width,
                self.layout.height,
            )
            .gap(gap)
            .content(move |ui| {
                for (index, item) in items.into_iter().enumerate() {
                    let selected = selected == item.value;
                    let next_value = item.value.clone();
                    let item_signal = signal.clone();
                    let mut button = button(ui, format!("{item_id_prefix}.{index}"))
                        .size(Size::fill(), item_height)
                        .text(item.label)
                        .font_size(font_size)
                        .secondary_theme(tokens)
                        .selected(selected)
                        .transition(transition)
                        .radius(radius)
                        .on_click(move || item_signal.set(next_value.clone()));
                    if let Some(icon) = item.icon {
                        button = button.icon_codepoint(icon).icon_size(icon_size);
                    }
                    button.build();
                }
            });

        self.ui.response(&id)
    }
}

pub fn nav_group(ui: &mut Ui, id: impl Into<String>) -> NavGroupBuilder<'_> {
    NavGroupBuilder::new(ui, id)
}
