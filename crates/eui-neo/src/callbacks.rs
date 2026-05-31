use rustc_hash::{FxHashMap, FxHashSet};

use crate::CallbackTransferStats;
use crate::{DragEvent, Element, KeyboardEvent, LayoutRect, PointerEvent, ScrollEvent};

macro_rules! callback_key {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub(crate) struct $name(String);

        impl $name {
            pub(crate) fn new(id: impl Into<String>) -> Self {
                Self(id.into())
            }
        }
    };
}

callback_key!(ClickCallbackId);
callback_key!(PressCallbackId);
callback_key!(ContextMenuCallbackId);
callback_key!(FocusChangedCallbackId);
callback_key!(TextInputCallbackId);
callback_key!(ScrollCallbackId);
callback_key!(DragCallbackId);
callback_key!(LayerDismissCallbackId);
callback_key!(TimerCallbackId);

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
    ) -> CallbackTransferStats {
        let mut ids = FxHashSet::default();
        collect_element_ids(elements, &mut ids);
        let mut stats = CallbackTransferStats::default();
        for id in ids {
            if let Some(callback) = previous.on_click.remove(&ClickCallbackId::new(&id)) {
                self.on_click.insert(ClickCallbackId::new(&id), callback);
                stats.click += 1;
            }
            if let Some(callback) = previous.on_press.remove(&PressCallbackId::new(&id)) {
                self.on_press.insert(PressCallbackId::new(&id), callback);
                stats.press += 1;
            }
            if let Some(callback) = previous
                .on_context_menu
                .remove(&ContextMenuCallbackId::new(&id))
            {
                self.on_context_menu
                    .insert(ContextMenuCallbackId::new(&id), callback);
                stats.context_menu += 1;
            }
            if let Some(callback) = previous
                .on_focus_changed
                .remove(&FocusChangedCallbackId::new(&id))
            {
                self.on_focus_changed
                    .insert(FocusChangedCallbackId::new(&id), callback);
                stats.focus_changed += 1;
            }
            if let Some(callback) = previous
                .on_text_input
                .remove(&TextInputCallbackId::new(&id))
            {
                self.on_text_input
                    .insert(TextInputCallbackId::new(&id), callback);
                stats.text_input += 1;
            }
            if let Some(callback) = previous.on_scroll.remove(&ScrollCallbackId::new(&id)) {
                self.on_scroll.insert(ScrollCallbackId::new(&id), callback);
                stats.scroll += 1;
            }
            if let Some(callback) = previous.on_drag.remove(&DragCallbackId::new(&id)) {
                self.on_drag.insert(DragCallbackId::new(&id), callback);
                stats.drag += 1;
            }
            if let Some(callback) = previous
                .on_layer_dismiss
                .remove(&LayerDismissCallbackId::new(&id))
            {
                self.on_layer_dismiss
                    .insert(LayerDismissCallbackId::new(&id), callback);
                stats.layer_dismiss += 1;
            }
            if let Some(callback) = previous.on_timer.remove(&TimerCallbackId::new(&id)) {
                self.on_timer.insert(TimerCallbackId::new(id), callback);
                stats.timer += 1;
            }
        }
        stats
    }

    pub(crate) fn has_text_input(&self, id: &str) -> bool {
        self.on_text_input
            .contains_key(&TextInputCallbackId::new(id))
    }

    pub(crate) fn has_scroll(&self, id: &str) -> bool {
        self.on_scroll.contains_key(&ScrollCallbackId::new(id))
    }

    pub(crate) fn has_drag(&self, id: &str) -> bool {
        self.on_drag.contains_key(&DragCallbackId::new(id))
    }
}

fn collect_element_ids(elements: &[Element], ids: &mut FxHashSet<String>) {
    for element in elements {
        ids.insert(element.id.clone());
        collect_element_ids(&element.children, ids);
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use super::*;
    use crate::{Element, ElementKind};

    #[test]
    fn transfer_for_elements_keeps_callback_roles_separate() {
        let click_count = Rc::new(Cell::new(0));
        let drag_count = Rc::new(Cell::new(0));
        let click_count_callback = click_count.clone();
        let drag_count_callback = drag_count.clone();
        let mut previous = UiCallbacks::default();
        previous.on_click.insert(
            ClickCallbackId::new("page.hit"),
            Box::new(move || click_count_callback.set(click_count_callback.get() + 1)),
        );
        previous.on_drag.insert(
            DragCallbackId::new("page.hit"),
            Box::new(move |_| drag_count_callback.set(drag_count_callback.get() + 1)),
        );

        let mut next = UiCallbacks::default();
        let stats = next.transfer_for_elements(
            &mut previous,
            &[Element::new(ElementKind::Rect, "page.hit")],
        );

        next.on_click
            .get_mut(&ClickCallbackId::new("page.hit"))
            .unwrap()();
        next.on_drag
            .get_mut(&DragCallbackId::new("page.hit"))
            .unwrap()(DragEvent::default());

        assert_eq!(click_count.get(), 1);
        assert_eq!(drag_count.get(), 1);
        assert_eq!(stats.click, 1);
        assert_eq!(stats.drag, 1);
        assert_eq!(stats.total(), 2);
        assert!(!next
            .on_click
            .contains_key(&ClickCallbackId::new("page.missing")));
        assert!(!next
            .on_drag
            .contains_key(&DragCallbackId::new("page.missing")));
    }
}
