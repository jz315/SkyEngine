//! Port of `EUI-NEO/components/timepicker.h`.

use std::cell::RefCell;
use std::rc::Rc;

use rustc_hash::FxHashMap;

use crate::Color;

use super::super::{
    AnimProperty, HorizontalAlign, LayoutRect, OutsideClickPolicy, PointerEvent, Response, Shadow,
    Signal, Transition, Ui, VerticalAlign,
};
use super::popover::{popover, PopoverPlacement};
use super::theme::{self, ThemeColorTokens};

type TimeChangeCallback = Rc<RefCell<Box<dyn FnMut(i32, i32)>>>;
type OpenChangeCallback = Rc<RefCell<Box<dyn FnMut(bool)>>>;

thread_local! {
    static TIME_DRAFTS: RefCell<FxHashMap<String, TimeDraft>> = RefCell::new(FxHashMap::default());
    static TIME_DRAG_STATES: RefCell<FxHashMap<String, DragState>> = RefCell::new(FxHashMap::default());
}

#[derive(Debug, Clone, Copy)]
pub struct TimePickerStyle {
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

impl TimePickerStyle {
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

impl Default for TimePickerStyle {
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

#[derive(Debug, Clone, Copy, Default)]
struct TimeDraft {
    active: bool,
    hour: i32,
    minute: i32,
}

#[derive(Debug, Clone, Copy)]
struct WheelItemVisual {
    opacity: f32,
    translate_y: f32,
    scale_x: f32,
    scale_y: f32,
}

pub struct TimePickerBuilder<'ui> {
    ui: &'ui mut Ui,
    id: String,
    style: TimePickerStyle,
    transition: Transition,
    on_change: Option<TimeChangeCallback>,
    on_open_change: Option<OpenChangeCallback>,
    hour: i32,
    minute: i32,
    minute_step: i32,
    screen_width: f32,
    screen_height: f32,
    width: f32,
    height: f32,
    open: bool,
    z_index: i32,
}

impl<'ui> TimePickerBuilder<'ui> {
    pub fn new(ui: &'ui mut Ui, id: impl Into<String>) -> Self {
        Self {
            ui,
            id: id.into(),
            style: TimePickerStyle::default(),
            transition: Transition::smooth(),
            on_change: None,
            on_open_change: None,
            hour: 9,
            minute: 30,
            minute_step: 1,
            screen_width: 800.0,
            screen_height: 600.0,
            width: 330.0,
            height: 264.0,
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

    pub fn time(mut self, hour: i32, minute: i32) -> Self {
        self.hour = hour.clamp(0, 23);
        self.minute = minute.clamp(0, 59);
        self
    }

    pub fn value_signal<T: 'static>(self, signal: Signal<T, [i32; 2]>) -> Self {
        let owner = self.id.clone();
        let [hour, minute] = self.ui.with_dependency_owner(owner, |ui| signal.watch(ui));
        self.time(hour, minute)
            .on_change(move |hour, minute| signal.set([hour, minute]))
    }

    pub fn minute_step(mut self, value: i32) -> Self {
        self.minute_step = value.clamp(1, 30);
        self
    }

    pub fn style(mut self, value: TimePickerStyle) -> Self {
        self.style = value;
        self
    }

    pub fn theme(mut self, tokens: ThemeColorTokens) -> Self {
        self.style = TimePickerStyle::new(tokens);
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
        F: FnMut(i32, i32) + 'static,
    {
        let next: TimeChangeCallback = Rc::new(RefCell::new(Box::new(callback)));
        self.on_change = Some(if let Some(existing) = self.on_change.take() {
            Rc::new(RefCell::new(Box::new(move |hour, minute| {
                (existing.borrow_mut())(hour, minute);
                (next.borrow_mut())(hour, minute);
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
        let panel_width = self.width.min((self.screen_width - 48.0).max(0.0));
        let panel_height = self.height.min((self.screen_height - 48.0).max(0.0));
        let panel_x = 24.0_f32.max((self.screen_width - panel_width) * 0.5);
        let panel_y = 24.0_f32.max((self.screen_height - panel_height) * 0.5);
        let visible = if self.open { 1.0 } else { 0.0 };
        let panel_scale = if self.open { 1.0 } else { 0.965 };
        let panel_offset_y = if self.open { 0.0 } else { 14.0 };
        let draft = sync_time_draft(&id, self.open, self.hour, self.minute);
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
                        time_panel(
                            ui,
                            &id,
                            panel_width,
                            panel_height,
                            self.open,
                            self.style,
                            self.transition,
                            draft,
                            self.hour,
                            self.minute,
                            self.minute_step,
                            self.on_change.clone(),
                            self.on_open_change.clone(),
                        );
                    });
            });

        self.ui.response(&id)
    }
}

pub fn time_picker(ui: &mut Ui, id: impl Into<String>) -> TimePickerBuilder<'_> {
    TimePickerBuilder::new(ui, id)
}

#[allow(clippy::too_many_arguments)]
fn time_panel(
    ui: &mut Ui,
    id: &str,
    width: f32,
    height: f32,
    open: bool,
    style: TimePickerStyle,
    transition: Transition,
    draft: TimeDraft,
    committed_hour: i32,
    committed_minute: i32,
    minute_step: i32,
    on_change: Option<TimeChangeCallback>,
    on_open_change: Option<OpenChangeCallback>,
) {
    let title_height = 58.0;
    let bottom_pad = 24.0;
    let row_height = 38.0;
    let column_y = title_height + 8.0;
    let column_height = 150.0_f32.max(height - title_height - bottom_pad - 8.0);
    let gap = 12.0;
    let pad = 24.0;
    let column_width = 1.0_f32.max((width - pad * 2.0 - gap * 2.0) / 3.0);

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
        .text("Time")
        .font_size(24.0)
        .line_height(29.0)
        .color(style.text)
        .build();

    let done_id = id.to_string();
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
            let draft = time_draft(&done_id);
            if draft.hour != committed_hour || draft.minute != committed_minute {
                call_time_change(&done_on_change, draft.hour, draft.minute);
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

    for column in 0..3 {
        let x = pad + column as f32 * (column_width + gap);
        time_wheel_column(
            ui,
            id,
            column,
            x,
            column_y,
            column_width,
            column_height,
            row_height,
            open,
            style,
            transition,
            draft,
            minute_step,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn time_wheel_column(
    ui: &mut Ui,
    id: &str,
    column: i32,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    row_height: f32,
    open: bool,
    style: TimePickerStyle,
    transition: Transition,
    draft: TimeDraft,
    minute_step: i32,
) {
    let column_id = format!("{id}.column.{column}");
    let value = time_column_value(column, draft.hour, draft.minute, minute_step);

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

    let press_column_id = column_id.clone();
    let press_draft_id = id.to_string();
    let drag_column_id = column_id.clone();
    let drag_draft_id = id.to_string();
    let scroll_draft_id = id.to_string();
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
            set_time_drag_state(
                &press_column_id,
                DragState {
                    bounds,
                    start_y: y,
                    start_value: value,
                },
            );
            apply_time_column_value(
                &press_draft_id,
                column,
                value + row_offset_from_pointer(y, bounds, height, row_height),
                minute_step,
            );
        })
        .on_drag(move |event| {
            let state = time_drag_state(&drag_column_id);
            let scale = if height > 0.0 {
                state.bounds.height / height
            } else {
                1.0
            };
            let delta = ((state.start_y - event.y) / scale.max(0.001) / row_height).round() as i32;
            apply_time_column_value(
                &drag_draft_id,
                column,
                state.start_value + delta,
                minute_step,
            );
        })
        .on_scroll(move |event| {
            if event.y.abs() > 0.001 {
                let draft = time_draft(&scroll_draft_id);
                let current_value =
                    time_column_value(column, draft.hour, draft.minute, minute_step);
                apply_time_column_value(
                    &scroll_draft_id,
                    column,
                    current_value + if event.y > 0.0 { -1 } else { 1 },
                    minute_step,
                );
            }
        })
        .build();

    for offset in -2_i32..=2 {
        let active = offset == 0;
        let distance = offset.abs();
        let visual = wheel_item_visual(offset);
        ui.text(format!("{column_id}.item.{}", offset + 2))
            .x(x + 4.0)
            .y(y + height * 0.5 - row_height * 0.5 + offset as f32 * row_height)
            .size((width - 8.0).max(0.0), row_height)
            .z_index(10 - distance)
            .text(time_item_text(column, value, offset, minute_step))
            .font_size(if active { 24.0 } else { 16.0 })
            .line_height(if active { 28.0 } else { 20.0 })
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

fn sync_time_draft(id: &str, open: bool, hour: i32, minute: i32) -> TimeDraft {
    TIME_DRAFTS.with(|drafts| {
        let mut drafts = drafts.borrow_mut();
        let draft = drafts.entry(id.to_string()).or_default();
        if !open || !draft.active {
            draft.hour = hour.clamp(0, 23);
            draft.minute = minute.clamp(0, 59);
            draft.active = open;
        }
        *draft
    })
}

fn time_draft(id: &str) -> TimeDraft {
    TIME_DRAFTS.with(|drafts| drafts.borrow().get(id).copied().unwrap_or_default())
}

fn set_time_drag_state(id: &str, state: DragState) {
    TIME_DRAG_STATES.with(|states| {
        states.borrow_mut().insert(id.to_string(), state);
    });
}

fn time_drag_state(id: &str) -> DragState {
    TIME_DRAG_STATES.with(|states| states.borrow().get(id).copied().unwrap_or_default())
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

fn two_digits(value: i32) -> String {
    format!("{:02}", value.clamp(0, 99))
}

fn resolved_minute_step(step: i32) -> i32 {
    step.clamp(1, 30)
}

fn minute_count(step: i32) -> i32 {
    let safe_step = resolved_minute_step(step);
    1.max((60 + safe_step - 1) / safe_step)
}

fn hour12(hour: i32) -> i32 {
    let value = hour % 12;
    if value == 0 {
        12
    } else {
        value
    }
}

fn pm(hour: i32) -> bool {
    hour >= 12
}

fn time_column_value(column: i32, hour: i32, minute: i32, step: i32) -> i32 {
    match column {
        0 => hour12(hour),
        1 => minute / resolved_minute_step(step),
        _ => {
            if pm(hour) {
                1
            } else {
                0
            }
        }
    }
}

fn time_item_text(column: i32, value: i32, offset: i32, step: i32) -> String {
    match column {
        0 => wrap_value(value + offset, 1, 12).to_string(),
        1 => {
            let index = wrap_value(value + offset, 0, minute_count(step) - 1);
            two_digits((index * resolved_minute_step(step)).clamp(0, 59))
        }
        _ => {
            if wrap_value(value + offset, 0, 1) == 1 {
                "PM".to_string()
            } else {
                "AM".to_string()
            }
        }
    }
}

fn apply_time_column_value(id: &str, column: i32, value: i32, step: i32) {
    TIME_DRAFTS.with(|drafts| {
        let mut drafts = drafts.borrow_mut();
        let draft = drafts.entry(id.to_string()).or_default();
        let mut next_hour = draft.hour.clamp(0, 23);
        let mut next_minute = draft.minute.clamp(0, 59);
        if column == 0 {
            let next_hour_12 = wrap_value(value, 1, 12);
            next_hour = if pm(next_hour) {
                if next_hour_12 == 12 {
                    12
                } else {
                    next_hour_12 + 12
                }
            } else if next_hour_12 == 12 {
                0
            } else {
                next_hour_12
            };
        } else if column == 1 {
            let index = wrap_value(value, 0, minute_count(step) - 1);
            next_minute = (index * resolved_minute_step(step)).clamp(0, 59);
        } else {
            let next_pm = wrap_value(value, 0, 1) == 1;
            let current_hour_12 = hour12(next_hour);
            next_hour = if next_pm {
                if current_hour_12 == 12 {
                    12
                } else {
                    current_hour_12 + 12
                }
            } else if current_hour_12 == 12 {
                0
            } else {
                current_hour_12
            };
        }
        draft.hour = next_hour;
        draft.minute = next_minute;
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

fn wheel_item_visual(offset: i32) -> WheelItemVisual {
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

fn call_time_change(callback: &Option<TimeChangeCallback>, hour: i32, minute: i32) {
    if let Some(callback) = callback {
        (callback.borrow_mut())(hour, minute);
    }
}

fn call_open_change(callback: &Option<OpenChangeCallback>, open: bool) {
    if let Some(callback) = callback {
        (callback.borrow_mut())(open);
    }
}
