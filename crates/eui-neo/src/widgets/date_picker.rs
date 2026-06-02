//! Port of `EUI-NEO/components/datepicker.h`.

use std::cell::RefCell;
use std::rc::Rc;

use rustc_hash::FxHashMap;

use crate::runtime::{LayerKind, NodeId};
use crate::Color;

use super::super::{
    AnimProperty, DragEvent, HorizontalAlign, LayoutRect, OutsideClickPolicy, PointerEvent,
    Response, ScrollEvent, Shadow, Signal, Transition, Ui, VerticalAlign,
};
use super::popover::{popover, PopoverPlacement};
use super::theme::{self, ThemeColorTokens};

type DateChangeCallback = Rc<RefCell<Box<dyn FnMut(i32, i32, i32)>>>;
type OpenChangeCallback = Rc<RefCell<Box<dyn FnMut(bool)>>>;

thread_local! {
    static DATE_DRAFTS: RefCell<FxHashMap<NodeId, DateDraft>> = RefCell::new(FxHashMap::default());
    static DATE_DRAG_STATES: RefCell<FxHashMap<NodeId, DragState>> = RefCell::new(FxHashMap::default());
}

#[derive(Debug, Clone, Copy)]
pub struct DatePickerStyle {
    pub backdrop: Color,
    pub surface: Color,
    pub column: Color,
    pub selected: Color,
    pub text: Color,
    pub muted_text: Color,
    pub accent: Color,
    pub border: Color,
    pub shadow: Shadow,
    pub radius: f32,
}

impl DatePickerStyle {
    pub fn new(tokens: ThemeColorTokens) -> Self {
        Self {
            backdrop: theme::color(0.0, 0.0, 0.0, if tokens.dark { 0.42 } else { 0.26 }),
            surface: if tokens.dark {
                theme::mix_color(tokens.surface, theme::color(0.0, 0.0, 0.0, 1.0), 0.14)
            } else {
                tokens.surface
            },
            column: theme::with_alpha(tokens.text, if tokens.dark { 0.045 } else { 0.035 }),
            selected: theme::with_alpha(tokens.primary, if tokens.dark { 0.18 } else { 0.12 }),
            text: tokens.text,
            muted_text: theme::with_opacity(tokens.text, 0.58),
            accent: tokens.primary,
            border: theme::with_opacity(tokens.border, 0.80),
            shadow: theme::popup_shadow(tokens),
            radius: 16.0,
        }
    }
}

impl Default for DatePickerStyle {
    fn default() -> Self {
        Self::new(theme::dark_theme_colors())
    }
}

#[derive(Debug, Clone, Copy)]
struct DragState {
    bounds: LayoutRect,
    start_y: f32,
    start_value: i32,
}

impl Default for DragState {
    fn default() -> Self {
        Self {
            bounds: LayoutRect::ZERO,
            start_y: 0.0,
            start_value: 0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct DateDraft {
    active: bool,
    year: i32,
    month: i32,
    day: i32,
}

impl Default for DateDraft {
    fn default() -> Self {
        Self {
            active: false,
            year: 2026,
            month: 1,
            day: 1,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct WheelItemVisual {
    opacity: f32,
    translate_y: f32,
    scale_x: f32,
    scale_y: f32,
}

pub struct DatePickerBuilder<'ui> {
    ui: &'ui mut Ui,
    id: String,
    style: DatePickerStyle,
    transition: Transition,
    on_change: Option<DateChangeCallback>,
    on_open_change: Option<OpenChangeCallback>,
    year: i32,
    month: i32,
    day: i32,
    screen_width: f32,
    screen_height: f32,
    width: f32,
    height: f32,
    open: bool,
    z_index: i32,
}

impl<'ui> DatePickerBuilder<'ui> {
    pub fn new(ui: &'ui mut Ui, id: impl Into<String>) -> Self {
        Self {
            ui,
            id: id.into(),
            style: DatePickerStyle::default(),
            transition: Transition::smooth(),
            on_change: None,
            on_open_change: None,
            year: 2026,
            month: 4,
            day: 28,
            screen_width: 800.0,
            screen_height: 600.0,
            width: 420.0,
            height: 270.0,
            open: false,
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
            .on_open_change(move |next| signal.set(next))
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

    pub fn date(mut self, year: i32, month: i32, day: i32) -> Self {
        self.year = year.clamp(1900, 2200);
        self.month = month.clamp(1, 12);
        self.day = day.clamp(1, days_in_month(self.year, self.month));
        self
    }

    pub fn value_signal<T: 'static>(self, signal: Signal<T, [i32; 3]>) -> Self {
        let owner = self.id.clone();
        let [year, month, day] = self.ui.with_dependency_owner(owner, |ui| signal.watch(ui));
        self.date(year, month, day)
            .on_change(move |year, month, day| signal.set([year, month, day]))
    }

    pub fn style(mut self, value: DatePickerStyle) -> Self {
        self.style = value;
        self
    }

    pub fn theme(mut self, tokens: ThemeColorTokens) -> Self {
        self.style = DatePickerStyle::new(tokens);
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
        F: FnMut(i32, i32, i32) + 'static,
    {
        let next: DateChangeCallback = Rc::new(RefCell::new(Box::new(callback)));
        self.on_change = Some(if let Some(existing) = self.on_change.take() {
            Rc::new(RefCell::new(Box::new(move |year, month, day| {
                (existing.borrow_mut())(year, month, day);
                (next.borrow_mut())(year, month, day);
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
        let state_id = NodeId::new(self.ui.resolve_id(&id));
        let panel_width = self.width.min((self.screen_width - 48.0).max(0.0));
        let panel_height = self.height.min((self.screen_height - 48.0).max(0.0));
        let panel_x = 24.0_f32.max((self.screen_width - panel_width) * 0.5);
        let panel_y = 24.0_f32.max((self.screen_height - panel_height) * 0.5);
        let visible = if self.open { 1.0 } else { 0.0 };
        let panel_scale = if self.open { 1.0 } else { 0.965 };
        let panel_offset_y = if self.open { 0.0 } else { 14.0 };
        let draft = sync_date_draft(&state_id, self.open, self.year, self.month, self.day);
        let open_change = self.on_open_change.clone();

        if self.open {
            self.ui.with_root_layer(|ui| {
                ui.rect(format!("{id}.backdrop"))
                    .position(0.0, 0.0)
                    .size(self.screen_width, self.screen_height)
                    .color(self.style.backdrop)
                    .opacity(visible)
                    .z_index(self.z_index)
                    .transition(self.transition)
                    .animate(AnimProperty::OPACITY)
                    .build();
            });
        }

        popover(self.ui, id.clone())
            .open(self.open)
            .layer_kind(LayerKind::Modal)
            .fallback_anchor(LayoutRect::new(panel_x, panel_y, 0.0, 0.0))
            .placement(PopoverPlacement::BottomStart)
            .gap(0.0)
            .size(panel_width, panel_height)
            .z_index(self.z_index + 1)
            .outside_click(OutsideClickPolicy::Close)
            .on_dismiss(move || call_open_change(&open_change, false))
            .content(|ui| {
                ui.stack(format!("{id}.panel"))
                    .size(panel_width, panel_height)
                    .opacity(visible)
                    .translate_y(panel_offset_y)
                    .scale(panel_scale)
                    .transform_origin(0.5, 0.5)
                    .transition(self.transition)
                    .animate(AnimProperty::OPACITY | AnimProperty::TRANSFORM)
                    .content(|ui| {
                        date_panel(
                            ui,
                            &id,
                            state_id.clone(),
                            panel_width,
                            panel_height,
                            self.open,
                            self.style,
                            self.transition,
                            draft,
                            self.year,
                            self.month,
                            self.day,
                            self.on_change.clone(),
                            self.on_open_change.clone(),
                        );
                    });
            });

        self.ui.response(&id)
    }
}

pub fn date_picker(ui: &mut Ui, id: impl Into<String>) -> DatePickerBuilder<'_> {
    DatePickerBuilder::new(ui, id)
}

fn date_panel(
    ui: &mut Ui,
    id: &str,
    state_id: NodeId,
    width: f32,
    height: f32,
    open: bool,
    style: DatePickerStyle,
    transition: Transition,
    draft: DateDraft,
    committed_year: i32,
    committed_month: i32,
    committed_day: i32,
    on_change: Option<DateChangeCallback>,
    on_open_change: Option<OpenChangeCallback>,
) {
    let title_height = 58.0;
    let bottom_pad = 24.0;
    let row_height = 38.0;
    let column_y = title_height + 8.0;
    let column_height = 150.0_f32.max(height - title_height - bottom_pad - 8.0);
    let gap = 12.0;
    let pad = 24.0;
    let month_width = 118.0_f32.max((width - pad * 2.0 - gap * 2.0) * 0.44);
    let day_width = 68.0_f32.max((width - pad * 2.0 - gap * 2.0) * 0.22);
    let year_width = 86.0_f32.max(width - pad * 2.0 - gap * 2.0 - month_width - day_width);

    ui.rect(format!("{id}.panel.bg"))
        .size(width, height)
        .color(style.surface)
        .radius(style.radius)
        .border(1.0, style.border)
        .shadow_style(style.shadow)
        .build();

    ui.rect(format!("{id}.panel.hit"))
        .size(width, height)
        .states(
            theme::color(0.0, 0.0, 0.0, 0.0),
            theme::color(0.0, 0.0, 0.0, 0.0),
            theme::color(0.0, 0.0, 0.0, 0.0),
        )
        .disabled(!open)
        .on_click(|| {})
        .on_scroll(|_| {})
        .build();

    ui.text(format!("{id}.title"))
        .x(24.0)
        .y(18.0)
        .size((width - 124.0).max(0.0), 30.0)
        .text("Date")
        .font_size(24.0)
        .line_height(29.0)
        .color(style.text)
        .build();

    let done_state_id = state_id.clone();
    let done_on_change = on_change.clone();
    let done_on_open_change = on_open_change.clone();
    ui.rect(format!("{id}.done.bg"))
        .x((width - 86.0).max(0.0))
        .y(18.0)
        .size(62.0, 30.0)
        .states(
            style.accent,
            theme::mix_color(style.accent, theme::color(1.0, 1.0, 1.0, 1.0), 0.12),
            theme::mix_color(style.accent, theme::color(0.0, 0.0, 0.0, 1.0), 0.14),
        )
        .radius(15.0)
        .disabled(!open)
        .on_click(move || {
            let draft = date_draft(&done_state_id);
            if draft.year != committed_year
                || draft.month != committed_month
                || draft.day != committed_day
            {
                call_date_change(&done_on_change, draft.year, draft.month, draft.day);
            }
            call_open_change(&done_on_open_change, false);
        })
        .build();

    ui.text(format!("{id}.done.text"))
        .x((width - 86.0).max(0.0))
        .y(18.0)
        .size(62.0, 30.0)
        .text("Done")
        .font_size(13.0)
        .line_height(16.0)
        .color(theme::color(1.0, 1.0, 1.0, 1.0))
        .horizontal_align(HorizontalAlign::Center)
        .vertical_align(VerticalAlign::Center)
        .build();

    date_wheel_column(
        ui,
        id,
        state_id.clone(),
        0,
        pad,
        column_y,
        month_width,
        column_height,
        row_height,
        open,
        style,
        transition,
        draft,
    );
    date_wheel_column(
        ui,
        id,
        state_id.clone(),
        1,
        pad + month_width + gap,
        column_y,
        day_width,
        column_height,
        row_height,
        open,
        style,
        transition,
        draft,
    );
    date_wheel_column(
        ui,
        id,
        state_id,
        2,
        pad + month_width + gap + day_width + gap,
        column_y,
        year_width,
        column_height,
        row_height,
        open,
        style,
        transition,
        draft,
    );
}

#[allow(clippy::too_many_arguments)]
fn date_wheel_column(
    ui: &mut Ui,
    id: &str,
    state_id: NodeId,
    column: i32,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    row_height: f32,
    open: bool,
    style: DatePickerStyle,
    transition: Transition,
    draft: DateDraft,
) {
    let column_id = format!("{id}.column.{column}");
    let column_state_id = NodeId::new(ui.resolve_id(&column_id));
    let value = column_value(column, draft.year, draft.month, draft.day);

    ui.rect(format!("{column_id}.bg"))
        .x(x)
        .y(y)
        .size(width, height)
        .color(style.column)
        .radius(style.radius)
        .build();

    ui.rect(format!("{column_id}.selected"))
        .x(x + 7.0)
        .y(y + height * 0.5 - row_height * 0.5)
        .size((width - 14.0).max(0.0), row_height)
        .color(style.selected)
        .radius(11.0)
        .build();

    let press_column_id = column_state_id.clone();
    let press_draft_id = state_id.clone();
    let drag_column_id = column_state_id;
    let drag_draft_id = state_id.clone();
    let scroll_draft_id = state_id;
    ui.rect(format!("{column_id}.hit"))
        .x(x)
        .y(y)
        .size(width, height)
        .states(
            theme::color(0.0, 0.0, 0.0, 0.0),
            theme::color(0.0, 0.0, 0.0, 0.0),
            theme::color(0.0, 0.0, 0.0, 0.0),
        )
        .disabled(!open)
        .on_press(move |event, bounds| {
            let y = pointer_y(event);
            set_date_drag_state(
                &press_column_id,
                DragState {
                    bounds,
                    start_y: y,
                    start_value: value,
                },
            );
            apply_date_column_value(
                &press_draft_id,
                column,
                value + row_offset_from_pointer(y, bounds, height, row_height),
            );
        })
        .on_drag(move |event| {
            let state = date_drag_state(&drag_column_id);
            let scale = if height > 0.0 {
                state.bounds.height / height
            } else {
                1.0
            };
            let delta = ((state.start_y - event.y) / scale.max(0.001) / row_height).round() as i32;
            apply_date_column_value(&drag_draft_id, column, state.start_value + delta);
        })
        .on_scroll(move |event| {
            if event.y.abs() > 0.001 {
                let draft = date_draft(&scroll_draft_id);
                let current_value = column_value(column, draft.year, draft.month, draft.day);
                apply_date_column_value(
                    &scroll_draft_id,
                    column,
                    current_value + if event.y > 0.0 { -1 } else { 1 },
                );
            }
        })
        .build();

    for offset in -2_i32..=2 {
        let text = item_text(column, value, offset, draft.year, draft.month);
        let active = offset == 0;
        let distance = offset.abs();
        let visual = wheel_item_visual(offset, text.is_empty());
        ui.text(format!("{column_id}.item.{}", offset + 2))
            .x(x + 4.0)
            .y(y + height * 0.5 - row_height * 0.5 + offset as f32 * row_height)
            .size((width - 8.0).max(0.0), row_height)
            .z_index(10 - distance)
            .text(text)
            .font_size(if active { 22.0 } else { 15.0 })
            .line_height(if active { 26.0 } else { 19.0 })
            .color(if active { style.text } else { style.muted_text })
            .opacity(visual.opacity)
            .translate_y(visual.translate_y)
            .scale_xy(visual.scale_x, visual.scale_y)
            .transform_origin(0.5, 0.5)
            .horizontal_align(HorizontalAlign::Center)
            .vertical_align(VerticalAlign::Center)
            .transition(transition)
            .animate(AnimProperty::TEXT_COLOR | AnimProperty::OPACITY | AnimProperty::TRANSFORM)
            .build();
    }
}

fn sync_date_draft(id: &NodeId, open: bool, year: i32, month: i32, day: i32) -> DateDraft {
    DATE_DRAFTS.with(|drafts| {
        let mut drafts = drafts.borrow_mut();
        let draft = drafts.entry(id.clone()).or_default();
        if !open || !draft.active {
            draft.year = year.clamp(1900, 2200);
            draft.month = month.clamp(1, 12);
            draft.day = day.clamp(1, days_in_month(draft.year, draft.month));
            draft.active = open;
        }
        *draft
    })
}

fn date_draft(id: &NodeId) -> DateDraft {
    DATE_DRAFTS.with(|drafts| drafts.borrow().get(id).copied().unwrap_or_default())
}

fn set_date_drag_state(id: &NodeId, state: DragState) {
    DATE_DRAG_STATES.with(|states| {
        states.borrow_mut().insert(id.clone(), state);
    });
}

fn date_drag_state(id: &NodeId) -> DragState {
    DATE_DRAG_STATES.with(|states| states.borrow().get(id).copied().unwrap_or_default())
}

#[cfg(test)]
pub(super) fn clear_date_picker_state_for_tests() {
    DATE_DRAFTS.with(|drafts| drafts.borrow_mut().clear());
    DATE_DRAG_STATES.with(|states| states.borrow_mut().clear());
}

#[cfg(test)]
pub(super) fn date_draft_keys_for_tests() -> Vec<NodeId> {
    DATE_DRAFTS.with(|drafts| {
        let mut keys: Vec<_> = drafts.borrow().keys().cloned().collect();
        keys.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        keys
    })
}

#[cfg(test)]
pub(super) fn date_drag_keys_for_tests() -> Vec<NodeId> {
    DATE_DRAG_STATES.with(|states| {
        let mut keys: Vec<_> = states.borrow().keys().cloned().collect();
        keys.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        keys
    })
}

fn leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_month(year: i32, month: i32) -> i32 {
    let days = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let month = month.clamp(1, 12);
    if month == 2 && leap_year(year) {
        29
    } else {
        days[(month - 1) as usize]
    }
}

fn wrap_value(value: i32, min_value: i32, max_value: i32) -> i32 {
    let span = max_value - min_value + 1;
    if span <= 0 {
        return min_value;
    }
    let mut shifted = (value - min_value) % span;
    if shifted < 0 {
        shifted += span;
    }
    min_value + shifted
}

fn month_name(month: i32) -> &'static str {
    const NAMES: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    NAMES[(month.clamp(1, 12) - 1) as usize]
}

fn column_value(column: i32, year: i32, month: i32, day: i32) -> i32 {
    match column {
        0 => month,
        1 => day,
        _ => year,
    }
}

fn item_text(column: i32, value: i32, offset: i32, year: i32, month: i32) -> String {
    match column {
        0 => month_name(wrap_value(value + offset, 1, 12)).to_string(),
        1 => wrap_value(value + offset, 1, days_in_month(year, month)).to_string(),
        _ => {
            let next_year = value + offset;
            if (1900..=2200).contains(&next_year) {
                next_year.to_string()
            } else {
                String::new()
            }
        }
    }
}

fn apply_date_column_value(id: &NodeId, column: i32, value: i32) {
    DATE_DRAFTS.with(|drafts| {
        let mut drafts = drafts.borrow_mut();
        let draft = drafts.entry(id.clone()).or_default();
        let mut next_year = draft.year.clamp(1900, 2200);
        let mut next_month = draft.month.clamp(1, 12);
        let mut next_day = draft.day.clamp(1, days_in_month(next_year, next_month));
        if column == 0 {
            next_month = wrap_value(value, 1, 12);
            next_day = next_day.min(days_in_month(next_year, next_month));
        } else if column == 1 {
            next_day = wrap_value(value, 1, days_in_month(next_year, next_month));
        } else {
            next_year = value.clamp(1900, 2200);
            next_day = next_day.min(days_in_month(next_year, next_month));
        }
        draft.year = next_year;
        draft.month = next_month;
        draft.day = next_day;
    });
}

fn row_offset_from_pointer(
    pointer_y: f32,
    bounds: LayoutRect,
    column_height: f32,
    row_height: f32,
) -> i32 {
    let scale = if column_height > 0.0 {
        bounds.height / column_height
    } else {
        1.0
    };
    let local_y = (pointer_y - bounds.y) / scale.max(0.001);
    ((local_y - column_height * 0.5) / row_height)
        .round()
        .clamp(-3.0, 3.0) as i32
}

fn wheel_item_visual(offset: i32, hidden: bool) -> WheelItemVisual {
    if hidden {
        return WheelItemVisual {
            opacity: 0.0,
            translate_y: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
        };
    }
    let distance = offset.abs() as f32;
    let direction = if offset < 0 {
        -1.0
    } else if offset > 0 {
        1.0
    } else {
        0.0
    };
    WheelItemVisual {
        opacity: (1.0 - distance * 0.25).clamp(0.48, 1.0),
        translate_y: -direction * distance * distance * 2.4,
        scale_x: (1.0 - distance * 0.015).clamp(0.95, 1.0),
        scale_y: (1.0 - distance * 0.120).clamp(0.72, 1.0),
    }
}

fn pointer_y(event: PointerEvent) -> f32 {
    event
        .position()
        .map(|position| position[1])
        .unwrap_or(event.y)
}

#[allow(dead_code)]
fn drag_delta_y(event: DragEvent) -> f32 {
    event.delta_y
}

#[allow(dead_code)]
fn scroll_active(event: ScrollEvent) -> bool {
    event.y.abs() > 0.001 || event.x.abs() > 0.001
}

fn call_date_change(callback: &Option<DateChangeCallback>, year: i32, month: i32, day: i32) {
    if let Some(callback) = callback {
        (callback.borrow_mut())(year, month, day);
    }
}

fn call_open_change(callback: &Option<OpenChangeCallback>, open: bool) {
    if let Some(callback) = callback {
        (callback.borrow_mut())(open);
    }
}
