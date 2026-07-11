//! High-level input actions resource.
//!
//! [`InputActions`] is the top-level ECS resource that users query for
//! semantic game input.  It is updated each frame by the runner from the
//! raw [`Input`](super::raw::Input) state.

use super::action::{ActionEntry, ActionKind, ActionMap, ActionValue, BindingKind};
use super::source::read_source;
use rustc_hash::FxHashMap;

/// High-level input system — query game actions instead of raw keys.
///
/// Register this as an ECS resource.  The runner calls
/// `update()` automatically each frame after syncing
/// the raw [`Input`](super::raw::Input) state.
///
/// # Example
///
/// ```rust,ignore
/// use sky_engine::{ActionMap, InputActions, InputSource, KeyCode};
///
/// // Setup (once)
/// let mut player = ActionMap::new("player");
/// player.add_button("jump", [InputSource::Key(KeyCode::Space)]);
/// player.add_axis_2d("move",
///     InputSource::Key(KeyCode::KeyW),
///     InputSource::Key(KeyCode::KeyS),
///     InputSource::Key(KeyCode::KeyA),
///     InputSource::Key(KeyCode::KeyD),
/// );
///
/// let mut actions = InputActions::new();
/// actions.add_map(player);
/// world.insert_resource(actions);
///
/// // Per-frame query
/// let actions = world.get_resource::<InputActions>().unwrap();
/// if actions.action_pressed("jump") { /* ... */ }
/// let dir = actions.axis_value("move"); // [f32; 2]
/// ```
pub struct InputActions {
    maps: Vec<ActionMap>,
    /// Previous frame action values (for edge detection).
    prev: FxHashMap<String, ActionValue>,
    /// Current frame action values.
    curr: FxHashMap<String, ActionValue>,
}

impl InputActions {
    /// Create an empty action system with no maps.
    pub fn new() -> Self {
        Self {
            maps: Vec::new(),
            prev: FxHashMap::default(),
            curr: FxHashMap::default(),
        }
    }

    /// Add an action map.
    pub fn add_map(&mut self, map: ActionMap) {
        self.maps.push(map);
    }

    /// Mutable access to a named action map (for runtime rebinding or
    /// enable/disable).
    pub fn map_mut(&mut self, name: &str) -> Option<&mut ActionMap> {
        self.maps.iter_mut().find(|m| m.name() == name)
    }

    /// Read-only access to a named action map.
    pub fn map(&self, name: &str) -> Option<&ActionMap> {
        self.maps.iter().find(|m| m.name() == name)
    }

    /// Enable or disable a named action map.
    pub fn set_map_enabled(&mut self, name: &str, enabled: bool) {
        if let Some(map) = self.map_mut(name) {
            map.set_enabled(enabled);
        }
    }

    /// Whether a named action map is currently enabled.
    pub fn is_map_enabled(&self, name: &str) -> bool {
        self.map(name).is_some_and(|m| m.is_enabled())
    }

    // ── Action queries ──────────────────────────────────────────────────

    /// Is the action currently held (value > 0.5)?
    #[inline]
    pub fn action_held(&self, name: &str) -> bool {
        self.curr.get(name).is_some_and(|v| v.as_bool())
    }

    /// Was the action just pressed this frame?
    ///
    /// True when current frame is active but previous frame was not.
    #[inline]
    pub fn action_pressed(&self, name: &str) -> bool {
        let curr = self.curr.get(name).is_some_and(|v| v.as_bool());
        let prev = self.prev.get(name).is_some_and(|v| v.as_bool());
        curr && !prev
    }

    /// Was the action just released this frame?
    #[inline]
    pub fn action_released(&self, name: &str) -> bool {
        let curr = self.curr.get(name).is_some_and(|v| v.as_bool());
        let prev = self.prev.get(name).is_some_and(|v| v.as_bool());
        !curr && prev
    }

    /// Get the 1D float value of an action (0.0 for inactive buttons,
    /// −1.0..1.0 for axes).
    #[inline]
    pub fn action_value(&self, name: &str) -> f32 {
        self.curr.get(name).map_or(0.0, |v| v.as_f32())
    }

    /// Get the 2D vector value of an axis action `[x, y]`.
    ///
    /// Diagonal values are normalised to the unit circle so WASD
    /// diagonal movement has magnitude 1, not √2.
    #[inline]
    pub fn axis_value(&self, name: &str) -> [f32; 2] {
        self.curr.get(name).map_or([0.0, 0.0], |v| v.as_vec2())
    }

    /// Raw [`ActionValue`] for an action.
    #[inline]
    pub fn raw_value(&self, name: &str) -> ActionValue {
        self.curr.get(name).copied().unwrap_or(ActionValue::ZERO)
    }

    // ── Frame update (called by runner) ─────────────────────────────────

    /// Recompute all action values from the raw input state.
    ///
    /// Called automatically by the runner each frame.
    pub(crate) fn update(&mut self, input: &super::raw::Input) {
        // Swap current → previous.
        std::mem::swap(&mut self.prev, &mut self.curr);
        self.curr.clear();

        for map in &self.maps {
            if !map.enabled {
                continue;
            }
            for (name, entry) in &map.actions {
                let value = evaluate_action(input, entry);
                // If multiple maps define the same action, last-enabled wins.
                self.curr.insert(name.clone(), value);
            }
        }
    }
}

impl Default for InputActions {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for InputActions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InputActions")
            .field("maps", &self.maps.len())
            .field("actions", &self.curr.len())
            .finish()
    }
}

// ── Evaluation ──────────────────────────────────────────────────────────────

/// Evaluate the current value of a single action entry.
fn evaluate_action(input: &super::raw::Input, entry: &ActionEntry) -> ActionValue {
    match entry.kind {
        ActionKind::Button => {
            // Any binding active → button is pressed.
            let mut value = 0.0f32;
            for binding in &entry.bindings {
                value = value.max(evaluate_binding_1d(input, binding));
            }
            ActionValue { x: value, y: 0.0 }
        }
        ActionKind::Axis1D => {
            let mut value = 0.0f32;
            for binding in &entry.bindings {
                value += evaluate_binding_1d(input, binding);
            }
            ActionValue {
                x: value.clamp(-1.0, 1.0),
                y: 0.0,
            }
        }
        ActionKind::Axis2D => {
            let mut x = 0.0f32;
            let mut y = 0.0f32;
            for binding in &entry.bindings {
                let (bx, by) = evaluate_binding_2d(input, binding);
                x += bx;
                y += by;
            }
            // Normalise diagonal to unit circle.
            let len_sq = x * x + y * y;
            if len_sq > 1.0 {
                let inv_len = 1.0 / len_sq.sqrt();
                x *= inv_len;
                y *= inv_len;
            }
            ActionValue { x, y }
        }
    }
}

/// Evaluate a single binding as a 1D value.
fn evaluate_binding_1d(input: &super::raw::Input, binding: &BindingKind) -> f32 {
    match binding {
        BindingKind::Simple(b) => read_source(input, b.source) * b.scale,
        BindingKind::Axis1D { negative, positive } => {
            let neg = read_source(input, negative.source) * negative.scale;
            let pos = read_source(input, positive.source) * positive.scale;
            (neg + pos).clamp(-1.0, 1.0)
        }
        BindingKind::Axis2D { left, right, .. } => {
            // When used as 1D, take x component only.
            let l = read_source(input, left.source) * left.scale;
            let r = read_source(input, right.source) * right.scale;
            (l + r).clamp(-1.0, 1.0)
        }
    }
}

/// Evaluate a single binding as a 2D value `(x, y)`.
fn evaluate_binding_2d(input: &super::raw::Input, binding: &BindingKind) -> (f32, f32) {
    match binding {
        BindingKind::Simple(b) => {
            let v = read_source(input, b.source) * b.scale;
            (v, 0.0)
        }
        BindingKind::Axis1D { negative, positive } => {
            let neg = read_source(input, negative.source) * negative.scale;
            let pos = read_source(input, positive.source) * positive.scale;
            ((neg + pos).clamp(-1.0, 1.0), 0.0)
        }
        BindingKind::Axis2D {
            up,
            down,
            left,
            right,
        } => {
            let u = read_source(input, up.source) * up.scale;
            let d = read_source(input, down.source) * down.scale;
            let l = read_source(input, left.source) * left.scale;
            let r = read_source(input, right.source) * right.scale;
            let x = (l + r).clamp(-1.0, 1.0);
            let y = (u + d).clamp(-1.0, 1.0);
            (x, y)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::raw::{Input, KeyCode, MouseButton};
    use crate::input::source::InputSource;

    fn make_actions() -> (InputActions, Input) {
        let mut map = ActionMap::new("player");
        map.add_button("jump", [InputSource::Key(KeyCode::Space)]);
        map.add_button("fire", [InputSource::Mouse(MouseButton::Left)]);
        map.add_axis_2d(
            "move",
            InputSource::Key(KeyCode::KeyW),
            InputSource::Key(KeyCode::KeyS),
            InputSource::Key(KeyCode::KeyA),
            InputSource::Key(KeyCode::KeyD),
        );
        map.add_axis_1d(
            "strafe",
            InputSource::Key(KeyCode::KeyA),
            InputSource::Key(KeyCode::KeyD),
        );

        let mut actions = InputActions::new();
        actions.add_map(map);
        (actions, Input::new())
    }

    #[test]
    fn button_pressed_held_released() {
        let (mut actions, mut input) = make_actions();

        // Frame 1: press Space
        input.key_down(KeyCode::Space);
        actions.update(&input);
        assert!(actions.action_pressed("jump"));
        assert!(actions.action_held("jump"));
        assert!(!actions.action_released("jump"));

        // Frame 2: still holding
        input.begin_frame();
        actions.update(&input);
        assert!(!actions.action_pressed("jump")); // not "just pressed" anymore
        assert!(actions.action_held("jump"));

        // Frame 3: release
        input.begin_frame();
        input.key_up(KeyCode::Space);
        actions.update(&input);
        assert!(!actions.action_held("jump"));
        assert!(actions.action_released("jump"));
    }

    #[test]
    fn axis_2d_cardinal() {
        let (mut actions, mut input) = make_actions();

        // Press W only → y = +1, x = 0
        input.key_down(KeyCode::KeyW);
        actions.update(&input);
        let [x, y] = actions.axis_value("move");
        assert!((x).abs() < f32::EPSILON);
        assert!((y - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn axis_2d_diagonal_normalised() {
        let (mut actions, mut input) = make_actions();

        // Press W + D → diagonal, normalised to unit circle
        input.key_down(KeyCode::KeyW);
        input.key_down(KeyCode::KeyD);
        actions.update(&input);
        let [x, y] = actions.axis_value("move");
        let len = (x * x + y * y).sqrt();
        assert!(
            (len - 1.0).abs() < 1e-5,
            "diagonal length should be ~1.0 but got {len}"
        );
    }

    #[test]
    fn axis_1d_negative_positive() {
        let (mut actions, mut input) = make_actions();

        // Press D → positive
        input.key_down(KeyCode::KeyD);
        actions.update(&input);
        assert!((actions.action_value("strafe") - 1.0).abs() < f32::EPSILON);

        // Also press A → cancel out to 0
        input.key_down(KeyCode::KeyA);
        actions.update(&input);
        assert!(actions.action_value("strafe").abs() < f32::EPSILON);
    }

    #[test]
    fn disabled_map_produces_no_values() {
        let (mut actions, mut input) = make_actions();

        input.key_down(KeyCode::Space);
        actions.set_map_enabled("player", false);
        actions.update(&input);
        assert!(!actions.action_held("jump"));
    }

    #[test]
    fn mouse_button_action() {
        let (mut actions, mut input) = make_actions();

        input.mouse_button_down(0); // left
        actions.update(&input);
        assert!(actions.action_pressed("fire"));
        assert!(actions.action_held("fire"));
    }

    #[test]
    fn unknown_action_returns_defaults() {
        let (mut actions, input) = make_actions();
        actions.update(&input);
        assert!(!actions.action_held("nonexistent"));
        assert_eq!(actions.action_value("nonexistent"), 0.0);
        assert_eq!(actions.axis_value("nonexistent"), [0.0, 0.0]);
    }
}
