//! Action definitions, bindings, and action maps.
//!
//! An [`ActionMap`] groups related input actions (e.g. "player", "ui") and
//! maps each action to one or more physical [`InputSource`]s via bindings.

use super::source::InputSource;
use rustc_hash::FxHashMap;

// ── ActionKind / ActionValue ────────────────────────────────────────────────

/// The type of value an action produces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionKind {
    /// Digital button — pressed or not (value: 0.0 or 1.0).
    Button,
    /// 1D analog axis (value: −1.0 to 1.0).
    Axis1D,
    /// 2D analog axis (value: [x, y] each −1.0 to 1.0).
    Axis2D,
}

/// The resolved value of an action for the current frame.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ActionValue {
    /// Primary component (Button: 0/1, Axis1D: −1..1, Axis2D: x).
    pub x: f32,
    /// Secondary component (Axis2D: y, otherwise 0).
    pub y: f32,
}

impl ActionValue {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };

    /// Interpret as a boolean (button pressed when x > 0.5).
    #[inline]
    pub fn as_bool(&self) -> bool {
        self.x > 0.5
    }

    /// Interpret as a 1D float value.
    #[inline]
    pub fn as_f32(&self) -> f32 {
        self.x
    }

    /// Interpret as a 2D vector `[x, y]`.
    #[inline]
    pub fn as_vec2(&self) -> [f32; 2] {
        [self.x, self.y]
    }
}

// ── Bindings ────────────────────────────────────────────────────────────────

/// How an [`InputSource`] maps to an action's value.
#[derive(Debug, Clone)]
pub struct InputBinding {
    /// The physical input source.
    pub source: InputSource,
    /// Multiplier applied to the source value (default 1.0).
    /// Use −1.0 for the negative direction of an axis.
    pub scale: f32,
}

impl InputBinding {
    /// Simple 1:1 binding with scale 1.0.
    pub fn new(source: InputSource) -> Self {
        Self { source, scale: 1.0 }
    }

    /// Binding with a custom scale factor.
    pub fn with_scale(source: InputSource, scale: f32) -> Self {
        Self { source, scale }
    }
}

/// Composite binding strategies that combine multiple sources.
#[derive(Debug, Clone)]
pub enum BindingKind {
    /// A single source maps directly to the action.
    Simple(InputBinding),
    /// Two keys form a 1D axis (negative + positive).
    Axis1D {
        negative: InputBinding,
        positive: InputBinding,
    },
    /// Four keys form a 2D axis (up/down/left/right).
    Axis2D {
        up: InputBinding,
        down: InputBinding,
        left: InputBinding,
        right: InputBinding,
    },
}

// ── ActionEntry ─────────────────────────────────────────────────────────────

/// Internal storage for a single action's metadata and bindings.
#[derive(Debug, Clone)]
pub(crate) struct ActionEntry {
    pub kind: ActionKind,
    pub bindings: Vec<BindingKind>,
}

// ── ActionMap ───────────────────────────────────────────────────────────────

/// A named group of input actions.
///
/// Action maps let you organise actions by context (e.g. "player", "vehicle",
/// "ui") and enable/disable entire groups at runtime.
///
/// # Example
///
/// ```rust,ignore
/// use sky_engine::{ActionMap, InputSource, KeyCode, MouseButton};
///
/// let mut map = ActionMap::new("player");
/// map.add_button("jump", [InputSource::Key(KeyCode::Space)]);
/// map.add_button("fire", [InputSource::Mouse(MouseButton::Left)]);
/// map.add_axis_2d(
///     "move",
///     InputSource::Key(KeyCode::KeyW),
///     InputSource::Key(KeyCode::KeyS),
///     InputSource::Key(KeyCode::KeyA),
///     InputSource::Key(KeyCode::KeyD),
/// );
/// ```
pub struct ActionMap {
    name: String,
    pub(crate) actions: FxHashMap<String, ActionEntry>,
    pub(crate) enabled: bool,
}

impl ActionMap {
    /// Create a new action map with the given name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            actions: FxHashMap::default(),
            enabled: true,
        }
    }

    /// The name of this action map.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Whether this action map is currently enabled.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Enable or disable this action map.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Register a button action with one or more bindings.
    pub fn add_button(
        &mut self,
        name: impl Into<String>,
        sources: impl IntoIterator<Item = InputSource>,
    ) -> &mut Self {
        let bindings = sources
            .into_iter()
            .map(|s| BindingKind::Simple(InputBinding::new(s)))
            .collect();
        self.actions.insert(
            name.into(),
            ActionEntry {
                kind: ActionKind::Button,
                bindings,
            },
        );
        self
    }

    /// Register a 1D axis action from two directional keys.
    pub fn add_axis_1d(
        &mut self,
        name: impl Into<String>,
        negative: InputSource,
        positive: InputSource,
    ) -> &mut Self {
        self.actions.insert(
            name.into(),
            ActionEntry {
                kind: ActionKind::Axis1D,
                bindings: vec![BindingKind::Axis1D {
                    negative: InputBinding::with_scale(negative, -1.0),
                    positive: InputBinding::new(positive),
                }],
            },
        );
        self
    }

    /// Register a 2D axis action from four directional keys (WASD-style).
    pub fn add_axis_2d(
        &mut self,
        name: impl Into<String>,
        up: InputSource,
        down: InputSource,
        left: InputSource,
        right: InputSource,
    ) -> &mut Self {
        self.actions.insert(
            name.into(),
            ActionEntry {
                kind: ActionKind::Axis2D,
                bindings: vec![BindingKind::Axis2D {
                    up: InputBinding::new(up),
                    down: InputBinding::with_scale(down, -1.0),
                    left: InputBinding::with_scale(left, -1.0),
                    right: InputBinding::new(right),
                }],
            },
        );
        self
    }

    /// Register a raw binding (advanced — for custom composite setups).
    pub fn add_raw(
        &mut self,
        name: impl Into<String>,
        kind: ActionKind,
        bindings: Vec<BindingKind>,
    ) -> &mut Self {
        self.actions
            .insert(name.into(), ActionEntry { kind, bindings });
        self
    }

    /// Replace a specific binding for an action at runtime (for rebinding).
    ///
    /// Returns `true` if the rebind succeeded.
    pub fn rebind(&mut self, action: &str, binding_index: usize, new_source: InputSource) -> bool {
        let Some(entry) = self.actions.get_mut(action) else {
            return false;
        };
        let Some(binding) = entry.bindings.get_mut(binding_index) else {
            return false;
        };
        match binding {
            BindingKind::Simple(b) => b.source = new_source,
            // For composite bindings, rebind replaces the positive source
            BindingKind::Axis1D { positive, .. } => positive.source = new_source,
            BindingKind::Axis2D { up, .. } => up.source = new_source,
        }
        true
    }
}

impl std::fmt::Debug for ActionMap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActionMap")
            .field("name", &self.name)
            .field("enabled", &self.enabled)
            .field("action_count", &self.actions.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::raw::KeyCode;

    #[test]
    fn action_map_add_button() {
        let mut map = ActionMap::new("test");
        map.add_button("jump", [InputSource::Key(KeyCode::Space)]);

        let entry = map.actions.get("jump").unwrap();
        assert_eq!(entry.kind, ActionKind::Button);
        assert_eq!(entry.bindings.len(), 1);
    }

    #[test]
    fn action_map_add_axis_2d() {
        let mut map = ActionMap::new("test");
        map.add_axis_2d(
            "move",
            InputSource::Key(KeyCode::KeyW),
            InputSource::Key(KeyCode::KeyS),
            InputSource::Key(KeyCode::KeyA),
            InputSource::Key(KeyCode::KeyD),
        );

        let entry = map.actions.get("move").unwrap();
        assert_eq!(entry.kind, ActionKind::Axis2D);
        assert_eq!(entry.bindings.len(), 1);
    }

    #[test]
    fn rebind_simple_action() {
        let mut map = ActionMap::new("test");
        map.add_button("fire", [InputSource::Key(KeyCode::Space)]);
        assert!(map.rebind("fire", 0, InputSource::Key(KeyCode::Enter)));

        let entry = map.actions.get("fire").unwrap();
        match &entry.bindings[0] {
            BindingKind::Simple(b) => assert_eq!(b.source, InputSource::Key(KeyCode::Enter)),
            _ => panic!("expected Simple binding"),
        }
    }

    #[test]
    fn rebind_nonexistent_returns_false() {
        let mut map = ActionMap::new("test");
        assert!(!map.rebind("ghost", 0, InputSource::Key(KeyCode::Space)));
    }

    #[test]
    fn enable_disable_toggle() {
        let mut map = ActionMap::new("test");
        assert!(map.is_enabled());
        map.set_enabled(false);
        assert!(!map.is_enabled());
    }
}
