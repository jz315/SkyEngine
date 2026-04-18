//! Input source identifiers — the physical origins of input.

use super::raw::{KeyCode, MouseButton};

/// A physical input source that can be bound to an action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InputSource {
    /// A keyboard key.
    Key(KeyCode),
    /// A mouse button.
    Mouse(MouseButton),
    /// A mouse axis (delta movement or scroll).
    MouseAxis(MouseAxisKind),
    // Future: GamepadButton(u8), GamepadAxis(u8)
}

/// Mouse axis variants for analog bindings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseAxisKind {
    /// Horizontal mouse movement delta.
    DeltaX,
    /// Vertical mouse movement delta.
    DeltaY,
    /// Horizontal scroll wheel delta.
    ScrollX,
    /// Vertical scroll wheel delta.
    ScrollY,
}

/// Read the current raw value (0.0 or 1.0 for digital, analog for axes)
/// of an [`InputSource`] from the low-level [`Input`](super::raw::Input) state.
pub(crate) fn read_source(input: &super::raw::Input, source: InputSource) -> f32 {
    match source {
        InputSource::Key(key) => {
            if input.key_held(key) {
                1.0
            } else {
                0.0
            }
        }
        InputSource::Mouse(button) => {
            if input.mouse_button_held(button) {
                1.0
            } else {
                0.0
            }
        }
        InputSource::MouseAxis(axis) => match axis {
            MouseAxisKind::DeltaX => input.mouse_delta()[0],
            MouseAxisKind::DeltaY => input.mouse_delta()[1],
            MouseAxisKind::ScrollX => input.scroll_delta()[0],
            MouseAxisKind::ScrollY => input.scroll_delta()[1],
        },
    }
}
