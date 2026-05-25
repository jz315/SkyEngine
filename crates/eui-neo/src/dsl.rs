use rustc_hash::FxHashMap;

use super::{
    ButtonSkin, CheckboxSkin, DragEvent, Element, ElementBuilder, ElementKind, KeyboardEvent,
    LayoutRect, PanelSkin, PointerEvent, Response, ScrollEvent, SkinRegistry, SliderSkin,
};

/// Logical screen size supplied to neo composition.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Screen {
    pub width: f32,
    pub height: f32,
}

impl Screen {
    pub fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }
}

/// Declarative UI builder. Applications describe the target tree each frame.
#[derive(Debug, Default)]
pub struct Ui {
    page_id: String,
    roots: Vec<Element>,
    path: Vec<usize>,
    responses: FxHashMap<String, Response>,
    callbacks: UiCallbacks,
    skins: SkinRegistry,
    generated_id: usize,
    focused_id: Option<String>,
}

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

impl Ui {
    pub fn new(page_id: impl Into<String>) -> Self {
        Self {
            page_id: page_id.into(),
            roots: Vec::new(),
            path: Vec::new(),
            responses: FxHashMap::default(),
            callbacks: UiCallbacks::default(),
            skins: SkinRegistry::default(),
            generated_id: 0,
            focused_id: None,
        }
    }

    pub fn page_id(&self) -> &str {
        &self.page_id
    }

    pub fn roots(&self) -> &[Element] {
        &self.roots
    }

    pub fn roots_mut(&mut self) -> &mut [Element] {
        &mut self.roots
    }

    pub(crate) fn into_parts(self) -> (Vec<Element>, UiCallbacks) {
        (self.roots, self.callbacks)
    }

    pub fn into_roots(self) -> Vec<Element> {
        self.into_parts().0
    }

    pub(crate) fn set_skins(&mut self, skins: SkinRegistry) {
        self.skins = skins;
    }

    pub fn skins(&self) -> &SkinRegistry {
        &self.skins
    }

    pub fn button_skin(&self, key: &str) -> Option<&ButtonSkin> {
        self.skins.button(key)
    }

    pub fn panel_skin(&self, key: &str) -> Option<&PanelSkin> {
        self.skins.panel(key)
    }

    pub fn checkbox_skin(&self, key: &str) -> Option<&CheckboxSkin> {
        self.skins.checkbox(key)
    }

    pub fn slider_skin(&self, key: &str) -> Option<&SliderSkin> {
        self.skins.slider(key)
    }

    pub fn set_response(&mut self, id: impl Into<String>, response: Response) {
        self.responses.insert(id.into(), response);
    }

    pub fn response(&self, id: &str) -> Response {
        self.responses
            .get(&self.resolve_id(id))
            .copied()
            .unwrap_or_default()
    }

    pub fn is_focused(&self, id: &str) -> bool {
        self.focused_id.as_deref() == Some(self.resolve_id(id).as_str())
    }

    pub(crate) fn set_focused_id(&mut self, id: Option<String>) {
        self.focused_id = id;
    }

    pub fn row(&mut self, id: impl Into<String>) -> ElementBuilder<'_> {
        self.element(ElementKind::Row, id)
    }

    pub fn column(&mut self, id: impl Into<String>) -> ElementBuilder<'_> {
        self.element(ElementKind::Column, id)
    }

    pub fn stack(&mut self, id: impl Into<String>) -> ElementBuilder<'_> {
        self.element(ElementKind::Stack, id)
    }

    pub fn rect(&mut self, id: impl Into<String>) -> ElementBuilder<'_> {
        self.element(ElementKind::Rect, id)
    }

    pub fn panel(&mut self, id: impl Into<String>) -> ElementBuilder<'_> {
        self.rect(id)
    }

    pub fn text(&mut self, id: impl Into<String>) -> ElementBuilder<'_> {
        self.element(ElementKind::Text, id)
    }

    pub fn label(&mut self, id: impl Into<String>) -> ElementBuilder<'_> {
        self.text(id)
    }

    pub fn image(&mut self, id: impl Into<String>) -> ElementBuilder<'_> {
        self.element(ElementKind::Image, id)
    }

    pub fn nine_slice(&mut self, id: impl Into<String>) -> ElementBuilder<'_> {
        self.element(ElementKind::NineSlice, id)
    }

    pub fn polygon(&mut self, id: impl Into<String>) -> ElementBuilder<'_> {
        self.element(ElementKind::Polygon, id)
    }

    pub(crate) fn push_element(&mut self, element: Element) -> usize {
        let children = children_at_path_mut(&mut self.roots, &self.path);
        let index = children.len();
        children.push(element);
        index
    }

    pub(crate) fn push_path(&mut self, index: usize) {
        self.path.push(index);
    }

    pub(crate) fn pop_path(&mut self) {
        self.path.pop();
    }

    fn element(&mut self, kind: ElementKind, id: impl Into<String>) -> ElementBuilder<'_> {
        let id = id.into();
        let id = if id.is_empty() {
            self.generated_id(kind)
        } else {
            self.resolve_id(&id)
        };
        ElementBuilder::new(self, Element::new(kind, id))
    }

    pub(crate) fn resolve_id(&self, id: &str) -> String {
        if id.is_empty() || self.page_id.is_empty() {
            return id.to_string();
        }
        if is_resolved_id(id, &self.page_id) {
            id.to_string()
        } else {
            let mut resolved = String::with_capacity(self.page_id.len() + 1 + id.len());
            resolved.push_str(&self.page_id);
            resolved.push('.');
            resolved.push_str(id);
            resolved
        }
    }

    fn generated_id(&mut self, kind: ElementKind) -> String {
        let prefix = match kind {
            ElementKind::Row => "__row",
            ElementKind::Column => "__column",
            ElementKind::Stack => "__stack",
            ElementKind::Rect => "__rect",
            ElementKind::Polygon => "__polygon",
            ElementKind::Text => "__text",
            ElementKind::Image => "__image",
            ElementKind::NineSlice => "__nine_slice",
        };
        let id = format!("{prefix}.{}", self.generated_id);
        self.generated_id += 1;
        self.resolve_id(&id)
    }

    pub(crate) fn register_on_click(&mut self, id: String, callback: Box<dyn FnMut()>) {
        self.callbacks.on_click.insert(id, callback);
    }

    pub(crate) fn register_on_press(
        &mut self,
        id: String,
        callback: Box<dyn FnMut(PointerEvent, LayoutRect)>,
    ) {
        self.callbacks.on_press.insert(id, callback);
    }

    pub(crate) fn register_on_context_menu(
        &mut self,
        id: String,
        callback: Box<dyn FnMut(PointerEvent, LayoutRect)>,
    ) {
        self.callbacks.on_context_menu.insert(id, callback);
    }

    pub(crate) fn register_on_focus_changed(&mut self, id: String, callback: Box<dyn FnMut(bool)>) {
        self.callbacks.on_focus_changed.insert(id, callback);
    }

    pub(crate) fn register_on_text_input(
        &mut self,
        id: String,
        callback: Box<dyn FnMut(KeyboardEvent)>,
    ) {
        self.callbacks.on_text_input.insert(id, callback);
    }

    pub(crate) fn register_on_scroll(&mut self, id: String, callback: Box<dyn FnMut(ScrollEvent)>) {
        self.callbacks.on_scroll.insert(id, callback);
    }

    pub(crate) fn register_on_drag(&mut self, id: String, callback: Box<dyn FnMut(DragEvent)>) {
        self.callbacks.on_drag.insert(id, callback);
    }

    pub(crate) fn register_on_timer(&mut self, id: String, callback: Box<dyn FnMut()>) {
        self.callbacks.on_timer.insert(id, callback);
    }
}

fn is_resolved_id(id: &str, page_id: &str) -> bool {
    id.len() > page_id.len()
        && id.as_bytes().get(page_id.len()) == Some(&b'.')
        && id.as_bytes().starts_with(page_id.as_bytes())
}

fn children_at_path_mut<'a>(
    elements: &'a mut Vec<Element>,
    path: &[usize],
) -> &'a mut Vec<Element> {
    if let Some((&index, rest)) = path.split_first() {
        children_at_path_mut(&mut elements[index].children, rest)
    } else {
        elements
    }
}

#[cfg(test)]
mod tests {
    use super::Ui;
    use crate::Size;

    #[test]
    fn content_nests_children_under_parent() {
        let mut ui = Ui::new("test");
        ui.column("root").content(|ui| {
            ui.text("title").text("Hello").build();
            ui.rect("panel").size(100.0, 80.0).build();
        });

        assert_eq!(ui.roots().len(), 1);
        assert_eq!(ui.roots()[0].id, "test.root");
        assert_eq!(ui.roots()[0].children.len(), 2);
        assert_eq!(ui.roots()[0].children[0].id, "test.title");
        assert_eq!(ui.roots()[0].children[1].width, Size::Fixed(100.0));
    }

    #[test]
    fn ids_are_page_prefixed_once() {
        let mut ui = Ui::new("page");
        ui.rect("button").build();
        ui.rect("page.existing").build();
        ui.rect("").build();

        assert_eq!(ui.roots()[0].id, "page.button");
        assert_eq!(ui.roots()[1].id, "page.existing");
        assert_eq!(ui.roots()[2].id, "page.__rect.0");
    }
}
