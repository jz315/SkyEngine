use rustc_hash::{FxHashMap, FxHashSet};

use crate::{DragEvent, Element, KeyboardEvent, LayoutRect, PointerEvent, ScrollEvent};

#[derive(Default)]
pub(crate) struct UiCallbacks {
    pub on_click: FxHashMap<String, Box<dyn FnMut()>>,
    pub on_press: FxHashMap<String, Box<dyn FnMut(PointerEvent, LayoutRect)>>,
    pub on_context_menu: FxHashMap<String, Box<dyn FnMut(PointerEvent, LayoutRect)>>,
    pub on_focus_changed: FxHashMap<String, Box<dyn FnMut(bool)>>,
    pub on_text_input: FxHashMap<String, Box<dyn FnMut(KeyboardEvent)>>,
    pub on_scroll: FxHashMap<String, Box<dyn FnMut(ScrollEvent)>>,
    pub on_drag: FxHashMap<String, Box<dyn FnMut(DragEvent)>>,
    pub on_timer: FxHashMap<String, Box<dyn FnMut()>>,
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
            .field("on_timer", &self.on_timer.len())
            .finish()
    }
}

impl UiCallbacks {
    pub(crate) fn transfer_for_elements(
        &mut self,
        previous: &mut UiCallbacks,
        elements: &[Element],
    ) {
        let mut ids = FxHashSet::default();
        collect_element_ids(elements, &mut ids);
        for id in ids {
            if let Some(callback) = previous.on_click.remove(&id) {
                self.on_click.insert(id.clone(), callback);
            }
            if let Some(callback) = previous.on_press.remove(&id) {
                self.on_press.insert(id.clone(), callback);
            }
            if let Some(callback) = previous.on_context_menu.remove(&id) {
                self.on_context_menu.insert(id.clone(), callback);
            }
            if let Some(callback) = previous.on_focus_changed.remove(&id) {
                self.on_focus_changed.insert(id.clone(), callback);
            }
            if let Some(callback) = previous.on_text_input.remove(&id) {
                self.on_text_input.insert(id.clone(), callback);
            }
            if let Some(callback) = previous.on_scroll.remove(&id) {
                self.on_scroll.insert(id.clone(), callback);
            }
            if let Some(callback) = previous.on_drag.remove(&id) {
                self.on_drag.insert(id.clone(), callback);
            }
            if let Some(callback) = previous.on_timer.remove(&id) {
                self.on_timer.insert(id, callback);
            }
        }
    }
}

fn collect_element_ids(elements: &[Element], ids: &mut FxHashSet<String>) {
    for element in elements {
        ids.insert(element.id.clone());
        collect_element_ids(&element.children, ids);
    }
}
