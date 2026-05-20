/// Pointer snapshot in logical UI coordinates.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PointerEvent {
    pub x: f32,
    pub y: f32,
    pub delta_x: f32,
    pub delta_y: f32,
    pub position: Option<[f32; 2]>,
    pub delta: [f32; 2],
    pub down: bool,
    pub pressed_this_frame: bool,
    pub released_this_frame: bool,
    pub right_down: bool,
    pub right_pressed_this_frame: bool,
    pub right_released_this_frame: bool,
}

impl PointerEvent {
    pub fn new(
        x: f32,
        y: f32,
        delta_x: f32,
        delta_y: f32,
        down: bool,
        pressed_this_frame: bool,
        released_this_frame: bool,
    ) -> Self {
        Self {
            x,
            y,
            delta_x,
            delta_y,
            position: Some([x, y]),
            delta: [delta_x, delta_y],
            down,
            pressed_this_frame,
            released_this_frame,
            right_down: false,
            right_pressed_this_frame: false,
            right_released_this_frame: false,
        }
    }

    pub fn at(x: f32, y: f32) -> Self {
        Self {
            x,
            y,
            position: Some([x, y]),
            ..Self::default()
        }
    }

    pub fn pressed_at(x: f32, y: f32) -> Self {
        Self {
            x,
            y,
            position: Some([x, y]),
            down: true,
            pressed_this_frame: true,
            ..Self::default()
        }
    }

    pub fn dragged_to(x: f32, y: f32, delta_x: f32, delta_y: f32) -> Self {
        Self {
            x,
            y,
            delta_x,
            delta_y,
            position: Some([x, y]),
            delta: [delta_x, delta_y],
            down: true,
            ..Self::default()
        }
    }

    pub fn released_at(x: f32, y: f32) -> Self {
        Self {
            x,
            y,
            position: Some([x, y]),
            released_this_frame: true,
            ..Self::default()
        }
    }

    pub fn right_pressed_at(x: f32, y: f32) -> Self {
        Self {
            x,
            y,
            position: Some([x, y]),
            right_down: true,
            right_pressed_this_frame: true,
            ..Self::default()
        }
    }

    pub fn position(self) -> Option<[f32; 2]> {
        self.position.or_else(|| {
            let has_event = self.down
                || self.pressed_this_frame
                || self.released_this_frame
                || self.right_down
                || self.right_pressed_this_frame
                || self.right_released_this_frame
                || self.x != 0.0
                || self.y != 0.0
                || self.delta_x != 0.0
                || self.delta_y != 0.0;
            has_event.then_some([self.x, self.y])
        })
    }

    pub fn delta(self) -> [f32; 2] {
        if self.delta != [0.0, 0.0] {
            self.delta
        } else {
            [self.delta_x, self.delta_y]
        }
    }
}

/// Text and keyboard input routed to the focused element.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KeyboardEvent {
    pub text: String,
    pub paste_text: String,
    pub enter: bool,
    pub escape: bool,
    pub backspace: bool,
    pub delete: bool,
    pub left: bool,
    pub right: bool,
    pub home: bool,
    pub end: bool,
    pub shift: bool,
    pub select_all: bool,
    pub copy: bool,
    pub cut: bool,
}

impl KeyboardEvent {
    pub fn has_input(&self) -> bool {
        !self.text.is_empty()
            || !self.paste_text.is_empty()
            || self.enter
            || self.escape
            || self.backspace
            || self.delete
            || self.left
            || self.right
            || self.home
            || self.end
            || self.select_all
            || self.copy
            || self.cut
    }

    pub fn hasInput(&self) -> bool {
        self.has_input()
    }
}

/// Scroll delta in logical UI coordinates.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ScrollEvent {
    pub x: f32,
    pub y: f32,
}

impl ScrollEvent {
    pub fn active(self) -> bool {
        self.x != 0.0 || self.y != 0.0
    }
}

/// Drag callback payload matching EUI-NEO's `core::dsl::DragEvent`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct DragEvent {
    pub x: f32,
    pub y: f32,
    pub delta_x: f32,
    pub delta_y: f32,
    pub total_x: f32,
    pub total_y: f32,
}

/// Runtime-owned interaction state keyed by stable element ID.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct InteractionState {
    pub hovered: bool,
    pub pressed: bool,
    pub clicked: bool,
    pub press_started: bool,
    pub released: bool,
    pub dragging: bool,
    pub active: bool,
    pub changed: bool,
    pub drag_start: [f32; 2],
    pub drag_delta: [f32; 2],
    pub drag_total: [f32; 2],
}
