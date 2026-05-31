use super::layers::LayerId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeId(String);

impl NodeId {
    pub(crate) fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventTargetId {
    Node(NodeId),
    Focus(NodeId),
    Scroll(NodeId),
    Text(NodeId),
    Layer(LayerId),
}

impl EventTargetId {
    pub fn node(id: impl Into<String>) -> Self {
        Self::Node(NodeId::new(id))
    }

    pub fn focus(id: impl Into<String>) -> Self {
        Self::Focus(NodeId::new(id))
    }

    pub fn scroll(id: impl Into<String>) -> Self {
        Self::Scroll(NodeId::new(id))
    }

    pub fn text(id: impl Into<String>) -> Self {
        Self::Text(NodeId::new(id))
    }

    pub fn layer(id: impl Into<String>) -> Self {
        Self::Layer(LayerId::new(id))
    }

    pub fn role(&self) -> &'static str {
        match self {
            Self::Node(_) => "node",
            Self::Focus(_) => "focus",
            Self::Scroll(_) => "scroll",
            Self::Text(_) => "text",
            Self::Layer(_) => "layer",
        }
    }

    pub fn id(&self) -> &str {
        match self {
            Self::Node(id) | Self::Focus(id) | Self::Scroll(id) | Self::Text(id) => id.as_str(),
            Self::Layer(id) => id.as_str(),
        }
    }
}

macro_rules! owner_id {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub(super) struct $name(String);

        impl $name {
            pub(super) fn new(id: impl Into<String>) -> Self {
                Self(id.into())
            }

            pub(super) fn as_str(&self) -> &str {
                &self.0
            }
        }
    };
}

owner_id!(PointerHoverId);
owner_id!(PointerActiveId);
owner_id!(PointerCaptureId);
owner_id!(KeyboardFocusId);
owner_id!(TextFocusId);
owner_id!(ImeOwnerId);
owner_id!(ScrollOwnerId);
owner_id!(DragOwnerId);

pub(super) trait OwnerId {
    fn as_str(&self) -> &str;
}

impl OwnerId for String {
    fn as_str(&self) -> &str {
        self
    }
}

macro_rules! impl_owner_id {
    ($name:ident) => {
        impl OwnerId for $name {
            fn as_str(&self) -> &str {
                self.as_str()
            }
        }
    };
}

impl_owner_id!(PointerHoverId);
impl_owner_id!(PointerActiveId);
impl_owner_id!(PointerCaptureId);
impl_owner_id!(KeyboardFocusId);
impl_owner_id!(TextFocusId);
impl_owner_id!(ImeOwnerId);
impl_owner_id!(ScrollOwnerId);
impl_owner_id!(DragOwnerId);

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct InputOwners {
    pub(super) pointer_hover: Option<PointerHoverId>,
    pub(super) pointer_active: Option<PointerActiveId>,
    pub(super) pointer_capture: Option<PointerCaptureId>,
    pub(super) keyboard_focus: Option<KeyboardFocusId>,
    pub(super) text_focus: Option<TextFocusId>,
    pub(super) ime_owner: Option<ImeOwnerId>,
    pub(super) scroll_owner: Option<ScrollOwnerId>,
    pub(super) drag_owner: Option<DragOwnerId>,
}

impl InputOwners {
    pub(super) fn pointer_active_string(&self) -> Option<String> {
        self.pointer_active
            .as_ref()
            .map(|owner| owner.as_str().to_string())
    }

    pub(super) fn pointer_capture_string(&self) -> Option<String> {
        self.pointer_capture
            .as_ref()
            .map(|owner| owner.as_str().to_string())
    }

    pub(super) fn set_pointer_press_target(&mut self, target: Option<String>) {
        self.pointer_active = target.as_ref().map(PointerActiveId::new);
        self.pointer_capture = target.map(PointerCaptureId::new);
    }

    pub(super) fn set_pointer_hover(&mut self, target: Option<String>) {
        self.pointer_hover = target.map(PointerHoverId::new);
    }

    pub(super) fn clear_pointer_press(&mut self) {
        self.pointer_active = None;
        self.pointer_capture = None;
        self.drag_owner = None;
    }

    pub(super) fn keyboard_focus_id(&self) -> Option<&str> {
        self.keyboard_focus.as_ref().map(KeyboardFocusId::as_str)
    }

    pub(super) fn keyboard_focus_string(&self) -> Option<String> {
        self.keyboard_focus_id().map(str::to_string)
    }

    pub(super) fn text_focus_id(&self) -> Option<&str> {
        self.text_focus.as_ref().map(TextFocusId::as_str)
    }

    pub(super) fn text_focus_string(&self) -> Option<String> {
        self.text_focus_id().map(str::to_string)
    }

    pub(super) fn ime_owner_id(&self) -> Option<&str> {
        self.ime_owner.as_ref().map(ImeOwnerId::as_str)
    }

    pub(super) fn set_keyboard_focus(&mut self, focused: Option<String>, text_enabled: bool) {
        self.keyboard_focus = focused.clone().map(KeyboardFocusId::new);
        self.text_focus = focused
            .as_ref()
            .filter(|_| text_enabled)
            .cloned()
            .map(TextFocusId::new);
        self.ime_owner = self
            .text_focus
            .as_ref()
            .map(|text_focus| ImeOwnerId::new(text_focus.as_str()));
    }

    pub(super) fn set_scroll_owner(&mut self, owner: String) {
        self.scroll_owner = Some(ScrollOwnerId::new(owner));
    }

    pub(super) fn set_drag_owner(&mut self, owner: String) {
        self.drag_owner = Some(DragOwnerId::new(owner));
    }

    pub(super) fn retain_existing(&mut self, mut exists: impl FnMut(&str) -> bool) -> bool {
        let mut changed = false;
        changed |= clear_missing_owner(&mut self.pointer_hover, &mut exists);
        changed |= clear_missing_owner(&mut self.pointer_active, &mut exists);
        changed |= clear_missing_owner(&mut self.pointer_capture, &mut exists);
        changed |= clear_missing_owner(&mut self.keyboard_focus, &mut exists);
        changed |= clear_missing_owner(&mut self.text_focus, &mut exists);
        changed |= clear_missing_owner(&mut self.ime_owner, &mut exists);
        changed |= clear_missing_owner(&mut self.scroll_owner, &mut exists);
        changed |= clear_missing_owner(&mut self.drag_owner, &mut exists);
        changed
    }
}

fn clear_missing_owner<T: OwnerId>(
    owner: &mut Option<T>,
    exists: &mut impl FnMut(&str) -> bool,
) -> bool {
    if owner.as_ref().is_some_and(|id| !exists(id.as_str())) {
        *owner = None;
        return true;
    }
    false
}
