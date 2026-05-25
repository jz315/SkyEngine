//! Port of `EUI-NEO/components/input.h`.

use std::cell::RefCell;
use std::rc::Rc;

use rustc_hash::FxHashMap;

use crate::Color;

use super::super::{
    AnimProperty, Binding, KeyboardEvent, LayoutRect, PointerEvent, Response, Shadow, Size,
    Transition, Ui, VerticalAlign,
};
use super::layout::WidgetLayout;
use super::text::measure_text_width;
use super::theme::{self, ThemeColorTokens};

const DEFAULT_WIDTH: f32 = 260.0;
const DEFAULT_HEIGHT: f32 = 40.0;

type ChangeCallback = Rc<RefCell<Box<dyn FnMut(&str)>>>;
type EnterCallback = Rc<RefCell<Box<dyn FnMut()>>>;
type FocusCallback = Rc<RefCell<Box<dyn FnMut(bool)>>>;

thread_local! {
    static INPUT_STATES: RefCell<FxHashMap<String, InputState>> = RefCell::new(FxHashMap::default());
}

#[derive(Debug, Clone, Copy)]
pub struct InputStyle {
    pub background: Color,
    pub hover: Color,
    pub focused: Color,
    pub pressed: Color,
    pub border: Color,
    pub focus_border: Color,
    pub text: Color,
    pub placeholder: Color,
    pub cursor: Color,
    pub shadow: Shadow,
    pub radius: f32,
}

impl InputStyle {
    pub fn new(tokens: ThemeColorTokens) -> Self {
        Self {
            background: tokens.surface,
            hover: tokens.surface_hover,
            focused: theme::resolve_field_fill(tokens, tokens.surface, 0.20, 0.70),
            pressed: tokens.surface_active,
            border: theme::with_opacity(tokens.border, 0.78),
            focus_border: theme::with_alpha(tokens.primary, 0.86),
            text: tokens.text,
            placeholder: theme::with_opacity(tokens.text, 0.45),
            cursor: tokens.primary,
            shadow: theme::popup_shadow(tokens),
            radius: 10.0,
        }
    }
}

impl Default for InputStyle {
    fn default() -> Self {
        Self::new(theme::dark_theme_colors())
    }
}

pub struct InputBuilder<'ui> {
    ui: &'ui mut Ui,
    id: String,
    style: InputStyle,
    transition: Transition,
    on_change: Option<ChangeCallback>,
    on_enter: Option<EnterCallback>,
    on_focus: Option<FocusCallback>,
    text: String,
    placeholder: String,
    multiline: bool,
    layout: WidgetLayout,
    inset: f32,
    font_size: f32,
}

impl<'ui> InputBuilder<'ui> {
    pub fn new(ui: &'ui mut Ui, id: impl Into<String>) -> Self {
        Self {
            ui,
            id: id.into(),
            style: InputStyle::default(),
            transition: Transition::snappy(),
            on_change: None,
            on_enter: None,
            on_focus: None,
            text: String::new(),
            placeholder: "Input".to_string(),
            multiline: false,
            layout: WidgetLayout::new(DEFAULT_WIDTH, DEFAULT_HEIGHT),
            inset: 12.0,
            font_size: 17.0,
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

    pub fn text(mut self, value: impl Into<String>) -> Self {
        self.text = value.into();
        self
    }

    pub fn text_bind<T: 'static>(self, binding: Binding<T, String>) -> Self {
        let value = binding.get();
        self.text(value)
            .on_change(move |next| binding.set(next.to_string()))
    }

    pub fn value(self, value: impl Into<String>) -> Self {
        self.text(value)
    }

    pub fn value_bind<T: 'static>(self, binding: Binding<T, String>) -> Self {
        self.text_bind(binding)
    }

    pub fn placeholder(mut self, value: impl Into<String>) -> Self {
        self.placeholder = value.into();
        self
    }

    pub fn multiline(mut self, value: bool) -> Self {
        self.multiline = value;
        self
    }

    pub fn font_size(mut self, value: f32) -> Self {
        self.font_size = value.max(1.0);
        self
    }

    pub fn inset(mut self, value: f32) -> Self {
        self.inset = value.max(0.0);
        self
    }

    pub fn style(mut self, value: InputStyle) -> Self {
        self.style = value;
        self
    }

    pub fn theme(mut self, tokens: ThemeColorTokens) -> Self {
        self.style = InputStyle::new(tokens);
        self
    }

    pub fn transition(mut self, value: Transition) -> Self {
        self.transition = value;
        self
    }

    pub fn on_change<F>(mut self, callback: F) -> Self
    where
        F: FnMut(&str) + 'static,
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

    pub fn on_enter<F>(mut self, callback: F) -> Self
    where
        F: FnMut() + 'static,
    {
        self.on_enter = Some(Rc::new(RefCell::new(Box::new(callback))));
        self
    }

    pub fn on_focus<F>(mut self, callback: F) -> Self
    where
        F: FnMut(bool) + 'static,
    {
        self.on_focus = Some(Rc::new(RefCell::new(Box::new(callback))));
        self
    }

    pub fn build(self) -> Response {
        let id = self.id.clone();
        let hit_id = format!("{id}.hit");
        let focused = self.ui.is_focused(&hit_id);
        let width = self.layout.fixed_width_or(DEFAULT_WIDTH);
        let height = self.layout.fixed_height_or(DEFAULT_HEIGHT);
        let text_width = (width - self.inset * 2.0).max(0.0);
        let allow_multiline = self.multiline;
        let on_change = self.on_change.clone();
        let on_enter = self.on_enter.clone();
        let on_focus = self.on_focus.clone();
        let text_line_height = self.font_size;
        let text_y = if self.multiline {
            self.inset
        } else {
            ((height - text_line_height) * 0.5).max(0.0)
        };
        let text_height = if self.multiline {
            (height - self.inset * 2.0).max(0.0)
        } else {
            text_line_height
        };
        let line_y = text_y;
        let inset = self.inset;
        let font_size = self.font_size;

        let snapshot = INPUT_STATES.with(|states| {
            let mut states = states.borrow_mut();
            let state = states.entry(id.clone()).or_default();
            if state.text != self.text {
                state.text = self.text.clone();
                state.cursor = clamp_utf8_boundary(&state.text, state.text.len());
                state.selection_start = state.cursor;
                state.selection_end = state.cursor;
                state.horizontal_scroll = 0.0;
            }
            state.cursor = clamp_utf8_boundary(&state.text, state.cursor);
            state.selection_start = clamp_utf8_boundary(&state.text, state.selection_start);
            state.selection_end = clamp_utf8_boundary(&state.text, state.selection_end);
            sync_scroll(state, text_width, font_size);
            state.clone()
        });

        let empty = snapshot.text.is_empty();
        let cursor_x = (self.inset
            + measure_width(&snapshot.text, 0, snapshot.cursor, self.font_size)
            - snapshot.horizontal_scroll)
            .clamp(self.inset, self.inset.max(width - self.inset - 2.0));
        let selection = selection_range(&snapshot);
        let has_selection = selection.0 != selection.1;
        let selection_x = self.inset
            + measure_width(&snapshot.text, 0, selection.0, self.font_size)
            - snapshot.horizontal_scroll;
        let selection_w = measure_width(&snapshot.text, selection.0, selection.1, self.font_size);
        let display_text = if empty {
            self.placeholder.clone()
        } else {
            snapshot.text.clone()
        };

        self.ui
            .stack(id.clone())
            .size(self.layout.width, self.layout.height)
            .clip()
            .min_width(self.layout.min_width)
            .max_width(self.layout.max_width)
            .min_height(self.layout.min_height)
            .max_height(self.layout.max_height)
            .grow(self.layout.grow)
            .margin_each(
                self.layout.margin.left,
                self.layout.margin.top,
                self.layout.margin.right,
                self.layout.margin.bottom,
            )
            .content(|ui| {
                let press_id = id.clone();
                let drag_id = id.clone();
                let input_id = id.clone();
                let focus_callback = on_focus.clone();
                let change_callback = on_change.clone();
                let enter_callback = on_enter.clone();

                ui.rect(hit_id.clone())
                    .fill()
                    .states(
                        if focused {
                            self.style.focused
                        } else {
                            self.style.background
                        },
                        self.style.hover,
                        self.style.pressed,
                    )
                    .radius(self.style.radius)
                    .border(
                        1.0,
                        if focused {
                            self.style.focus_border
                        } else {
                            self.style.border
                        },
                    )
                    .shadow_style(if focused {
                        self.style.shadow
                    } else {
                        Shadow::default()
                    })
                    .transition(self.transition)
                    .animate(AnimProperty::COLOR | AnimProperty::BORDER | AnimProperty::SHADOW)
                    .focusable(true)
                    .ime_rect(cursor_x, text_y, 1.5, text_line_height)
                    .on_press(move |event, bounds| {
                        INPUT_STATES.with(|states| {
                            let mut states = states.borrow_mut();
                            let state = states.entry(press_id.clone()).or_default();
                            state.last_bounds = bounds;
                            state.cursor = cursor_from_pointer(
                                state,
                                pointer_x(event),
                                bounds,
                                inset,
                                font_size,
                            );
                            clear_selection(state);
                            state.drag_anchor = state.cursor;
                            state.selecting = true;
                        });
                    })
                    .on_focus_changed(move |value| {
                        if let Some(callback) = &focus_callback {
                            (callback.borrow_mut())(value);
                        }
                    })
                    .on_drag(move |event| {
                        INPUT_STATES.with(|states| {
                            let mut states = states.borrow_mut();
                            let state = states.entry(drag_id.clone()).or_default();
                            state.cursor = cursor_from_pointer(
                                state,
                                event.x,
                                state.last_bounds,
                                inset,
                                font_size,
                            );
                            state.selection_start = state.drag_anchor;
                            state.selection_end = state.cursor;
                            let viewport_width = (state.last_bounds.width - inset * 2.0).max(0.0);
                            sync_scroll(state, viewport_width, font_size);
                        });
                    })
                    .on_text_input(move |event| {
                        INPUT_STATES.with(|states| {
                            let mut states = states.borrow_mut();
                            let state = states.entry(input_id.clone()).or_default();
                            let changed = apply_keyboard_event(
                                state,
                                &event,
                                allow_multiline,
                                enter_callback.as_ref(),
                                inset,
                                font_size,
                            );
                            if changed {
                                if let Some(callback) = &change_callback {
                                    (callback.borrow_mut())(&state.text);
                                }
                            }
                        });
                    })
                    .build();

                if has_selection {
                    ui.rect(format!("{id}.selection"))
                        .position(selection_x.max(self.inset), line_y)
                        .size(
                            1.0_f32.max(
                                selection_w.min(width - self.inset - selection_x.max(self.inset)),
                            ),
                            text_line_height,
                        )
                        .color(theme::with_alpha(self.style.cursor, 0.24))
                        .radius(3.0)
                        .build();
                }

                ui.text(format!("{id}.text"))
                    .position(self.inset - snapshot.horizontal_scroll, text_y)
                    .size(Size::fill(), text_height)
                    .text(display_text)
                    .font_size(self.font_size)
                    .line_height(text_line_height)
                    .color(if empty {
                        self.style.placeholder
                    } else {
                        self.style.text
                    })
                    .wrap(self.multiline)
                    .vertical_align(VerticalAlign::Top)
                    .build();

                if focused {
                    ui.rect(format!("{id}.cursor"))
                        .position(cursor_x, ((height - self.font_size * 1.18) * 0.5).max(0.0))
                        .size(1.5, self.font_size * 1.18)
                        .color(self.style.cursor)
                        .radius(1.0)
                        .build();
                }
            })
    }
}

pub fn input(ui: &mut Ui, id: impl Into<String>) -> InputBuilder<'_> {
    InputBuilder::new(ui, id)
}

#[derive(Debug, Clone, Default)]
struct InputState {
    text: String,
    cursor: usize,
    selection_start: usize,
    selection_end: usize,
    drag_anchor: usize,
    selecting: bool,
    horizontal_scroll: f32,
    last_bounds: LayoutRect,
}

fn filtered_text(input: &str, multiline: bool) -> String {
    input
        .chars()
        .filter(|ch| multiline || (*ch != '\n' && *ch != '\r'))
        .collect()
}

fn prev_utf8_index(value: &str, index: usize) -> usize {
    let mut out = index.min(value.len());
    if out == 0 {
        return 0;
    }
    out -= 1;
    while out > 0 && !value.is_char_boundary(out) {
        out -= 1;
    }
    out
}

fn next_utf8_index(value: &str, index: usize) -> usize {
    let mut out = index.min(value.len());
    if out >= value.len() {
        return value.len();
    }
    out += 1;
    while out < value.len() && !value.is_char_boundary(out) {
        out += 1;
    }
    out
}

fn clamp_utf8_boundary(value: &str, index: usize) -> usize {
    let mut out = index.min(value.len());
    while out > 0 && out < value.len() && !value.is_char_boundary(out) {
        out -= 1;
    }
    out
}

fn selection_range(state: &InputState) -> (usize, usize) {
    (
        state.selection_start.min(state.selection_end),
        state.selection_start.max(state.selection_end),
    )
}

fn has_text_selection(state: &InputState) -> bool {
    state.selection_start != state.selection_end
}

fn clear_selection(state: &mut InputState) {
    state.selection_start = state.cursor;
    state.selection_end = state.cursor;
    state.drag_anchor = state.cursor;
}

fn erase_selection(state: &mut InputState) {
    let range = selection_range(state);
    if range.0 == range.1 {
        return;
    }
    state.text.replace_range(range.0..range.1, "");
    state.cursor = range.0;
    clear_selection(state);
}

fn insert_at_cursor(state: &mut InputState, value: &str) {
    if value.is_empty() {
        return;
    }
    if has_text_selection(state) {
        erase_selection(state);
    }
    state.text.insert_str(state.cursor, value);
    state.cursor += value.len();
    clear_selection(state);
}

fn move_cursor(state: &mut InputState, direction: i32, keep_selection: bool) {
    let previous = state.cursor;
    if !keep_selection && has_text_selection(state) {
        let range = selection_range(state);
        state.cursor = if direction < 0 { range.0 } else { range.1 };
        clear_selection(state);
        return;
    }
    state.cursor = if direction < 0 {
        prev_utf8_index(&state.text, state.cursor)
    } else {
        next_utf8_index(&state.text, state.cursor)
    };
    if keep_selection {
        if !has_text_selection(state) {
            state.selection_start = previous;
        }
        state.selection_end = state.cursor;
    } else {
        clear_selection(state);
    }
}

fn move_cursor_to(state: &mut InputState, position: usize, keep_selection: bool) {
    let previous = state.cursor;
    state.cursor = clamp_utf8_boundary(&state.text, position);
    if keep_selection {
        if !has_text_selection(state) {
            state.selection_start = previous;
        }
        state.selection_end = state.cursor;
    } else {
        clear_selection(state);
    }
}

fn measure_width(value: &str, start: usize, end: usize, font_size: f32) -> f32 {
    let clamped_start = clamp_utf8_boundary(value, start.min(value.len()));
    let clamped_end = clamp_utf8_boundary(value, end.clamp(clamped_start, value.len()));
    if clamped_end <= clamped_start {
        return 0.0;
    }
    measure_text_width(&value[clamped_start..clamped_end], "", font_size, 400)
}

fn cursor_from_pointer(
    state: &InputState,
    pointer_x: f32,
    bounds: LayoutRect,
    inset: f32,
    font_size: f32,
) -> usize {
    let local_x = pointer_x - bounds.x;
    let target = local_x - inset + state.horizontal_scroll;
    let mut cursor_x = 0.0;
    let mut index = 0;
    while index < state.text.len() {
        let next = next_utf8_index(&state.text, index);
        let advance = measure_width(&state.text, index, next, font_size);
        if target < cursor_x + advance * 0.5 {
            return index;
        }
        cursor_x += advance;
        index = next;
    }
    state.text.len()
}

fn sync_scroll(state: &mut InputState, viewport_width: f32, font_size: f32) {
    let text_width = measure_width(&state.text, 0, state.text.len(), font_size);
    let cursor_pixel = measure_width(&state.text, 0, state.cursor, font_size);
    if text_width <= viewport_width {
        state.horizontal_scroll = 0.0;
        return;
    }
    let right_safe = 8.0_f32.max(viewport_width - 10.0);
    if cursor_pixel - state.horizontal_scroll < 0.0 {
        state.horizontal_scroll = cursor_pixel;
    } else if cursor_pixel - state.horizontal_scroll > right_safe {
        state.horizontal_scroll = cursor_pixel - right_safe;
    }
    state.horizontal_scroll = state
        .horizontal_scroll
        .clamp(0.0, (text_width - viewport_width + 10.0).max(0.0));
}

fn apply_keyboard_event(
    state: &mut InputState,
    event: &KeyboardEvent,
    allow_multiline: bool,
    on_enter: Option<&EnterCallback>,
    inset: f32,
    font_size: f32,
) -> bool {
    let mut changed = false;

    if event.select_all {
        state.selection_start = 0;
        state.selection_end = state.text.len();
        state.cursor = state.selection_end;
    }
    if event.copy {
        copy_selection(state);
    }
    if event.cut && has_text_selection(state) {
        copy_selection(state);
        erase_selection(state);
        changed = true;
    }
    if event.left {
        move_cursor(state, -1, event.shift);
    }
    if event.right {
        move_cursor(state, 1, event.shift);
    }
    if event.home {
        move_cursor_to(state, 0, event.shift);
    }
    if event.end {
        move_cursor_to(state, state.text.len(), event.shift);
    }
    if event.delete {
        if has_text_selection(state) {
            erase_selection(state);
            changed = true;
        } else if state.cursor < state.text.len() {
            let next = next_utf8_index(&state.text, state.cursor);
            state.text.replace_range(state.cursor..next, "");
            changed = true;
        }
    }
    if event.backspace {
        if has_text_selection(state) {
            erase_selection(state);
            changed = true;
        } else if state.cursor > 0 {
            let previous = prev_utf8_index(&state.text, state.cursor);
            state.text.replace_range(previous..state.cursor, "");
            state.cursor = previous;
            clear_selection(state);
            changed = true;
        }
    }
    if !event.text.is_empty() {
        insert_at_cursor(state, &filtered_text(&event.text, allow_multiline));
        changed = true;
    }
    if !event.paste_text.is_empty() {
        insert_at_cursor(state, &filtered_text(&event.paste_text, allow_multiline));
        changed = true;
    }
    if event.enter {
        if allow_multiline {
            insert_at_cursor(state, "\n");
            changed = true;
        } else if let Some(callback) = on_enter {
            (callback.borrow_mut())();
        }
    }
    if event.escape {
        if let Some(callback) = on_enter {
            (callback.borrow_mut())();
        }
    }
    let width = if state.last_bounds.width > 0.0 {
        state.last_bounds.width
    } else {
        DEFAULT_WIDTH
    };
    sync_scroll(state, (width - inset * 2.0).max(0.0), font_size);
    changed
}

fn copy_selection(state: &InputState) {
    if !has_text_selection(state) {
        return;
    }
    let range = selection_range(state);
    let selected = state.text[range.0..range.1].to_string();
    let _ = selected;
}

fn pointer_x(event: PointerEvent) -> f32 {
    event
        .position()
        .map(|position| position[0])
        .unwrap_or(event.x)
}

#[cfg(test)]
mod tests {
    use super::{apply_keyboard_event, InputState};
    use crate::KeyboardEvent;

    #[test]
    fn input_backspace_removes_previous_utf8_scalar() {
        let mut state = InputState {
            text: "a中".to_string(),
            cursor: "a中".len(),
            selection_start: "a中".len(),
            selection_end: "a中".len(),
            ..InputState::default()
        };

        assert!(apply_keyboard_event(
            &mut state,
            &KeyboardEvent {
                backspace: true,
                ..KeyboardEvent::default()
            },
            false,
            None,
            12.0,
            17.0,
        ));
        assert_eq!(state.text, "a");
        assert_eq!(state.cursor, 1);
    }

    #[test]
    fn input_single_line_filters_newlines() {
        let mut state = InputState::default();

        assert!(apply_keyboard_event(
            &mut state,
            &KeyboardEvent {
                text: "a\nb\rc".to_string(),
                ..KeyboardEvent::default()
            },
            false,
            None,
            12.0,
            17.0,
        ));
        assert_eq!(state.text, "abc");
    }
}
