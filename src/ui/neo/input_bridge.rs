use crate::input::{Input, KeyCode, MouseButton};
use winit::keyboard::ModifiersState;

use super::{KeyboardEvent, PointerEvent, ScrollEvent};

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

pub(crate) fn keyboard_from_key_event(
    event: &winit::event::KeyEvent,
    modifiers: ModifiersState,
) -> KeyboardEvent {
    let shift = modifiers.shift_key();
    let ctrl = modifiers.control_key() || modifiers.super_key();
    let physical_key = match event.physical_key {
        winit::keyboard::PhysicalKey::Code(code) => Some(KeyCode::from_winit(code)),
        _ => None,
    };
    let mut keyboard = KeyboardEvent {
        shift,
        select_all: ctrl && physical_key == Some(KeyCode::KeyA),
        copy: ctrl && physical_key == Some(KeyCode::KeyC),
        cut: ctrl && physical_key == Some(KeyCode::KeyX),
        ..KeyboardEvent::default()
    };

    if ctrl && physical_key == Some(KeyCode::KeyV) {
        keyboard.paste_text = clipboard_text().unwrap_or_default();
        return keyboard;
    }

    if let Some(key) = physical_key {
        keyboard.enter = key == KeyCode::Enter;
        keyboard.escape = key == KeyCode::Escape;
        keyboard.backspace = key == KeyCode::Backspace;
        keyboard.delete = key == KeyCode::Delete;
        keyboard.left = key == KeyCode::ArrowLeft;
        keyboard.right = key == KeyCode::ArrowRight;
        keyboard.home = key == KeyCode::Home;
        keyboard.end = key == KeyCode::End;
    }

    if !ctrl {
        if let Some(text) = event.text.as_ref() {
            let text = text.as_str();
            if text != "\r" && text != "\n" && text != "\u{8}" && text != "\u{7f}" {
                keyboard.text.push_str(text);
            }
        }
    }

    keyboard
}

pub(crate) fn merge_keyboard_event(base: &mut KeyboardEvent, extra: KeyboardEvent) {
    base.text.push_str(&extra.text);
    base.paste_text.push_str(&extra.paste_text);
    base.enter |= extra.enter;
    base.escape |= extra.escape;
    base.backspace |= extra.backspace;
    base.delete |= extra.delete;
    base.left |= extra.left;
    base.right |= extra.right;
    base.home |= extra.home;
    base.end |= extra.end;
    base.shift |= extra.shift;
    base.select_all |= extra.select_all;
    base.copy |= extra.copy;
    base.cut |= extra.cut;
}

fn clipboard_text() -> Option<String> {
    arboard::Clipboard::new().ok()?.get_text().ok()
}
