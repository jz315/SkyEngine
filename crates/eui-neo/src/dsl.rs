use std::borrow::Cow;
use std::cell::RefCell;
use std::time::Duration;

use rustc_hash::{FxHashMap, FxHashSet};

use super::{
    ButtonSkin, CheckboxSkin, DragEvent, Element, ElementBuilder, ElementKind, KeyboardEvent,
    LayoutRect, PanelSkin, PointerEvent, Response, ScrollEvent, SkinRegistry, SliderSkin,
};
use crate::retained::{
    scope_has_dirty_descendant, ScopeComposeAction, ScopeComposeEvent, ScopeComposeStats,
    ScopeRoots, ScopeSet,
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
    previous_roots: Vec<Element>,
    previous_scope_roots: ScopeRoots,
    previous_frame_cache: RefCell<FxHashMap<String, Option<LayoutRect>>>,
    path: Vec<usize>,
    element_stack: Vec<String>,
    scope_stack: Vec<String>,
    dependency_owner_stack: Vec<String>,
    scope_roots: ScopeRoots,
    dirty_scopes: ScopeSet,
    live_scopes: ScopeSet,
    clock_scopes: ScopeSet,
    scope_reuse_enabled: bool,
    responses: FxHashMap<String, Response>,
    callbacks: UiCallbacks,
    previous_callbacks: UiCallbacks,
    skins: SkinRegistry,
    generated_id: usize,
    focused_id: Option<String>,
    scope_stats: ScopeComposeStats,
    scope_events: Vec<ScopeComposeEvent>,
    clock_seconds: f64,
    clock_frame_index: u64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClockTick {
    pub seconds: f32,
    pub frame_index: u64,
    pub period: Duration,
}

#[derive(Debug)]
pub struct UiClock<'ui> {
    seconds: f64,
    frame_index: u64,
    owner: Option<String>,
    live_scopes: &'ui mut ScopeSet,
    clock_scopes: &'ui mut ScopeSet,
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

impl UiCallbacks {
    fn transfer_for_elements(&mut self, previous: &mut UiCallbacks, elements: &[Element]) {
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

impl Ui {
    pub fn new(page_id: impl Into<String>) -> Self {
        Self {
            page_id: page_id.into(),
            roots: Vec::new(),
            previous_roots: Vec::new(),
            previous_scope_roots: ScopeRoots::default(),
            previous_frame_cache: RefCell::new(FxHashMap::default()),
            path: Vec::new(),
            element_stack: Vec::new(),
            scope_stack: Vec::new(),
            dependency_owner_stack: Vec::new(),
            scope_roots: ScopeRoots::default(),
            dirty_scopes: FxHashSet::default(),
            live_scopes: FxHashSet::default(),
            clock_scopes: FxHashSet::default(),
            scope_reuse_enabled: false,
            responses: FxHashMap::default(),
            callbacks: UiCallbacks::default(),
            previous_callbacks: UiCallbacks::default(),
            skins: SkinRegistry::default(),
            generated_id: 0,
            focused_id: None,
            scope_stats: ScopeComposeStats::default(),
            scope_events: Vec::new(),
            clock_seconds: 0.0,
            clock_frame_index: 0,
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

    pub(crate) fn into_parts(
        self,
    ) -> (
        Vec<Element>,
        UiCallbacks,
        ScopeRoots,
        ScopeSet,
        ScopeSet,
        ScopeComposeStats,
        Vec<ScopeComposeEvent>,
    ) {
        (
            self.roots,
            self.callbacks,
            self.scope_roots,
            self.live_scopes,
            self.clock_scopes,
            self.scope_stats,
            self.scope_events,
        )
    }

    pub fn into_roots(self) -> Vec<Element> {
        self.roots
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
            .get(self.resolve_id_ref(id).as_ref())
            .copied()
            .unwrap_or_default()
    }

    /// Return the element frame from the previous composed layout pass.
    ///
    /// Immediate-mode helpers can use this for APIs whose behavior depends on
    /// resolved layout, such as scroll containers with `Size::Fill` viewports.
    pub fn previous_frame(&self, id: &str) -> Option<LayoutRect> {
        let id = self.resolve_id_ref(id);
        if let Some(frame) = self.previous_frame_cache.borrow().get(id.as_ref()) {
            return *frame;
        }
        let frame = find_frame(&self.previous_roots, id.as_ref());
        self.previous_frame_cache
            .borrow_mut()
            .insert(id.into_owned(), frame);
        frame
    }

    pub fn is_focused(&self, id: &str) -> bool {
        self.focused_id.as_deref() == Some(self.resolve_id_ref(id).as_ref())
    }

    pub(crate) fn set_previous_roots(&mut self, roots: Vec<Element>) {
        self.previous_roots = roots;
        self.previous_frame_cache.borrow_mut().clear();
    }

    pub(crate) fn set_scope_reuse(
        &mut self,
        previous_scope_roots: ScopeRoots,
        dirty_scopes: ScopeSet,
        previous_callbacks: UiCallbacks,
    ) {
        self.previous_scope_roots = previous_scope_roots;
        self.dirty_scopes = dirty_scopes;
        self.previous_callbacks = previous_callbacks;
        self.scope_reuse_enabled = true;
    }

    pub(crate) fn with_root_layer<R>(&mut self, build: impl FnOnce(&mut Ui) -> R) -> R {
        let saved_path = std::mem::take(&mut self.path);
        let result = build(self);
        self.path = saved_path;
        result
    }

    pub(crate) fn set_focused_id(&mut self, id: Option<String>) {
        self.focused_id = id;
    }

    pub(crate) fn set_clock(&mut self, seconds: f64, frame_index: u64) {
        self.clock_seconds = seconds.max(0.0);
        self.clock_frame_index = frame_index;
    }

    pub fn clock(&mut self) -> UiClock<'_> {
        UiClock {
            seconds: self.clock_seconds,
            frame_index: self.clock_frame_index,
            owner: self.dependency_owner_id(),
            live_scopes: &mut self.live_scopes,
            clock_scopes: &mut self.clock_scopes,
        }
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

    pub fn scroll_y(&mut self, id: impl Into<String>) -> crate::widgets::ScrollYBuilder<'_> {
        crate::widgets::scroll_y(self, id)
    }

    pub fn scroll_x(&mut self, id: impl Into<String>) -> crate::widgets::ScrollXBuilder<'_> {
        crate::widgets::scroll_x(self, id)
    }

    pub fn scroll_xy(&mut self, id: impl Into<String>) -> crate::widgets::ScrollXYBuilder<'_> {
        crate::widgets::scroll_xy(self, id)
    }

    pub fn popover(&mut self, id: impl Into<String>) -> crate::widgets::PopoverBuilder<'_> {
        crate::widgets::popover(self, id)
    }

    pub fn polygon(&mut self, id: impl Into<String>) -> ElementBuilder<'_> {
        self.element(ElementKind::Polygon, id)
    }

    pub fn scope(&mut self, id: impl Into<String>, build: impl FnOnce(&mut Ui)) {
        let id = self.resolve_scope_id(&id.into());
        self.build_scope(id, build);
    }

    /// Retained scope that is rebuilt on every scoped compose.
    ///
    /// Use this for frame-time or procedural animation. App data should still
    /// flow through [`State`](crate::State) / [`Signal`](crate::Signal); this is
    /// only the retention boundary's "do not reuse me next frame" marker.
    pub fn live_scope(&mut self, id: impl Into<String>, build: impl FnOnce(&mut Ui)) {
        let id = self.resolve_scope_id(&id.into());
        self.live_scopes.insert(id.clone());
        self.build_scope(id, build);
    }

    fn build_scope(&mut self, id: String, build: impl FnOnce(&mut Ui)) {
        // Reusing a scope transfers its previous elements and callbacks as a
        // unit. If the scope itself or any nested scope is dirty, rebuild it so
        // signal reads and callbacks capture fresh state.
        if self.scope_reuse_enabled && !scope_has_dirty_descendant(&self.dirty_scopes, &id) {
            if let Some(elements) = self.previous_scope_roots.get(&id).cloned() {
                self.callbacks
                    .transfer_for_elements(&mut self.previous_callbacks, &elements);
                let children = children_at_path_mut(&mut self.roots, &self.path);
                children.extend(elements.clone());
                self.scope_events.push(ScopeComposeEvent {
                    scope: id.clone(),
                    action: ScopeComposeAction::Reused,
                });
                self.scope_roots.insert(id, elements);
                self.scope_stats.reused += 1;
                return;
            }
        }

        self.scope_stack.push(id);
        let start = children_at_path_mut(&mut self.roots, &self.path).len();
        build(self);
        let id = self
            .scope_stack
            .pop()
            .expect("scope stack should contain active scope");
        let roots = children_at_path_mut(&mut self.roots, &self.path)[start..].to_vec();
        self.scope_events.push(ScopeComposeEvent {
            scope: id.clone(),
            action: ScopeComposeAction::Built,
        });
        self.scope_roots.insert(id, roots);
        self.scope_stats.built += 1;
    }

    pub fn active_scope_id(&self) -> Option<String> {
        self.scope_stack
            .last()
            .cloned()
            .or_else(|| (!self.page_id.is_empty()).then(|| self.page_id.clone()))
    }

    pub fn dependency_owner_id(&self) -> Option<String> {
        self.scope_stack
            .last()
            .cloned()
            .or_else(|| self.dependency_owner_stack.last().cloned())
            .or_else(|| self.element_stack.last().cloned())
            .or_else(|| (!self.page_id.is_empty()).then(|| self.page_id.clone()))
    }

    pub(crate) fn with_dependency_owner<R>(
        &mut self,
        id: impl AsRef<str>,
        build: impl FnOnce(&mut Ui) -> R,
    ) -> R {
        let id = self.resolve_id(id.as_ref());
        self.dependency_owner_stack.push(id);
        let result = build(self);
        self.dependency_owner_stack
            .pop()
            .expect("dependency owner stack should contain pushed owner");
        result
    }

    pub(crate) fn push_element(&mut self, element: Element) -> usize {
        let children = children_at_path_mut(&mut self.roots, &self.path);
        let index = children.len();
        children.push(element);
        index
    }

    pub(crate) fn push_path(&mut self, index: usize) {
        let id = children_at_path_mut(&mut self.roots, &self.path)[index]
            .id
            .clone();
        self.element_stack.push(id);
        self.path.push(index);
    }

    pub(crate) fn pop_path(&mut self) {
        self.path.pop();
        self.element_stack
            .pop()
            .expect("element stack should contain pushed element");
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
        self.resolve_id_ref(id).into_owned()
    }

    fn resolve_scope_id(&self, id: &str) -> String {
        if id.is_empty() || self.page_id.is_empty() || is_resolved_id(id, &self.page_id) {
            return self.resolve_id(id);
        }
        if let Some(parent) = self.scope_stack.last() {
            if is_resolved_id(id, parent) || id == parent {
                id.to_string()
            } else {
                format!("{parent}.{id}")
            }
        } else {
            self.resolve_id(id)
        }
    }

    fn resolve_id_ref<'a>(&self, id: &'a str) -> Cow<'a, str> {
        if id.is_empty() || self.page_id.is_empty() {
            return Cow::Borrowed(id);
        }
        if is_resolved_id(id, &self.page_id) {
            Cow::Borrowed(id)
        } else {
            let mut resolved = String::with_capacity(self.page_id.len() + 1 + id.len());
            resolved.push_str(&self.page_id);
            resolved.push('.');
            resolved.push_str(id);
            Cow::Owned(resolved)
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

impl UiClock<'_> {
    pub fn seconds(mut self) -> f32 {
        self.register_dependency();
        self.seconds as f32
    }

    pub fn frame_index(mut self) -> u64 {
        self.register_dependency();
        self.frame_index
    }

    pub fn every(mut self, period: Duration) -> ClockTick {
        self.register_dependency();
        ClockTick {
            seconds: self.seconds as f32,
            frame_index: self.frame_index,
            period,
        }
    }

    fn register_dependency(&mut self) {
        if let Some(owner) = self.owner.as_ref() {
            self.live_scopes.insert(owner.clone());
            self.clock_scopes.insert(owner.clone());
        }
    }
}

fn find_frame(elements: &[Element], id: &str) -> Option<LayoutRect> {
    for element in elements {
        if element.id == id {
            return Some(element.frame);
        }
        if let Some(frame) = find_frame(&element.children, id) {
            return Some(frame);
        }
    }
    None
}

fn collect_element_ids(elements: &[Element], ids: &mut FxHashSet<String>) {
    for element in elements {
        ids.insert(element.id.clone());
        collect_element_ids(&element.children, ids);
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

    #[test]
    fn nested_scopes_extend_the_parent_scope_id() {
        let mut ui = Ui::new("page");

        ui.scope("nav", |ui| {
            assert_eq!(ui.active_scope_id().as_deref(), Some("page.nav"));
            ui.scope("selection", |ui| {
                assert_eq!(ui.active_scope_id().as_deref(), Some("page.nav.selection"));
            });
        });
    }
}
