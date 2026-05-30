use std::borrow::Cow;
use std::cell::RefCell;
use std::time::Instant;

use rustc_hash::{FxHashMap, FxHashSet};

use super::{
    ButtonSkin, CheckboxSkin, DragEvent, Element, ElementBuilder, ElementKind, KeyboardEvent,
    LayoutRect, PanelSkin, PointerEvent, Response, ScrollEvent, SkinRegistry, SliderSkin,
};
use crate::callbacks::UiCallbacks;
use crate::clock::{ClockPeriodMap, UiClock};
use crate::retained::{
    RetainedComposeAction, RetainedComposeEvent, RetainedComposeReason, RetainedComposeStats,
    RetainedRoot, RetainedTiming, ScopeComposeRecord, ScopeRoots, ScopeSet,
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
    previous_clock_periods: Option<ClockPeriodMap>,
    dirty_root_ids: FxHashSet<String>,
    previous_frame_cache: RefCell<FxHashMap<String, Option<LayoutRect>>>,
    path: Vec<usize>,
    element_stack: Vec<String>,
    scope_stack: Vec<String>,
    dependency_owner_stack: Vec<String>,
    dirty_owner_stack: Vec<String>,
    scope_roots: ScopeRoots,
    dirty_scopes: ScopeSet,
    live_scopes: ScopeSet,
    clock_ids: ScopeSet,
    clock_periods: Option<ClockPeriodMap>,
    scope_reuse_enabled: bool,
    responses: FxHashMap<String, Response>,
    callbacks: UiCallbacks,
    previous_callbacks: UiCallbacks,
    skins: SkinRegistry,
    generated_id: usize,
    focused_id: Option<String>,
    retained_stats: RetainedComposeStats,
    diagnostics_enabled: bool,
    retained_timing: RetainedTiming,
    retained_events: Vec<RetainedComposeEvent>,
    scope_compose_records: Vec<ScopeComposeRecord>,
    clock_seconds: f64,
    clock_frame_index: u64,
}

pub(crate) struct UiParts {
    pub roots: Vec<Element>,
    pub callbacks: UiCallbacks,
    pub scope_roots: ScopeRoots,
    pub live_scopes: ScopeSet,
    pub clock_ids: ScopeSet,
    pub clock_periods: Option<ClockPeriodMap>,
    pub retained_stats: RetainedComposeStats,
    pub retained_events: Vec<RetainedComposeEvent>,
    pub scope_compose_records: Vec<ScopeComposeRecord>,
    pub previous_scope_roots: ScopeRoots,
    pub previous_roots: Vec<Element>,
}

struct ScopeComposeMetrics {
    build_ms: f32,
    self_build_ms: f32,
    previous_roots: usize,
    current_roots: usize,
    element_count: usize,
}

impl Ui {
    pub fn new(page_id: impl Into<String>) -> Self {
        Self {
            page_id: page_id.into(),
            roots: Vec::new(),
            previous_roots: Vec::new(),
            previous_scope_roots: ScopeRoots::default(),
            previous_clock_periods: None,
            dirty_root_ids: FxHashSet::default(),
            previous_frame_cache: RefCell::new(FxHashMap::default()),
            path: Vec::new(),
            element_stack: Vec::new(),
            scope_stack: Vec::new(),
            dependency_owner_stack: Vec::new(),
            dirty_owner_stack: Vec::new(),
            scope_roots: ScopeRoots::default(),
            dirty_scopes: FxHashSet::default(),
            live_scopes: FxHashSet::default(),
            clock_ids: FxHashSet::default(),
            clock_periods: None,
            scope_reuse_enabled: false,
            responses: FxHashMap::default(),
            callbacks: UiCallbacks::default(),
            previous_callbacks: UiCallbacks::default(),
            skins: SkinRegistry::default(),
            generated_id: 0,
            focused_id: None,
            retained_stats: RetainedComposeStats::default(),
            diagnostics_enabled: cfg!(debug_assertions),
            retained_timing: RetainedTiming::default(),
            retained_events: Vec::new(),
            scope_compose_records: Vec::new(),
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

    pub(crate) fn into_parts(self) -> UiParts {
        UiParts {
            roots: self.roots,
            callbacks: self.callbacks,
            scope_roots: self.scope_roots,
            live_scopes: self.live_scopes,
            clock_ids: self.clock_ids,
            clock_periods: self.clock_periods,
            retained_stats: self.retained_stats,
            retained_events: self.retained_events,
            scope_compose_records: self.scope_compose_records,
            previous_scope_roots: self.previous_scope_roots,
            previous_roots: self.previous_roots,
        }
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
        previous_clock_periods: Option<ClockPeriodMap>,
    ) {
        self.previous_scope_roots = previous_scope_roots;
        self.dirty_root_ids = dirty_root_ids_for_scopes(&dirty_scopes, &self.previous_scope_roots);
        self.dirty_scopes = dirty_scopes;
        self.previous_callbacks = previous_callbacks;
        self.previous_clock_periods = previous_clock_periods;
        self.scope_reuse_enabled = true;
    }

    pub(crate) fn set_profile_timing(&mut self, enabled: bool) {
        self.retained_timing.set_enabled(enabled);
    }

    pub(crate) fn set_diagnostics_enabled(&mut self, enabled: bool) {
        self.diagnostics_enabled = enabled;
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
            clock_ids: &mut self.clock_ids,
            clock_periods: &mut self.clock_periods,
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

    #[cfg(test)]
    pub(crate) fn retained_scope(&mut self, id: impl Into<String>, build: impl FnOnce(&mut Ui)) {
        let id = self.resolve_scope_id(&id.into());
        self.build_scope(id, build);
    }

    #[cfg(test)]
    pub(crate) fn retained_live_scope(
        &mut self,
        id: impl Into<String>,
        build: impl FnOnce(&mut Ui),
    ) {
        let id = self.resolve_scope_id(&id.into());
        self.live_scopes.insert(id.clone());
        self.build_scope(id, build);
    }

    #[cfg(test)]
    fn build_scope(&mut self, id: String, build: impl FnOnce(&mut Ui)) {
        // Reusing a scope transfers its previous elements and callbacks as a
        // unit. If the scope itself or any nested scope is dirty, rebuild it so
        // signal reads and callbacks capture fresh state.
        let reuse_blocker = self.retained_reuse_blocker(&id);
        if reuse_blocker.is_none() {
            if let Some(elements) = self.previous_elements_for_scope(&id) {
                self.callbacks
                    .transfer_for_elements(&mut self.previous_callbacks, &elements);
                self.preserve_clock_dependencies_for_reused_scope(&id);
                let children = children_at_path_mut(&mut self.roots, &self.path);
                children.extend(elements.clone());
                self.record_scope_compose(
                    id.clone(),
                    RetainedComposeAction::Reused,
                    RetainedComposeReason::CleanReuse,
                    ScopeComposeMetrics {
                        build_ms: 0.0,
                        self_build_ms: 0.0,
                        previous_roots: elements.len(),
                        current_roots: elements.len(),
                        element_count: count_elements(&elements),
                    },
                );
                self.record_scope_roots(id, &elements);
                self.retained_stats.reused += 1;
                return;
            }
        }

        let build_reason = reuse_blocker.unwrap_or(RetainedComposeReason::MissingPreviousElement);
        let pushed_dirty_owner = self.push_dirty_owner_if_exact_dirty(&id);
        let id_for_reset = id.clone();
        self.scope_stack.push(id);
        let start = children_at_path_mut(&mut self.roots, &self.path).len();
        self.schedule_rebuilt_scope_dependency_reset(&id_for_reset);
        let build_start = self.begin_scope_timing();
        build(self);
        let (build_ms, self_build_ms) = self.finish_scope_timing(build_start);
        let id = self
            .scope_stack
            .pop()
            .expect("scope stack should contain active scope");
        if pushed_dirty_owner {
            self.dirty_owner_stack
                .pop()
                .expect("dirty owner stack should contain active scope");
        }
        let roots = children_at_path_mut(&mut self.roots, &self.path)[start..].to_vec();
        self.record_scope_compose(
            id.clone(),
            RetainedComposeAction::Built,
            build_reason,
            ScopeComposeMetrics {
                build_ms,
                self_build_ms,
                previous_roots: self
                    .previous_scope_roots
                    .get(&id)
                    .map_or(0, |roots| roots.len()),
                current_roots: roots.len(),
                element_count: count_elements(&roots),
            },
        );
        self.record_scope_roots(id, &roots);
        self.retained_stats.built += 1;
    }

    #[cfg(test)]
    pub(crate) fn active_scope_id(&self) -> Option<String> {
        self.scope_stack
            .last()
            .cloned()
            .or_else(|| (!self.page_id.is_empty()).then(|| self.page_id.clone()))
    }

    pub(crate) fn dependency_owner_id(&self) -> Option<String> {
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
        self.schedule_rebuilt_scope_dependency_reset(&id);
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

    pub(crate) fn schedule_rebuilt_scope_dependency_reset(&mut self, scope: &str) {
        if self.scope_reuse_enabled
            && self.previous_scope_roots.contains_key(scope)
            && retained_element_has_dirty_dependency(
                &self.dirty_root_ids,
                &self.previous_scope_roots,
                scope,
            )
        {
            crate::signal::schedule_scope_dependency_reset(scope);
        }
    }

    pub(crate) fn reuse_retained_element(&mut self, id: &str) -> bool {
        let reuse_blocker = self.retained_reuse_blocker(id);
        if !self.dirty_owner_stack.is_empty() || reuse_blocker.is_some() {
            return false;
        }
        let Some(elements) = self.previous_elements_for_scope(id) else {
            return false;
        };
        self.callbacks
            .transfer_for_elements(&mut self.previous_callbacks, &elements);
        self.preserve_clock_dependencies_for_reused_scope(id);
        let children = children_at_path_mut(&mut self.roots, &self.path);
        children.extend(elements.clone());
        self.record_scope_compose(
            id.to_string(),
            RetainedComposeAction::Reused,
            RetainedComposeReason::CleanReuse,
            ScopeComposeMetrics {
                build_ms: 0.0,
                self_build_ms: 0.0,
                previous_roots: elements.len(),
                current_roots: elements.len(),
                element_count: count_elements(&elements),
            },
        );
        self.record_scope_roots(id.to_string(), &elements);
        self.retained_stats.reused += 1;
        true
    }

    pub(crate) fn record_retained_element(
        &mut self,
        id: String,
        index: usize,
        build_ms: f32,
        self_build_ms: f32,
    ) {
        let element = children_at_path_mut(&mut self.roots, &self.path)[index].clone();
        let roots = vec![element];
        self.record_scope_compose(
            id.clone(),
            RetainedComposeAction::Built,
            self.retained_build_reason(&id)
                .unwrap_or(RetainedComposeReason::MissingPreviousElement),
            ScopeComposeMetrics {
                build_ms,
                self_build_ms,
                previous_roots: self
                    .previous_scope_roots
                    .get(&id)
                    .map_or(0, |roots| roots.len()),
                current_roots: roots.len(),
                element_count: count_elements(&roots),
            },
        );
        self.record_scope_roots(id, &roots);
        self.retained_stats.built += 1;
    }

    fn retained_build_reason(&self, id: &str) -> Option<RetainedComposeReason> {
        if !self.dirty_owner_stack.is_empty() {
            return Some(RetainedComposeReason::DirtyAncestor);
        }
        self.retained_reuse_blocker(id)
    }

    fn retained_reuse_blocker(&self, id: &str) -> Option<RetainedComposeReason> {
        if !self.scope_reuse_enabled {
            return Some(RetainedComposeReason::RetainedReuseUnavailable);
        }
        if !self.previous_scope_roots.contains_key(id) {
            return Some(RetainedComposeReason::MissingPreviousRoots);
        }
        if self.dirty_scopes.contains(id) {
            return Some(RetainedComposeReason::DirtyScope);
        }
        if retained_element_has_dirty_dependency(
            &self.dirty_root_ids,
            &self.previous_scope_roots,
            id,
        ) {
            return Some(RetainedComposeReason::DirtyDescendant);
        }
        None
    }

    fn record_scope_compose(
        &mut self,
        id: String,
        action: RetainedComposeAction,
        reason: RetainedComposeReason,
        metrics: ScopeComposeMetrics,
    ) {
        if !self.diagnostics_enabled {
            return;
        }
        self.retained_events.push(RetainedComposeEvent {
            id: id.clone(),
            action,
            reason,
        });
        self.scope_compose_records.push(ScopeComposeRecord {
            id,
            action,
            reason,
            build_ms: metrics.build_ms,
            self_build_ms: metrics.self_build_ms,
            previous_roots: metrics.previous_roots,
            current_roots: metrics.current_roots,
            element_count: metrics.element_count,
        });
    }

    fn record_scope_roots(&mut self, id: String, elements: &[Element]) {
        let roots = self
            .retained_timing
            .metadata(|| RetainedRoot::from_elements(elements));
        self.scope_roots.insert(id, roots);
    }

    fn previous_elements_for_scope(&mut self, id: &str) -> Option<Vec<Element>> {
        self.retained_timing.lookup(|| {
            previous_elements_for_scope(&self.previous_roots, &self.previous_scope_roots, id)
        })
    }

    fn preserve_clock_dependencies_for_reused_scope(&mut self, id: &str) {
        let Some(previous_clock_periods) = self.previous_clock_periods.as_ref() else {
            return;
        };
        let Some(period) = previous_clock_periods.get(id).copied() else {
            return;
        };
        self.clock_ids.insert(id.to_string());
        self.clock_periods
            .get_or_insert_with(ClockPeriodMap::default)
            .insert(id.to_string(), period);
    }

    pub(crate) fn begin_scope_timing(&mut self) -> Option<Instant> {
        self.retained_timing.begin_scope()
    }

    pub(crate) fn finish_scope_timing(&mut self, start: Option<Instant>) -> (f32, f32) {
        self.retained_timing.finish_scope(start)
    }

    pub(crate) fn push_dirty_owner_if_exact_dirty(&mut self, id: &str) -> bool {
        if self.dirty_scopes.contains(id) {
            self.dirty_owner_stack.push(id.to_string());
            true
        } else {
            false
        }
    }

    pub(crate) fn pop_dirty_owner(&mut self) {
        self.dirty_owner_stack
            .pop()
            .expect("dirty owner stack should contain pushed owner");
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

    #[cfg(test)]
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

fn dirty_root_ids_for_scopes(
    dirty_scopes: &ScopeSet,
    previous_scope_roots: &ScopeRoots,
) -> FxHashSet<String> {
    dirty_scopes
        .iter()
        .filter_map(|dirty| previous_scope_roots.get(dirty))
        .flat_map(|roots| roots.iter().map(|root| root.id.clone()))
        .collect()
}

fn retained_element_has_dirty_dependency(
    dirty_root_ids: &FxHashSet<String>,
    previous_scope_roots: &ScopeRoots,
    id: &str,
) -> bool {
    previous_scope_roots
        .get(id)
        .is_some_and(|elements| element_tree_contains_any(elements, dirty_root_ids))
}

fn element_tree_contains_any(elements: &[RetainedRoot], ids: &FxHashSet<String>) -> bool {
    elements.iter().any(|element| {
        ids.contains(&element.id) || element_tree_contains_any(&element.children, ids)
    })
}

fn previous_elements_for_scope(
    previous_roots: &[Element],
    previous_scope_roots: &ScopeRoots,
    id: &str,
) -> Option<Vec<Element>> {
    let roots = previous_scope_roots.get(id)?;
    let mut elements = Vec::with_capacity(roots.len());
    for root in roots {
        elements.push(find_element(previous_roots, &root.id)?.clone());
    }
    Some(elements)
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

fn find_element<'a>(elements: &'a [Element], id: &str) -> Option<&'a Element> {
    for element in elements {
        if element.id == id {
            return Some(element);
        }
        if let Some(found) = find_element(&element.children, id) {
            return Some(found);
        }
    }
    None
}
fn count_elements(elements: &[Element]) -> usize {
    elements
        .iter()
        .map(|element| 1 + count_elements(&element.children))
        .sum()
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

        ui.retained_scope("nav", |ui| {
            assert_eq!(ui.active_scope_id().as_deref(), Some("page.nav"));
            ui.retained_scope("selection", |ui| {
                assert_eq!(ui.active_scope_id().as_deref(), Some("page.nav.selection"));
            });
        });
    }
}
