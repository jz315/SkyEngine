use crate::input::{Input, MouseButton};

use super::{PointerEvent, ScrollEvent};

pub(crate) use eui_neo_winit::{keyboard_from_key_event, merge_keyboard_event};

pub(crate) fn pointer_from_input(input: &Input) -> PointerEvent {
    let position = input.mouse_in_window().then(|| input.mouse_position());
    let [x, y] = position.unwrap_or_default();
    let [delta_x, delta_y] = input.mouse_delta();
    PointerEvent {
        x,
        y,
        delta_x,
        delta_y,
        position,
        delta: [delta_x, delta_y],
        down: input.mouse_button_held(MouseButton::Left),
        pressed_this_frame: input.mouse_button_pressed(MouseButton::Left),
        released_this_frame: input.mouse_button_released(MouseButton::Left),
        right_down: input.mouse_button_held(MouseButton::Right),
        right_pressed_this_frame: input.mouse_button_pressed(MouseButton::Right),
        right_released_this_frame: input.mouse_button_released(MouseButton::Right),
    }
}

pub(crate) fn scroll_from_input(input: &Input) -> ScrollEvent {
    let [x, y] = input.scroll_delta();
    ScrollEvent { x, y }
}
