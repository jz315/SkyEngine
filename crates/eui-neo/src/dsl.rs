use std::borrow::Cow;
use std::cell::RefCell;
use std::time::Instant;

use rustc_hash::{FxHashMap, FxHashSet};

use super::{
    ButtonSkin, CheckboxSkin, DragEvent, Element, ElementBuilder, ElementKind, KeyboardEvent,
    LayoutRect, PanelSkin, PointerEvent, Response, ScrollEvent, SkinRegistry, SliderSkin,
};
use crate::callbacks::{
    ClickCallbackId, ContextMenuCallbackId, DragCallbackId, FocusChangedCallbackId,
    LayerDismissCallbackId, PressCallbackId, ScrollCallbackId, TextInputCallbackId,
    TimerCallbackId, UiCallbacks,
};
use crate::clock::{ClockPeriodMap, UiClock};
use crate::retained::{
    RetainedComposeEvent, RetainedComposeReason, RetainedComposeStats, RetainedTiming,
    ScopeComposeRecord, ScopeId, ScopeRoots, ScopeSet,
};
use crate::runtime::reconcile::{
    apply_rebuilt_scope_dependency_reset, apply_retained_build, apply_retained_reuse_plan,
    retained_layer_replay_for_scope, retained_scope_compose_plan, RetainedLayerReplay,
    RetainedReuseContext, RetainedReuseState, RetainedScopeApplied, RetainedScopeBuildPlan,
    RetainedScopeComposePlan,
};
use crate::runtime::{LayerId, LayerIntent, NodeId, ScopeLayerIntents, ScopeLayerRoots};

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
    previous_scope_layer_roots: ScopeLayerRoots,
    previous_scope_layer_intents: ScopeLayerIntents,
    previous_clock_periods: Option<ClockPeriodMap>,
    retained_reuse: RetainedReuseState,
    previous_frame_cache: RefCell<FxHashMap<NodeId, Option<LayoutRect>>>,
    path: Vec<usize>,
    element_stack: Vec<NodeId>,
    scope_stack: Vec<ScopeId>,
    dependency_owner_stack: Vec<ScopeId>,
    dirty_owner_stack: Vec<ScopeId>,
    scope_roots: ScopeRoots,
    scope_layer_roots: ScopeLayerRoots,
    scope_layer_intents: ScopeLayerIntents,
    live_scopes: ScopeSet,
    clock_ids: ScopeSet,
    clock_periods: Option<ClockPeriodMap>,
    responses: FxHashMap<NodeId, Response>,
    callbacks: UiCallbacks,
    previous_callbacks: UiCallbacks,
    skins: SkinRegistry,
    generated_id: usize,
    focused_id: Option<NodeId>,
    retained_stats: RetainedComposeStats,
    diagnostics_enabled: bool,
    retained_timing: RetainedTiming,
    retained_events: Vec<RetainedComposeEvent>,
    scope_compose_records: Vec<ScopeComposeRecord>,
    layer_intents: Vec<LayerIntent>,
    clock_seconds: f64,
    clock_frame_index: u64,
}

pub(crate) struct UiParts {
    pub roots: Vec<Element>,
    pub callbacks: UiCallbacks,
    pub scope_roots: ScopeRoots,
    pub scope_layer_roots: ScopeLayerRoots,
    pub scope_layer_intents: ScopeLayerIntents,
    pub live_scopes: ScopeSet,
    pub clock_ids: ScopeSet,
    pub clock_periods: Option<ClockPeriodMap>,
    pub retained_stats: RetainedComposeStats,
    pub retained_events: Vec<RetainedComposeEvent>,
    pub scope_compose_records: Vec<ScopeComposeRecord>,
    pub layer_intents: Vec<LayerIntent>,
    pub previous_scope_roots: ScopeRoots,
    pub previous_roots: Vec<Element>,
}

enum RetainedScopeBegin {
    Reused,
    Build(RetainedScopeBuildPlan),
}

impl Ui {
    pub fn new(page_id: impl Into<String>) -> Self {
        Self {
            page_id: page_id.into(),
            roots: Vec::new(),
            previous_roots: Vec::new(),
            previous_scope_roots: ScopeRoots::default(),
            previous_scope_layer_roots: ScopeLayerRoots::default(),
            previous_scope_layer_intents: ScopeLayerIntents::default(),
            previous_clock_periods: None,
            retained_reuse: RetainedReuseState::default(),
            previous_frame_cache: RefCell::new(FxHashMap::default()),
            path: Vec::new(),
            element_stack: Vec::new(),
            scope_stack: Vec::new(),
            dependency_owner_stack: Vec::new(),
            dirty_owner_stack: Vec::new(),
            scope_roots: ScopeRoots::default(),
            scope_layer_roots: ScopeLayerRoots::default(),
            scope_layer_intents: ScopeLayerIntents::default(),
            live_scopes: ScopeSet::default(),
            clock_ids: ScopeSet::default(),
            clock_periods: None,
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
            layer_intents: Vec::new(),
            clock_seconds: 0.0,
            clock_frame_index: 0,
        }
    }

    pub fn page_id(&self) -> &str {
        &self.page_id
    }

    #[cfg(test)]
    pub(crate) fn roots(&self) -> &[Element] {
        &self.roots
    }

    pub(crate) fn into_parts(self) -> UiParts {
        UiParts {
            roots: self.roots,
            callbacks: self.callbacks,
            scope_roots: self.scope_roots,
            scope_layer_roots: self.scope_layer_roots,
            scope_layer_intents: self.scope_layer_intents,
            live_scopes: self.live_scopes,
            clock_ids: self.clock_ids,
            clock_periods: self.clock_periods,
            retained_stats: self.retained_stats,
            retained_events: self.retained_events,
            scope_compose_records: self.scope_compose_records,
            layer_intents: self.layer_intents,
            previous_scope_roots: self.previous_scope_roots,
            previous_roots: self.previous_roots,
        }
    }

    #[cfg(test)]
    pub(crate) fn into_roots(self) -> Vec<Element> {
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

    pub(crate) fn set_response_node(&mut self, id: NodeId, response: Response) {
        self.responses.insert(id, response);
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
            .insert(NodeId::new(id.into_owned()), frame);
        frame
    }

    pub fn is_focused(&self, id: &str) -> bool {
        let id = self.resolve_id_ref(id);
        self.focused_id
            .as_ref()
            .is_some_and(|focused| focused.as_str() == id.as_ref())
    }

    pub(crate) fn set_previous_roots(&mut self, roots: Vec<Element>) {
        self.previous_roots = roots;
        self.previous_frame_cache.borrow_mut().clear();
    }

    pub(crate) fn set_scope_reuse(
        &mut self,
        previous_scope_roots: ScopeRoots,
        previous_scope_layer_roots: ScopeLayerRoots,
        previous_scope_layer_intents: ScopeLayerIntents,
        dirty_scopes: ScopeSet,
        previous_callbacks: UiCallbacks,
        previous_clock_periods: Option<ClockPeriodMap>,
    ) {
        self.retained_reuse = RetainedReuseState::enabled(dirty_scopes, &previous_scope_roots);
        self.previous_scope_roots = previous_scope_roots;
        self.previous_scope_layer_roots = previous_scope_layer_roots;
        self.previous_scope_layer_intents = previous_scope_layer_intents;
        self.previous_callbacks = previous_callbacks;
        self.previous_clock_periods = previous_clock_periods;
    }

    pub(crate) fn set_profile_timing(&mut self, enabled: bool) {
        self.retained_timing.set_enabled(enabled);
    }

    pub(crate) fn set_diagnostics_enabled(&mut self, enabled: bool) {
        self.diagnostics_enabled = enabled;
    }

    pub(crate) fn with_root_layer<R>(&mut self, build: impl FnOnce(&mut Ui) -> R) -> R {
        let active_scopes = self.active_retained_scope_ids();
        let saved_path = std::mem::take(&mut self.path);
        let root_start = self.roots.len();
        let result = build(self);
        if !active_scopes.is_empty() {
            let root_ids: Vec<_> = self.roots[root_start..]
                .iter()
                .map(|root| NodeId::new(&root.id))
                .collect();
            self.record_scope_layer_roots(&active_scopes, &root_ids);
        }
        self.path = saved_path;
        result
    }

    pub(crate) fn register_layer_intent(&mut self, intent: LayerIntent) {
        let active_scopes = self.active_retained_scope_ids();
        self.record_scope_layer_intent(&active_scopes, &intent);
        self.layer_intents.push(intent);
    }

    pub(crate) fn nearest_clip_ancestor_id(&self) -> Option<NodeId> {
        let mut children = self.roots.as_slice();
        let mut nearest = None;
        for &index in &self.path {
            let element = children.get(index)?;
            if element.clip {
                nearest = Some(NodeId::new(&element.id));
            }
            children = &element.children;
        }
        nearest
    }

    pub(crate) fn register_on_layer_dismiss(&mut self, id: LayerId, callback: Box<dyn FnMut()>) {
        self.callbacks
            .on_layer_dismiss
            .insert(LayerDismissCallbackId::new(id), callback);
    }

    pub(crate) fn set_focused_id(&mut self, id: Option<NodeId>) {
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
    fn build_scope(&mut self, id: ScopeId, build: impl FnOnce(&mut Ui)) {
        let build_plan = match self.begin_retained_scope(&id) {
            RetainedScopeBegin::Reused => return,
            RetainedScopeBegin::Build(plan) => plan,
        };

        let pushed_dirty_owner = self.push_dirty_owner_for_build_plan(&id, build_plan);
        self.scope_stack.push(id);
        let start = children_at_path_mut(&mut self.roots, &self.path).len();
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
        let built = self.retained_timing.metadata(|| {
            apply_retained_build(
                id,
                &roots,
                build_plan.reason,
                build_ms,
                self_build_ms,
                &self.previous_scope_roots,
            )
        });
        self.record_retained_scope_application(built);
    }

    #[cfg(test)]
    pub(crate) fn active_scope_id(&self) -> Option<ScopeId> {
        self.scope_stack
            .last()
            .cloned()
            .or_else(|| (!self.page_id.is_empty()).then(|| ScopeId::new(self.page_id.clone())))
    }

    pub(crate) fn dependency_owner_id(&self) -> Option<ScopeId> {
        self.scope_stack
            .last()
            .cloned()
            .or_else(|| self.dependency_owner_stack.last().cloned())
            .or_else(|| self.element_stack.last().map(ScopeId::from_node))
            .or_else(|| (!self.page_id.is_empty()).then(|| ScopeId::new(self.page_id.clone())))
    }

    pub(crate) fn with_dependency_owner<R>(
        &mut self,
        id: impl AsRef<str>,
        build: impl FnOnce(&mut Ui) -> R,
    ) -> R {
        let id = ScopeId::new(self.resolve_id(id.as_ref()));
        apply_rebuilt_scope_dependency_reset(&self.retained_reuse_context(), &id);
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

    pub(crate) fn compose_retained_element(
        &mut self,
        element: Element,
        content: impl FnOnce(&mut Ui),
    ) -> Response {
        let response = self.response(&element.id);
        let scope_id = ScopeId::new(element.id.clone());
        let build_plan = match self.begin_retained_scope(&scope_id) {
            RetainedScopeBegin::Reused => return response,
            RetainedScopeBegin::Build(plan) => plan,
        };
        let build_start = self.begin_scope_timing();
        let index = self.push_element(element);
        self.push_path(index);
        let pushed_dirty_owner = self.push_dirty_owner_for_build_plan(&scope_id, build_plan);
        content(self);
        if pushed_dirty_owner {
            self.pop_dirty_owner();
        }
        self.pop_path();
        let (build_ms, self_build_ms) = self.finish_scope_timing(build_start);
        self.record_retained_element(scope_id, index, build_ms, self_build_ms, build_plan);
        response
    }

    fn begin_retained_scope(&mut self, id: &ScopeId) -> RetainedScopeBegin {
        let has_dirty_ancestor = !self.dirty_owner_stack.is_empty();
        let retained_reuse = &self.retained_reuse;
        let previous_roots = &self.previous_roots;
        let previous_scope_roots = &self.previous_scope_roots;
        let compose_plan = self.retained_timing.lookup(|| {
            let context = retained_reuse.context(previous_roots, previous_scope_roots);
            retained_scope_compose_plan(&context, id, has_dirty_ancestor)
        });
        match compose_plan {
            RetainedScopeComposePlan::Reuse(reuse_plan) => {
                self.apply_retained_scope_reuse(id, reuse_plan)
                    .expect("reconciler reuse plan should apply");
                RetainedScopeBegin::Reused
            }
            RetainedScopeComposePlan::Build(plan) => {
                apply_rebuilt_scope_dependency_reset(&self.retained_reuse_context(), id);
                RetainedScopeBegin::Build(plan)
            }
        }
    }

    fn apply_retained_scope_reuse(
        &mut self,
        id: &ScopeId,
        reuse_plan: crate::runtime::reconcile::RetainedReusePlan,
    ) -> Result<(), RetainedComposeReason> {
        let layer_replay = retained_layer_replay_for_scope(
            id,
            &self.previous_roots,
            &self.previous_scope_layer_roots,
            &self.previous_scope_layer_intents,
        );
        let mut active_layers = self.layer_intents.clone();
        active_layers.extend(layer_replay.intents.iter().cloned());
        let reuse = apply_retained_reuse_plan(
            id,
            reuse_plan,
            &mut self.callbacks,
            &mut self.previous_callbacks,
            &active_layers,
            &layer_replay.roots,
            &self.previous_scope_roots,
            &self.scope_roots,
            self.previous_clock_periods.as_ref(),
            &mut self.clock_ids,
            &mut self.clock_periods,
        )?;
        let elements = reuse.elements;
        let scope = reuse.scope;
        self.scope_roots.extend(reuse.preserved_scope_roots);
        let children = children_at_path_mut(&mut self.roots, &self.path);
        children.extend(elements);
        self.record_retained_scope_application(scope);
        self.replay_retained_scope_layers(id, layer_replay);
        Ok(())
    }

    pub(crate) fn record_retained_element(
        &mut self,
        id: ScopeId,
        index: usize,
        build_ms: f32,
        self_build_ms: f32,
        build_plan: RetainedScopeBuildPlan,
    ) {
        let element = children_at_path_mut(&mut self.roots, &self.path)[index].clone();
        let roots = vec![element];
        let built = self.retained_timing.metadata(|| {
            apply_retained_build(
                id,
                &roots,
                build_plan.reason,
                build_ms,
                self_build_ms,
                &self.previous_scope_roots,
            )
        });
        self.record_retained_scope_application(built);
    }

    fn retained_reuse_context(&self) -> RetainedReuseContext<'_> {
        self.retained_reuse
            .context(&self.previous_roots, &self.previous_scope_roots)
    }

    fn replay_retained_scope_layers(&mut self, id: &ScopeId, replay: RetainedLayerReplay) {
        let active_scopes = self.active_retained_scope_ids_with(id);
        for intent in replay.intents {
            self.record_scope_layer_intent(&active_scopes, &intent);
            self.upsert_layer_intent(intent);
        }

        let root_ids: Vec<_> = replay
            .roots
            .iter()
            .map(|root| NodeId::new(&root.id))
            .collect();
        self.record_scope_layer_roots(&active_scopes, &root_ids);
        for root in replay.roots {
            if !self.roots.iter().any(|existing| existing.id == root.id) {
                self.roots.push(root);
            }
        }
    }

    fn active_retained_scope_ids(&self) -> Vec<ScopeId> {
        let mut scopes = Vec::with_capacity(self.scope_stack.len() + self.element_stack.len());
        let mut seen = FxHashSet::default();
        for scope in &self.scope_stack {
            if seen.insert(scope.clone()) {
                scopes.push(scope.clone());
            }
        }
        for element in &self.element_stack {
            let scope = ScopeId::from_node(element);
            if seen.insert(scope.clone()) {
                scopes.push(scope);
            }
        }
        scopes
    }

    fn active_retained_scope_ids_with(&self, id: &ScopeId) -> Vec<ScopeId> {
        let mut scopes = self.active_retained_scope_ids();
        if !scopes.iter().any(|scope| scope == id) {
            scopes.push(id.clone());
        }
        scopes
    }

    fn record_scope_layer_intent(&mut self, scopes: &[ScopeId], intent: &LayerIntent) {
        for scope in scopes {
            let intents = self.scope_layer_intents.entry(scope.clone()).or_default();
            if let Some(existing) = intents.iter_mut().find(|existing| existing.id == intent.id) {
                *existing = intent.clone();
            } else {
                intents.push(intent.clone());
            }
        }
    }

    fn record_scope_layer_roots(&mut self, scopes: &[ScopeId], roots: &[NodeId]) {
        if roots.is_empty() {
            return;
        }
        for scope in scopes {
            let scope_roots = self.scope_layer_roots.entry(scope.clone()).or_default();
            for root in roots {
                if !scope_roots.iter().any(|existing| existing == root) {
                    scope_roots.push(root.clone());
                }
            }
        }
    }

    fn upsert_layer_intent(&mut self, intent: LayerIntent) {
        if let Some(existing) = self
            .layer_intents
            .iter_mut()
            .find(|existing| existing.id == intent.id)
        {
            *existing = intent;
        } else {
            self.layer_intents.push(intent);
        }
    }

    fn record_scope_compose_record(&mut self, record: ScopeComposeRecord) {
        if !self.diagnostics_enabled {
            return;
        }
        self.retained_events.push(record.event());
        self.scope_compose_records.push(record);
    }

    fn record_retained_scope_application(&mut self, applied: RetainedScopeApplied) {
        let action = applied.action();
        let (id, retained_roots, record) = applied.into_parts();
        self.record_scope_compose_record(record);
        self.scope_roots.insert(id, retained_roots);
        match action {
            crate::retained::RetainedComposeAction::Built => self.retained_stats.built += 1,
            crate::retained::RetainedComposeAction::Reused => self.retained_stats.reused += 1,
        }
    }

    pub(crate) fn begin_scope_timing(&mut self) -> Option<Instant> {
        self.retained_timing.begin_scope()
    }

    pub(crate) fn finish_scope_timing(&mut self, start: Option<Instant>) -> (f32, f32) {
        self.retained_timing.finish_scope(start)
    }

    fn push_dirty_owner_for_build_plan(
        &mut self,
        id: &ScopeId,
        build_plan: RetainedScopeBuildPlan,
    ) -> bool {
        if build_plan.dirty_owner {
            self.dirty_owner_stack.push(id.clone());
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
        self.element_stack.push(NodeId::new(id));
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
    fn resolve_scope_id(&self, id: &str) -> ScopeId {
        if id.is_empty() || self.page_id.is_empty() || is_resolved_id(id, &self.page_id) {
            return ScopeId::new(self.resolve_id(id));
        }
        if let Some(parent) = self.scope_stack.last() {
            if is_resolved_id(id, parent.as_str()) || id == parent.as_str() {
                ScopeId::new(id)
            } else {
                ScopeId::new(format!("{parent}.{id}"))
            }
        } else {
            ScopeId::new(self.resolve_id(id))
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

    pub(crate) fn register_on_click(&mut self, id: NodeId, callback: Box<dyn FnMut()>) {
        self.callbacks
            .on_click
            .insert(ClickCallbackId::new(id), callback);
    }

    pub(crate) fn register_on_press(
        &mut self,
        id: NodeId,
        callback: Box<dyn FnMut(PointerEvent, LayoutRect)>,
    ) {
        self.callbacks
            .on_press
            .insert(PressCallbackId::new(id), callback);
    }

    pub(crate) fn register_on_context_menu(
        &mut self,
        id: NodeId,
        callback: Box<dyn FnMut(PointerEvent, LayoutRect)>,
    ) {
        self.callbacks
            .on_context_menu
            .insert(ContextMenuCallbackId::new(id), callback);
    }

    pub(crate) fn register_on_focus_changed(&mut self, id: NodeId, callback: Box<dyn FnMut(bool)>) {
        self.callbacks
            .on_focus_changed
            .insert(FocusChangedCallbackId::new(id), callback);
    }

    pub(crate) fn register_on_text_input(
        &mut self,
        id: NodeId,
        callback: Box<dyn FnMut(KeyboardEvent)>,
    ) {
        self.callbacks
            .on_text_input
            .insert(TextInputCallbackId::new(id), callback);
    }

    pub(crate) fn register_on_scroll(&mut self, id: NodeId, callback: Box<dyn FnMut(ScrollEvent)>) {
        self.callbacks
            .on_scroll
            .insert(ScrollCallbackId::new(id), callback);
    }

    pub(crate) fn register_on_drag(&mut self, id: NodeId, callback: Box<dyn FnMut(DragEvent)>) {
        self.callbacks
            .on_drag
            .insert(DragCallbackId::new(id), callback);
    }

    pub(crate) fn register_on_timer(&mut self, id: NodeId, callback: Box<dyn FnMut()>) {
        self.callbacks
            .on_timer
            .insert(TimerCallbackId::new(id), callback);
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
            assert_eq!(
                ui.active_scope_id().as_ref().map(|id| id.as_str()),
                Some("page.nav")
            );
            ui.retained_scope("selection", |ui| {
                assert_eq!(
                    ui.active_scope_id().as_ref().map(|id| id.as_str()),
                    Some("page.nav.selection")
                );
            });
        });
    }
}
