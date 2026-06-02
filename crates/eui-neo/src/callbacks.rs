use rustc_hash::{FxHashMap, FxHashSet};

use crate::retained::CallbackTransferStats;
use crate::runtime::{LayerId, LayerIntent, NodeId};
use crate::{DragEvent, Element, KeyboardEvent, LayoutRect, PointerEvent, ScrollEvent};

macro_rules! node_callback_key {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub(crate) struct $name(NodeId);

        impl $name {
            pub(crate) fn new(id: NodeId) -> Self {
                Self(id)
            }

            pub(crate) fn node(id: &NodeId) -> Self {
                Self(id.clone())
            }

            pub(crate) fn as_str(&self) -> &str {
                self.0.as_str()
            }
        }

        impl std::borrow::Borrow<str> for $name {
            fn borrow(&self) -> &str {
                self.as_str()
            }
        }
    };
}

macro_rules! layer_callback_key {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub(crate) struct $name(LayerId);

        impl $name {
            pub(crate) fn new(id: LayerId) -> Self {
                Self(id)
            }

            pub(crate) fn layer(id: &LayerId) -> Self {
                Self(id.clone())
            }

            pub(crate) fn as_str(&self) -> &str {
                self.0.as_str()
            }
        }

        impl std::borrow::Borrow<str> for $name {
            fn borrow(&self) -> &str {
                self.as_str()
            }
        }
    };
}

node_callback_key!(ClickCallbackId);
node_callback_key!(PressCallbackId);
node_callback_key!(ContextMenuCallbackId);
node_callback_key!(FocusChangedCallbackId);
node_callback_key!(TextInputCallbackId);
node_callback_key!(ScrollCallbackId);
node_callback_key!(DragCallbackId);
layer_callback_key!(LayerDismissCallbackId);
node_callback_key!(TimerCallbackId);

#[derive(Default)]
pub(crate) struct UiCallbacks {
    pub on_click: FxHashMap<ClickCallbackId, Box<dyn FnMut()>>,
    pub on_press: FxHashMap<PressCallbackId, Box<dyn FnMut(PointerEvent, LayoutRect)>>,
    pub on_context_menu: FxHashMap<ContextMenuCallbackId, Box<dyn FnMut(PointerEvent, LayoutRect)>>,
    pub on_focus_changed: FxHashMap<FocusChangedCallbackId, Box<dyn FnMut(bool)>>,
    pub on_text_input: FxHashMap<TextInputCallbackId, Box<dyn FnMut(KeyboardEvent)>>,
    pub on_scroll: FxHashMap<ScrollCallbackId, Box<dyn FnMut(ScrollEvent)>>,
    pub on_drag: FxHashMap<DragCallbackId, Box<dyn FnMut(DragEvent)>>,
    pub on_layer_dismiss: FxHashMap<LayerDismissCallbackId, Box<dyn FnMut()>>,
    pub on_timer: FxHashMap<TimerCallbackId, Box<dyn FnMut()>>,
}

impl std::fmt::Debug for UiCallbacks {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UiCallbacks")
            .field("on_click", &self.on_click.len())
            .field("on_press", &self.on_press.len())
            .field("on_context_menu", &self.on_context_menu.len())
            .field("on_focus_changed", &self.on_focus_changed.len())
            .field("on_text_input", &self.on_text_input.len())
            .field("on_scroll", &self.on_scroll.len())
            .field("on_drag", &self.on_drag.len())
            .field("on_layer_dismiss", &self.on_layer_dismiss.len())
            .field("on_timer", &self.on_timer.len())
            .finish()
    }
}

impl UiCallbacks {
    pub(crate) fn transfer_for_elements(
        &mut self,
        previous: &mut UiCallbacks,
        elements: &[Element],
        active_layers: &[LayerIntent],
    ) -> CallbackTransferStats {
        let mut ids = CallbackTransferIds::collect(elements, active_layers);
        let mut stats = CallbackTransferStats::default();
        for id in ids.nodes.drain() {
            if let Some(callback) = previous.on_click.remove(&ClickCallbackId::node(&id)) {
                self.on_click.insert(ClickCallbackId::node(&id), callback);
                stats.click += 1;
            }
            if let Some(callback) = previous.on_press.remove(&PressCallbackId::node(&id)) {
                self.on_press.insert(PressCallbackId::node(&id), callback);
                stats.press += 1;
            }
            if let Some(callback) = previous
                .on_context_menu
                .remove(&ContextMenuCallbackId::node(&id))
            {
                self.on_context_menu
                    .insert(ContextMenuCallbackId::node(&id), callback);
                stats.context_menu += 1;
            }
            if let Some(callback) = previous
                .on_focus_changed
                .remove(&FocusChangedCallbackId::node(&id))
            {
                self.on_focus_changed
                    .insert(FocusChangedCallbackId::node(&id), callback);
                stats.focus_changed += 1;
            }
            if let Some(callback) = previous
                .on_text_input
                .remove(&TextInputCallbackId::node(&id))
            {
                self.on_text_input
                    .insert(TextInputCallbackId::node(&id), callback);
                stats.text_input += 1;
            }
            if let Some(callback) = previous.on_scroll.remove(&ScrollCallbackId::node(&id)) {
                self.on_scroll.insert(ScrollCallbackId::node(&id), callback);
                stats.scroll += 1;
            }
            if let Some(callback) = previous.on_drag.remove(&DragCallbackId::node(&id)) {
                self.on_drag.insert(DragCallbackId::node(&id), callback);
                stats.drag += 1;
            }
            if let Some(callback) = previous.on_timer.remove(&TimerCallbackId::node(&id)) {
                self.on_timer.insert(TimerCallbackId::node(&id), callback);
                stats.timer += 1;
            }
        }
        for layer_id in ids.layers.drain() {
            if let Some(callback) = previous
                .on_layer_dismiss
                .remove(&LayerDismissCallbackId::layer(&layer_id))
            {
                self.on_layer_dismiss
                    .insert(LayerDismissCallbackId::layer(&layer_id), callback);
                stats.layer_dismiss += 1;
            }
        }
        stats
    }

    pub(crate) fn has_text_input(&self, id: &NodeId) -> bool {
        self.on_text_input
            .contains_key(&TextInputCallbackId::node(id))
    }

    pub(crate) fn has_scroll(&self, id: &NodeId) -> bool {
        self.on_scroll.contains_key(&ScrollCallbackId::node(id))
    }

    pub(crate) fn has_drag(&self, id: &NodeId) -> bool {
        self.on_drag.contains_key(&DragCallbackId::node(id))
    }
}

#[derive(Default)]
struct CallbackTransferIds {
    nodes: FxHashSet<NodeId>,
    layers: FxHashSet<LayerId>,
}

impl CallbackTransferIds {
    fn collect(elements: &[Element], active_layers: &[LayerIntent]) -> Self {
        let mut ids = Self::default();
        collect_element_ids(elements, &mut ids.nodes);
        ids.layers.extend(
            active_layers
                .iter()
                .filter(|intent| {
                    ids.nodes.contains(&intent.owner) || ids.nodes.contains(&intent.root)
                })
                .map(|intent| intent.id.clone()),
        );
        ids
    }
}

fn collect_element_ids(elements: &[Element], ids: &mut FxHashSet<NodeId>) {
    for element in elements {
        ids.insert(NodeId::new(&element.id));
        collect_element_ids(&element.children, ids);
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use super::*;
    use crate::runtime::LayerSize;
    use crate::{Element, ElementKind};

    #[test]
    fn transfer_for_elements_keeps_callback_roles_separate() {
        let click_count = Rc::new(Cell::new(0));
        let drag_count = Rc::new(Cell::new(0));
        let click_count_callback = click_count.clone();
        let drag_count_callback = drag_count.clone();
        let mut previous = UiCallbacks::default();
        previous.on_click.insert(
            ClickCallbackId::new(NodeId::new("page.hit")),
            Box::new(move || click_count_callback.set(click_count_callback.get() + 1)),
        );
        previous.on_drag.insert(
            DragCallbackId::new(NodeId::new("page.hit")),
            Box::new(move |_| drag_count_callback.set(drag_count_callback.get() + 1)),
        );

        let mut next = UiCallbacks::default();
        let stats = next.transfer_for_elements(
            &mut previous,
            &[Element::new(ElementKind::Rect, "page.hit")],
            &[],
        );

        next.on_click
            .get_mut(&ClickCallbackId::new(NodeId::new("page.hit")))
            .unwrap()();
        next.on_drag
            .get_mut(&DragCallbackId::new(NodeId::new("page.hit")))
            .unwrap()(DragEvent::default());

        assert_eq!(click_count.get(), 1);
        assert_eq!(drag_count.get(), 1);
        assert_eq!(stats.click, 1);
        assert_eq!(stats.drag, 1);
        assert_eq!(stats.total(), 2);
        assert!(!next
            .on_click
            .contains_key(&ClickCallbackId::new(NodeId::new("page.missing"))));
        assert!(!next
            .on_drag
            .contains_key(&DragCallbackId::new(NodeId::new("page.missing"))));
    }

    #[test]
    fn transfer_for_elements_uses_layer_intents_for_layer_callbacks() {
        let dismiss_count = Rc::new(Cell::new(0));
        let dismiss_count_callback = dismiss_count.clone();
        let mut previous = UiCallbacks::default();
        previous.on_layer_dismiss.insert(
            LayerDismissCallbackId::new(LayerId::new("page.layer")),
            Box::new(move || dismiss_count_callback.set(dismiss_count_callback.get() + 1)),
        );

        let mut next = UiCallbacks::default();
        let stats = next.transfer_for_elements(
            &mut previous,
            &[Element::new(ElementKind::Rect, "page.hit")],
            &[],
        );

        assert_eq!(stats.layer_dismiss, 0);
        assert_eq!(stats.total(), 0);
        assert!(!next
            .on_layer_dismiss
            .contains_key(&LayerDismissCallbackId::new(LayerId::new("page.layer"))));
        assert!(previous
            .on_layer_dismiss
            .contains_key(&LayerDismissCallbackId::new(LayerId::new("page.layer"))));

        let stats = next.transfer_for_elements(
            &mut previous,
            &[Element::new(ElementKind::Rect, "page.hit")],
            &[layer_intent("page.layer", "page.other")],
        );

        assert_eq!(stats.layer_dismiss, 0);
        assert_eq!(stats.total(), 0);
        assert!(previous
            .on_layer_dismiss
            .contains_key(&LayerDismissCallbackId::new(LayerId::new("page.layer"))));

        let stats = next.transfer_for_elements(
            &mut previous,
            &[Element::new(ElementKind::Rect, "page.hit")],
            &[layer_intent("page.layer", "page.hit")],
        );

        next.on_layer_dismiss
            .get_mut(&LayerDismissCallbackId::new(LayerId::new("page.layer")))
            .unwrap()();

        assert_eq!(dismiss_count.get(), 1);
        assert_eq!(stats.layer_dismiss, 1);
        assert_eq!(stats.total(), 1);
        assert!(previous.on_layer_dismiss.is_empty());
    }

    #[test]
    fn transfer_for_elements_uses_layer_root_for_logical_owner_callbacks() {
        let dismiss_count = Rc::new(Cell::new(0));
        let dismiss_count_callback = dismiss_count.clone();
        let mut previous = UiCallbacks::default();
        previous.on_layer_dismiss.insert(
            LayerDismissCallbackId::new(LayerId::new("page.dialog.panel")),
            Box::new(move || dismiss_count_callback.set(dismiss_count_callback.get() + 1)),
        );

        let mut next = UiCallbacks::default();
        let mut intent = layer_intent("page.dialog.panel", "page.dialog");
        intent.root = NodeId::new("page.dialog.panel");
        let stats = next.transfer_for_elements(
            &mut previous,
            &[Element::new(ElementKind::Rect, "page.dialog.panel")],
            &[intent],
        );

        next.on_layer_dismiss
            .get_mut(&LayerDismissCallbackId::new(LayerId::new(
                "page.dialog.panel",
            )))
            .unwrap()();

        assert_eq!(dismiss_count.get(), 1);
        assert_eq!(stats.layer_dismiss, 1);
        assert!(previous.on_layer_dismiss.is_empty());
    }

    fn layer_intent(id: &str, owner: &str) -> LayerIntent {
        LayerIntent {
            id: LayerId::new(id),
            owner: NodeId::new(owner),
            root: NodeId::new(id),
            anchor: None,
            fallback_anchor: None,
            boundary: None,
            open: true,
            kind: Default::default(),
            placement: Default::default(),
            size: LayerSize::new(Default::default(), Default::default()),
            gap: 0.0,
            offset: [0.0, 0.0],
            collision: Default::default(),
            z_index: 0,
            outside_click: Default::default(),
        }
    }
}
