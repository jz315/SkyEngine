use rustc_hash::FxHashSet;

use crate::ecs::EntityId;

use super::{KeyCode, MouseButton};

/// Identifies the layer that claimed an input event this frame.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum InteractionOwner {
    Ui,
    Domain(&'static str),
    Entity(EntityId),
    Custom(String),
}

/// Long-lived pointer capture owner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InteractionCapture {
    pub owner: InteractionOwner,
    pub entity: Option<EntityId>,
}

/// Frame-local routing state shared by UI, gameplay, tools, and domain systems.
///
/// The raw [`Input`](super::Input) resource remains read-only factual state.
/// `InteractionContext` records which physical inputs have already been claimed
/// so later systems do not reinterpret the same click or key as a second
/// semantic action.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct InteractionContext {
    pointer_consumed: [bool; 5],
    scroll_consumed: bool,
    key_consumed: FxHashSet<KeyCode>,
    hovered: Option<EntityId>,
    pressed: Option<EntityId>,
    focused: Option<EntityId>,
    capture: Option<InteractionCapture>,
}

impl InteractionContext {
    pub fn new() -> Self {
        Self::default()
    }

    /// Clear per-frame consumption while preserving focus and pointer capture.
    pub fn begin_frame(&mut self) {
        self.pointer_consumed = [false; 5];
        self.scroll_consumed = false;
        self.key_consumed.clear();
        self.hovered = None;
        self.pressed = None;
    }

    pub fn consume_pointer(&mut self, button: MouseButton) {
        self.pointer_consumed[button.index()] = true;
    }

    pub fn pointer_consumed(&self, button: MouseButton) -> bool {
        self.pointer_consumed[button.index()]
    }

    pub fn consume_scroll(&mut self) {
        self.scroll_consumed = true;
    }

    pub fn scroll_consumed(&self) -> bool {
        self.scroll_consumed
    }

    pub fn consume_key(&mut self, key: KeyCode) {
        self.key_consumed.insert(key);
    }

    pub fn key_consumed(&self, key: KeyCode) -> bool {
        self.key_consumed.contains(&key)
    }

    pub fn set_hovered(&mut self, hovered: Option<EntityId>) {
        self.hovered = hovered;
    }

    pub fn hovered(&self) -> Option<EntityId> {
        self.hovered
    }

    pub fn set_pressed(&mut self, pressed: Option<EntityId>) {
        self.pressed = pressed;
    }

    pub fn pressed(&self) -> Option<EntityId> {
        self.pressed
    }

    pub fn set_focus(&mut self, focused: Option<EntityId>) {
        self.focused = focused;
    }

    pub fn focused(&self) -> Option<EntityId> {
        self.focused
    }

    pub fn set_capture(&mut self, capture: Option<InteractionCapture>) {
        self.capture = capture;
    }

    pub fn capture(&self) -> Option<&InteractionCapture> {
        self.capture.as_ref()
    }

    pub fn wants_pointer(&self) -> bool {
        self.hovered.is_some() || self.pressed.is_some() || self.capture.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::{InteractionCapture, InteractionContext, InteractionOwner};
    use crate::input::{KeyCode, MouseButton};

    #[test]
    fn begin_frame_clears_consumption_but_preserves_capture() {
        let mut context = InteractionContext::new();
        context.consume_pointer(MouseButton::Left);
        context.consume_key(KeyCode::Space);
        context.set_capture(Some(InteractionCapture {
            owner: InteractionOwner::Domain("test"),
            entity: None,
        }));

        context.begin_frame();

        assert!(!context.pointer_consumed(MouseButton::Left));
        assert!(!context.key_consumed(KeyCode::Space));
        assert_eq!(
            context.capture().map(|capture| &capture.owner),
            Some(&InteractionOwner::Domain("test"))
        );
    }
}
