//! winit adapters for `eui-neo`.

use eui_neo::KeyboardEvent;
use winit::keyboard::{KeyCode, ModifiersState, PhysicalKey};

#[derive(Debug, Clone, Copy, Default)]
pub struct KeyboardOptions<'a> {
    pub paste_text: Option<&'a str>,
}

/// Convert a pressed winit key event into an `eui-neo` keyboard snapshot.
pub fn keyboard_from_key_event(
    event: &winit::event::KeyEvent,
    modifiers: ModifiersState,
) -> KeyboardEvent {
    keyboard(event, modifiers, KeyboardOptions::default())
}

/// Convert a pressed winit key event into an `eui-neo` keyboard snapshot.
pub fn keyboard(
    event: &winit::event::KeyEvent,
    modifiers: ModifiersState,
    options: KeyboardOptions<'_>,
) -> KeyboardEvent {
    let shift = modifiers.shift_key();
    let ctrl = modifiers.control_key() || modifiers.super_key();
    let physical_key = match event.physical_key {
        PhysicalKey::Code(code) => Some(code),
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
        keyboard.paste_text = options.paste_text.unwrap_or_default().to_string();
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

/// Merge another keyboard snapshot into `base`, preserving accumulated text and
/// one-frame command flags.
pub fn merge_keyboard_event(base: &mut KeyboardEvent, extra: KeyboardEvent) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_keyboard_event_accumulates_text_and_flags() {
        let mut base = KeyboardEvent {
            text: "a".to_string(),
            left: true,
            ..KeyboardEvent::default()
        };
        merge_keyboard_event(
            &mut base,
            KeyboardEvent {
                text: "b".to_string(),
                enter: true,
                shift: true,
                ..KeyboardEvent::default()
            },
        );

        assert_eq!(base.text, "ab");
        assert!(base.left);
        assert!(base.enter);
        assert!(base.shift);
    }
}
