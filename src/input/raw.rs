//! Raw keyboard and mouse input state tracking.
//!
//! Uses fixed-size bit arrays instead of `HashSet` for zero-allocation
//! per-frame updates. The runner keeps a live input snapshot and copies it
//! into the world resource with a plain struct assignment each frame.

/// Total number of key codes (must cover all `KeyCode` variants).
const KEY_COUNT: usize = 82;

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
    mouse_buttons: [bool; 5], // left, right, middle, back, forward
    mouse_buttons_pressed: [bool; 5],
    mouse_buttons_released: [bool; 5],
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
            mouse_buttons: [false; 5],
            mouse_buttons_pressed: [false; 5],
            mouse_buttons_released: [false; 5],
            scroll_delta: [0.0; 2],
        }
    }

    /// Call at the start of each frame to clear one-shot events.
    pub(crate) fn begin_frame(&mut self) {
        self.keys_pressed = [false; KEY_COUNT];
        self.keys_released = [false; KEY_COUNT];
        self.mouse_buttons_pressed = [false; 5];
        self.mouse_buttons_released = [false; 5];
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
        if button < 5 {
            self.mouse_buttons[button] = true;
            self.mouse_buttons_pressed[button] = true;
        }
    }

    pub(crate) fn mouse_button_up(&mut self, button: usize) {
        if button < 5 {
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
        if button < 5 {
            self.mouse_buttons[button] = false;
        }
    }

    pub(crate) fn reset(&mut self) {
        self.keys_held = [false; KEY_COUNT];
        self.keys_pressed = [false; KEY_COUNT];
        self.keys_released = [false; KEY_COUNT];
        self.cursor_in_window = false;
        self.mouse_buttons = [false; 5];
        self.mouse_buttons_pressed = [false; 5];
        self.mouse_buttons_released = [false; 5];
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

    /// Is a mouse button currently held?
    #[inline]
    pub fn mouse_button_held(&self, button: MouseButton) -> bool {
        let idx = button as usize;
        idx < 5 && self.mouse_buttons[idx]
    }

    /// Was a mouse button pressed this frame (one-shot)?
    #[inline]
    pub fn mouse_button_pressed(&self, button: MouseButton) -> bool {
        let idx = button as usize;
        idx < 5 && self.mouse_buttons_pressed[idx]
    }

    /// Was a mouse button released this frame (one-shot)?
    #[inline]
    pub fn mouse_button_released(&self, button: MouseButton) -> bool {
        let idx = button as usize;
        idx < 5 && self.mouse_buttons_released[idx]
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

// ── MouseButton ─────────────────────────────────────────────────────────────

/// Mouse button identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum MouseButton {
    Left = 0,
    Right = 1,
    Middle = 2,
    Back = 3,
    Forward = 4,
}

impl MouseButton {
    /// Convert from winit's `MouseButton`.
    pub fn from_winit(button: winit::event::MouseButton) -> Option<Self> {
        match button {
            winit::event::MouseButton::Left => Some(Self::Left),
            winit::event::MouseButton::Right => Some(Self::Right),
            winit::event::MouseButton::Middle => Some(Self::Middle),
            winit::event::MouseButton::Back => Some(Self::Back),
            winit::event::MouseButton::Forward => Some(Self::Forward),
            _ => None,
        }
    }

    /// Convert to a button index (for the internal arrays).
    #[inline]
    pub fn index(self) -> usize {
        self as usize
    }
}

// ── KeyCode ─────────────────────────────────────────────────────────────────

/// Re-exported subset of winit key codes for convenience.
///
/// Covers the most commonly used keys. Unknown variants are mapped to
/// [`KeyCode::Unknown`] and silently ignored by the input system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum KeyCode {
    // ── Arrows ──────────────────────
    ArrowUp = 0,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    // ── Special ─────────────────────
    Space,
    Enter,
    Escape,
    // ── Letters ─────────────────────
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
    // ── Digits ──────────────────────
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
    // ── Modifiers ───────────────────
    ShiftLeft,
    ShiftRight,
    ControlLeft,
    ControlRight,
    AltLeft,
    AltRight,
    // ── Editing / Navigation ────────
    Tab,
    Backspace,
    Delete,
    Insert,
    Home,
    End,
    PageUp,
    PageDown,
    CapsLock,
    // ── Function keys ───────────────
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    // ── Misc ────────────────────────
    Minus,
    Equal,
    BracketLeft,
    BracketRight,
    Backslash,
    Semicolon,
    Quote,
    Comma,
    Period,
    Slash,
    Backquote,
    // ── Sentinel ────────────────────
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
    KeyCode::AltLeft,
    KeyCode::AltRight,
    KeyCode::Tab,
    KeyCode::Backspace,
    KeyCode::Delete,
    KeyCode::Insert,
    KeyCode::Home,
    KeyCode::End,
    KeyCode::PageUp,
    KeyCode::PageDown,
    KeyCode::CapsLock,
    KeyCode::F1,
    KeyCode::F2,
    KeyCode::F3,
    KeyCode::F4,
    KeyCode::F5,
    KeyCode::F6,
    KeyCode::F7,
    KeyCode::F8,
    KeyCode::F9,
    KeyCode::F10,
    KeyCode::F11,
    KeyCode::F12,
    KeyCode::Minus,
    KeyCode::Equal,
    KeyCode::BracketLeft,
    KeyCode::BracketRight,
    KeyCode::Backslash,
    KeyCode::Semicolon,
    KeyCode::Quote,
    KeyCode::Comma,
    KeyCode::Period,
    KeyCode::Slash,
    KeyCode::Backquote,
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
            W::AltLeft => Self::AltLeft,
            W::AltRight => Self::AltRight,
            W::Tab => Self::Tab,
            W::Backspace => Self::Backspace,
            W::Delete => Self::Delete,
            W::Insert => Self::Insert,
            W::Home => Self::Home,
            W::End => Self::End,
            W::PageUp => Self::PageUp,
            W::PageDown => Self::PageDown,
            W::CapsLock => Self::CapsLock,
            W::F1 => Self::F1,
            W::F2 => Self::F2,
            W::F3 => Self::F3,
            W::F4 => Self::F4,
            W::F5 => Self::F5,
            W::F6 => Self::F6,
            W::F7 => Self::F7,
            W::F8 => Self::F8,
            W::F9 => Self::F9,
            W::F10 => Self::F10,
            W::F11 => Self::F11,
            W::F12 => Self::F12,
            W::Minus => Self::Minus,
            W::Equal => Self::Equal,
            W::BracketLeft => Self::BracketLeft,
            W::BracketRight => Self::BracketRight,
            W::Backslash => Self::Backslash,
            W::Semicolon => Self::Semicolon,
            W::Quote => Self::Quote,
            W::Comma => Self::Comma,
            W::Period => Self::Period,
            W::Slash => Self::Slash,
            W::Backquote => Self::Backquote,
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

    #[test]
    fn mouse_button_enum_queries() {
        use super::MouseButton;

        let mut input = Input::new();
        input.mouse_button_down(MouseButton::Left.index());
        assert!(input.mouse_button_held(MouseButton::Left));
        assert!(input.mouse_button_pressed(MouseButton::Left));
        assert!(!input.mouse_button_held(MouseButton::Right));

        input.begin_frame();
        assert!(input.mouse_button_held(MouseButton::Left));
        assert!(!input.mouse_button_pressed(MouseButton::Left));

        input.mouse_button_up(MouseButton::Left.index());
        assert!(!input.mouse_button_held(MouseButton::Left));
        assert!(input.mouse_button_released(MouseButton::Left));
    }

    #[test]
    fn extended_keys_are_tracked() {
        let mut input = Input::new();
        input.key_down(KeyCode::F1);
        assert!(input.key_held(KeyCode::F1));
        assert!(input.key_pressed(KeyCode::F1));

        input.begin_frame();
        assert!(input.key_held(KeyCode::F1));
        assert!(!input.key_pressed(KeyCode::F1));

        input.key_up(KeyCode::F1);
        assert!(!input.key_held(KeyCode::F1));
        assert!(input.key_released(KeyCode::F1));
    }
}
