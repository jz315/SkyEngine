use super::layers::LayerId;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NodeId(String);

impl NodeId {
    pub fn new(id: impl Into<String>) -> Self {
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

impl std::borrow::Borrow<str> for NodeId {
    fn borrow(&self) -> &str {
        self.as_str()
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

    pub fn node_id(&self) -> Option<&NodeId> {
        match self {
            Self::Node(id) => Some(id),
            _ => None,
        }
    }

    pub fn focus_id(&self) -> Option<&NodeId> {
        match self {
            Self::Focus(id) => Some(id),
            _ => None,
        }
    }

    pub fn scroll_id(&self) -> Option<&NodeId> {
        match self {
            Self::Scroll(id) => Some(id),
            _ => None,
        }
    }

    pub fn text_id(&self) -> Option<&NodeId> {
        match self {
            Self::Text(id) => Some(id),
            _ => None,
        }
    }

    pub fn layer_id(&self) -> Option<&LayerId> {
        match self {
            Self::Layer(id) => Some(id),
            _ => None,
        }
    }
}

macro_rules! owner_id {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub(super) struct $name(NodeId);

        impl $name {
            pub(super) fn new(id: NodeId) -> Self {
                Self(id)
            }

            pub(super) fn node_id(&self) -> &NodeId {
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
    fn node_id(&self) -> &NodeId;
}

macro_rules! impl_owner_id {
    ($name:ident) => {
        impl OwnerId for $name {
            fn node_id(&self) -> &NodeId {
                self.node_id()
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
    pub(super) fn pointer_active_node_id(&self) -> Option<NodeId> {
        self.pointer_active
            .as_ref()
            .map(|owner| owner.node_id().clone())
    }

    pub(super) fn pointer_hover_node_id(&self) -> Option<NodeId> {
        self.pointer_hover
            .as_ref()
            .map(|owner| owner.node_id().clone())
    }

    pub(super) fn pointer_capture_node_id(&self) -> Option<NodeId> {
        self.pointer_capture
            .as_ref()
            .map(|owner| owner.node_id().clone())
    }

    pub(super) fn set_pointer_press_target(&mut self, target: Option<NodeId>) {
        self.pointer_active = target.clone().map(PointerActiveId::new);
        self.pointer_capture = target.map(PointerCaptureId::new);
    }

    pub(super) fn set_pointer_hover(&mut self, target: Option<NodeId>) {
        self.pointer_hover = target.map(PointerHoverId::new);
    }

    pub(super) fn clear_pointer_press(&mut self) {
        self.pointer_active = None;
        self.pointer_capture = None;
        self.drag_owner = None;
    }

    pub(super) fn keyboard_focus_id(&self) -> Option<&str> {
        self.keyboard_focus
            .as_ref()
            .map(|owner| owner.node_id().as_str())
    }

    pub(super) fn keyboard_focus_node_id(&self) -> Option<NodeId> {
        self.keyboard_focus
            .as_ref()
            .map(|owner| owner.node_id().clone())
    }

    pub(super) fn text_focus_id(&self) -> Option<&str> {
        self.text_focus
            .as_ref()
            .map(|owner| owner.node_id().as_str())
    }

    pub(super) fn text_focus_node_id(&self) -> Option<NodeId> {
        self.text_focus
            .as_ref()
            .map(|owner| owner.node_id().clone())
    }

    pub(super) fn ime_owner_node_id(&self) -> Option<NodeId> {
        self.ime_owner.as_ref().map(|owner| owner.node_id().clone())
    }

    pub(super) fn set_keyboard_focus(&mut self, focused: Option<NodeId>, text_enabled: bool) {
        self.keyboard_focus = focused.clone().map(KeyboardFocusId::new);
        self.text_focus = focused.filter(|_| text_enabled).map(TextFocusId::new);
        self.ime_owner = self
            .text_focus
            .as_ref()
            .map(|text_focus| ImeOwnerId::new(text_focus.node_id().clone()));
    }

    pub(super) fn set_scroll_owner(&mut self, owner: NodeId) {
        self.scroll_owner = Some(ScrollOwnerId::new(owner));
    }

    pub(super) fn scroll_owner_node_id(&self) -> Option<NodeId> {
        self.scroll_owner
            .as_ref()
            .map(|owner| owner.node_id().clone())
    }

    pub(super) fn set_drag_owner(&mut self, owner: NodeId) {
        self.drag_owner = Some(DragOwnerId::new(owner));
    }

    pub(super) fn drag_owner_node_id(&self) -> Option<NodeId> {
        self.drag_owner
            .as_ref()
            .map(|owner| owner.node_id().clone())
    }

    pub(super) fn retain_existing(&mut self, mut exists: impl FnMut(&NodeId) -> bool) -> bool {
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
    exists: &mut impl FnMut(&NodeId) -> bool,
) -> bool {
    if owner.as_ref().is_some_and(|id| !exists(id.node_id())) {
        *owner = None;
        return true;
    }
    false
}
