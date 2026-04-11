//! Keyboard and mouse input state tracking.
//!
//! Uses fixed-size bit arrays instead of `HashSet` for zero-allocation
//! per-frame updates. The runner keeps a live input snapshot and copies it
//! into the world resource with a plain struct assignment each frame.

/// Total number of key codes (must cover all `KeyCode` variants).
const KEY_COUNT: usize = 49;

/// Tracks keyboard and mouse state per frame.
///
/// Stored as an ECS resource. The runner updates it from a live snapshot
/// with a single struct copy per frame.
#[derive(Clone, Copy)]
pub struct Input {
    keys_held: [bool; KEY_COUNT],
    keys_pressed: [bool; KEY_COUNT],
    keys_released: [bool; KEY_COUNT],
    mouse_position: [f32; 2],
    mouse_position_prev: [f32; 2],
    cursor_in_window: bool,
    mouse_buttons: [bool; 3], // left, right, middle
    mouse_buttons_pressed: [bool; 3],
    mouse_buttons_released: [bool; 3],
    scroll_delta: [f32; 2],
}

impl Input {
    pub(crate) fn new() -> Self {
        Self {
            keys_held: [false; KEY_COUNT],
            keys_pressed: [false; KEY_COUNT],
            keys_released: [false; KEY_COUNT],
            mouse_position: [0.0; 2],
            mouse_position_prev: [0.0; 2],
            cursor_in_window: false,
            mouse_buttons: [false; 3],
            mouse_buttons_pressed: [false; 3],
            mouse_buttons_released: [false; 3],
            scroll_delta: [0.0; 2],
        }
    }

    /// Call at the start of each frame to clear one-shot events.
    pub(crate) fn begin_frame(&mut self) {
        self.keys_pressed = [false; KEY_COUNT];
        self.keys_released = [false; KEY_COUNT];
        self.mouse_buttons_pressed = [false; 3];
        self.mouse_buttons_released = [false; 3];
        self.mouse_position_prev = self.mouse_position;
        self.scroll_delta = [0.0; 2];
    }

    pub(crate) fn key_down(&mut self, key: KeyCode) {
        let idx = key as usize;
        if idx < KEY_COUNT && !self.keys_held[idx] {
            self.keys_held[idx] = true;
            self.keys_pressed[idx] = true;
        }
    }

    pub(crate) fn key_up(&mut self, key: KeyCode) {
        let idx = key as usize;
        if idx < KEY_COUNT {
            self.keys_held[idx] = false;
            self.keys_released[idx] = true;
        }
    }

    pub(crate) fn set_mouse_position(&mut self, x: f32, y: f32) {
        self.mouse_position = [x, y];
        self.cursor_in_window = true;
    }

    pub(crate) fn set_mouse_position_suppressed(&mut self, x: f32, y: f32) {
        self.mouse_position = [x, y];
        self.cursor_in_window = false;
    }

    pub(crate) fn set_cursor_in_window(&mut self, in_window: bool) {
        self.cursor_in_window = in_window;
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
            self.mouse_buttons_released[button] = true;
        }
    }

    pub(crate) fn add_scroll_delta(&mut self, dx: f32, dy: f32) {
        self.scroll_delta[0] += dx;
        self.scroll_delta[1] += dy;
    }

    pub(crate) fn suppress_key_down(&mut self, _key: KeyCode) {}

    pub(crate) fn suppress_key_up(&mut self, key: KeyCode) {
        let idx = key as usize;
        if idx < KEY_COUNT {
            self.keys_held[idx] = false;
        }
    }

    pub(crate) fn suppress_mouse_button_down(&mut self, _button: usize) {}

    pub(crate) fn suppress_mouse_button_up(&mut self, button: usize) {
        if button < 3 {
            self.mouse_buttons[button] = false;
        }
    }

    pub(crate) fn reset(&mut self) {
        self.keys_held = [false; KEY_COUNT];
        self.keys_pressed = [false; KEY_COUNT];
        self.keys_released = [false; KEY_COUNT];
        self.cursor_in_window = false;
        self.mouse_buttons = [false; 3];
        self.mouse_buttons_pressed = [false; 3];
        self.mouse_buttons_released = [false; 3];
        self.mouse_position_prev = self.mouse_position;
        self.scroll_delta = [0.0; 2];
    }

    // ── Public queries ──────────────────────────────────────────────────

    /// Is the key currently held down?
    #[inline]
    pub fn key_held(&self, key: KeyCode) -> bool {
        let idx = key as usize;
        idx < KEY_COUNT && self.keys_held[idx]
    }

    /// Was the key pressed this frame (one-shot)?
    #[inline]
    pub fn key_pressed(&self, key: KeyCode) -> bool {
        let idx = key as usize;
        idx < KEY_COUNT && self.keys_pressed[idx]
    }

    /// Was the key released this frame (one-shot)?
    #[inline]
    pub fn key_released(&self, key: KeyCode) -> bool {
        let idx = key as usize;
        idx < KEY_COUNT && self.keys_released[idx]
    }

    /// Current mouse position in logical pixels.
    #[inline]
    pub fn mouse_position(&self) -> [f32; 2] {
        self.mouse_position
    }

    /// Is the cursor currently inside the window client area?
    #[inline]
    pub fn mouse_in_window(&self) -> bool {
        self.cursor_in_window
    }

    /// Mouse movement delta since last frame.
    #[inline]
    pub fn mouse_delta(&self) -> [f32; 2] {
        [
            self.mouse_position[0] - self.mouse_position_prev[0],
            self.mouse_position[1] - self.mouse_position_prev[1],
        ]
    }

    /// Scroll wheel delta this frame `[horizontal, vertical]`.
    #[inline]
    pub fn scroll_delta(&self) -> [f32; 2] {
        self.scroll_delta
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

    /// Was the left mouse button released this frame?
    #[inline]
    pub fn mouse_left_released(&self) -> bool {
        self.mouse_buttons_released[0]
    }

    /// Was the right mouse button pressed this frame?
    #[inline]
    pub fn mouse_right_pressed(&self) -> bool {
        self.mouse_buttons_pressed[1]
    }

    /// Return all currently held keys.
    pub fn held_keys(&self) -> Vec<KeyCode> {
        ALL_KEY_CODES
            .iter()
            .copied()
            .filter(|k| self.key_held(*k))
            .collect()
    }
}

/// Re-exported subset of winit key codes for convenience.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum KeyCode {
    ArrowUp = 0,
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

const ALL_KEY_CODES: [KeyCode; KEY_COUNT] = [
    KeyCode::ArrowUp,
    KeyCode::ArrowDown,
    KeyCode::ArrowLeft,
    KeyCode::ArrowRight,
    KeyCode::Space,
    KeyCode::Enter,
    KeyCode::Escape,
    KeyCode::KeyA,
    KeyCode::KeyB,
    KeyCode::KeyC,
    KeyCode::KeyD,
    KeyCode::KeyE,
    KeyCode::KeyF,
    KeyCode::KeyG,
    KeyCode::KeyH,
    KeyCode::KeyI,
    KeyCode::KeyJ,
    KeyCode::KeyK,
    KeyCode::KeyL,
    KeyCode::KeyM,
    KeyCode::KeyN,
    KeyCode::KeyO,
    KeyCode::KeyP,
    KeyCode::KeyQ,
    KeyCode::KeyR,
    KeyCode::KeyS,
    KeyCode::KeyT,
    KeyCode::KeyU,
    KeyCode::KeyV,
    KeyCode::KeyW,
    KeyCode::KeyX,
    KeyCode::KeyY,
    KeyCode::KeyZ,
    KeyCode::Digit0,
    KeyCode::Digit1,
    KeyCode::Digit2,
    KeyCode::Digit3,
    KeyCode::Digit4,
    KeyCode::Digit5,
    KeyCode::Digit6,
    KeyCode::Digit7,
    KeyCode::Digit8,
    KeyCode::Digit9,
    KeyCode::ShiftLeft,
    KeyCode::ShiftRight,
    KeyCode::ControlLeft,
    KeyCode::ControlRight,
    KeyCode::Tab,
    KeyCode::Unknown,
];

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

#[cfg(test)]
mod tests {
    use super::{Input, KeyCode};

    #[test]
    fn suppressed_release_clears_held_without_emitting_one_shot() {
        let mut input = Input::new();
        input.key_down(KeyCode::KeyA);
        input.begin_frame();

        input.suppress_key_up(KeyCode::KeyA);

        assert!(!input.key_held(KeyCode::KeyA));
        assert!(!input.key_released(KeyCode::KeyA));
    }

    #[test]
    fn cursor_presence_tracks_window_membership() {
        let mut input = Input::new();
        assert!(!input.mouse_in_window());

        input.set_mouse_position(10.0, 20.0);
        assert!(input.mouse_in_window());

        input.set_cursor_in_window(false);
        assert!(!input.mouse_in_window());

        input.reset();
        assert!(!input.mouse_in_window());
    }

    #[test]
    fn suppressed_pointer_position_does_not_mark_cursor_as_game_visible() {
        let mut input = Input::new();
        input.set_mouse_position_suppressed(32.0, 48.0);

        assert_eq!(input.mouse_position(), [32.0, 48.0]);
        assert!(!input.mouse_in_window());
    }
}
