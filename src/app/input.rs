//! Keyboard and mouse input state tracking.

use std::collections::HashSet;

/// Tracks keyboard and mouse state per frame.
///
/// Updated by the [`AppRunner`] from winit events. Available as a resource
/// in the ECS world.
#[derive(Clone)]
pub struct Input {
    keys_held: HashSet<KeyCode>,
    keys_pressed: HashSet<KeyCode>,
    keys_released: HashSet<KeyCode>,
    mouse_position: [f32; 2],
    mouse_buttons: [bool; 3], // left, right, middle
    mouse_buttons_pressed: [bool; 3],
}

impl Input {
    pub(crate) fn new() -> Self {
        Self {
            keys_held: HashSet::new(),
            keys_pressed: HashSet::new(),
            keys_released: HashSet::new(),
            mouse_position: [0.0; 2],
            mouse_buttons: [false; 3],
            mouse_buttons_pressed: [false; 3],
        }
    }

    /// Call at the start of each frame to clear one-shot events.
    pub(crate) fn begin_frame(&mut self) {
        self.keys_pressed.clear();
        self.keys_released.clear();
        self.mouse_buttons_pressed = [false; 3];
    }

    pub(crate) fn key_down(&mut self, key: KeyCode) {
        if self.keys_held.insert(key) {
            self.keys_pressed.insert(key);
        }
    }

    pub(crate) fn key_up(&mut self, key: KeyCode) {
        self.keys_held.remove(&key);
        self.keys_released.insert(key);
    }

    pub(crate) fn set_mouse_position(&mut self, x: f32, y: f32) {
        self.mouse_position = [x, y];
    }

    pub(crate) fn mouse_button_down(&mut self, button: usize) {
        if button < 3 {
            self.mouse_buttons[button] = true;
            self.mouse_buttons_pressed[button] = true;
        }
    }

    pub(crate) fn mouse_button_up(&mut self, button: usize) {
        if button < 3 {
            self.mouse_buttons[button] = false;
        }
    }

    // ── Public queries ──────────────────────────────────────────────────

    /// Is the key currently held down?
    #[inline]
    pub fn key_held(&self, key: KeyCode) -> bool {
        self.keys_held.contains(&key)
    }

    /// Was the key pressed this frame (one-shot)?
    #[inline]
    pub fn key_pressed(&self, key: KeyCode) -> bool {
        self.keys_pressed.contains(&key)
    }

    /// Was the key released this frame (one-shot)?
    #[inline]
    pub fn key_released(&self, key: KeyCode) -> bool {
        self.keys_released.contains(&key)
    }

    /// Current mouse position in logical pixels.
    #[inline]
    pub fn mouse_position(&self) -> [f32; 2] {
        self.mouse_position
    }

    /// Is the left mouse button held?
    #[inline]
    pub fn mouse_left(&self) -> bool {
        self.mouse_buttons[0]
    }

    /// Is the right mouse button held?
    #[inline]
    pub fn mouse_right(&self) -> bool {
        self.mouse_buttons[1]
    }

    /// Was the left mouse button pressed this frame?
    #[inline]
    pub fn mouse_left_pressed(&self) -> bool {
        self.mouse_buttons_pressed[0]
    }

    /// Return a copy of all currently held keys.
    pub fn held_keys(&self) -> Vec<KeyCode> {
        self.keys_held.iter().copied().collect()
    }
}

/// Re-exported subset of winit key codes for convenience.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyCode {
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Space,
    Enter,
    Escape,
    KeyA,
    KeyB,
    KeyC,
    KeyD,
    KeyE,
    KeyF,
    KeyG,
    KeyH,
    KeyI,
    KeyJ,
    KeyK,
    KeyL,
    KeyM,
    KeyN,
    KeyO,
    KeyP,
    KeyQ,
    KeyR,
    KeyS,
    KeyT,
    KeyU,
    KeyV,
    KeyW,
    KeyX,
    KeyY,
    KeyZ,
    Digit0,
    Digit1,
    Digit2,
    Digit3,
    Digit4,
    Digit5,
    Digit6,
    Digit7,
    Digit8,
    Digit9,
    ShiftLeft,
    ShiftRight,
    ControlLeft,
    ControlRight,
    Tab,
    Unknown,
}

impl KeyCode {
    /// Convert from winit's `KeyCode`.
    pub fn from_winit(key: winit::keyboard::KeyCode) -> Self {
        use winit::keyboard::KeyCode as W;
        match key {
            W::ArrowUp => Self::ArrowUp,
            W::ArrowDown => Self::ArrowDown,
            W::ArrowLeft => Self::ArrowLeft,
            W::ArrowRight => Self::ArrowRight,
            W::Space => Self::Space,
            W::Enter => Self::Enter,
            W::Escape => Self::Escape,
            W::KeyA => Self::KeyA,
            W::KeyB => Self::KeyB,
            W::KeyC => Self::KeyC,
            W::KeyD => Self::KeyD,
            W::KeyE => Self::KeyE,
            W::KeyF => Self::KeyF,
            W::KeyG => Self::KeyG,
            W::KeyH => Self::KeyH,
            W::KeyI => Self::KeyI,
            W::KeyJ => Self::KeyJ,
            W::KeyK => Self::KeyK,
            W::KeyL => Self::KeyL,
            W::KeyM => Self::KeyM,
            W::KeyN => Self::KeyN,
            W::KeyO => Self::KeyO,
            W::KeyP => Self::KeyP,
            W::KeyQ => Self::KeyQ,
            W::KeyR => Self::KeyR,
            W::KeyS => Self::KeyS,
            W::KeyT => Self::KeyT,
            W::KeyU => Self::KeyU,
            W::KeyV => Self::KeyV,
            W::KeyW => Self::KeyW,
            W::KeyX => Self::KeyX,
            W::KeyY => Self::KeyY,
            W::KeyZ => Self::KeyZ,
            W::Digit0 => Self::Digit0,
            W::Digit1 => Self::Digit1,
            W::Digit2 => Self::Digit2,
            W::Digit3 => Self::Digit3,
            W::Digit4 => Self::Digit4,
            W::Digit5 => Self::Digit5,
            W::Digit6 => Self::Digit6,
            W::Digit7 => Self::Digit7,
            W::Digit8 => Self::Digit8,
            W::Digit9 => Self::Digit9,
            W::ShiftLeft => Self::ShiftLeft,
            W::ShiftRight => Self::ShiftRight,
            W::ControlLeft => Self::ControlLeft,
            W::ControlRight => Self::ControlRight,
            W::Tab => Self::Tab,
            _ => Self::Unknown,
        }
    }
}
