use super::*;

impl Runtime {
    pub fn draw_debug_trace(&self) -> crate::diagnostics::UiDrawDebugTrace {
        crate::diagnostics::UiDrawDebugTrace::from_runtime(self)
    }

    pub fn retained_compose_stats(&self) -> RetainedComposeStats {
        self.tree.retained_stats
    }

    pub fn debug_snapshot(&self) -> &UiDebugSnapshot {
        &self.debug.snapshot
    }

    pub fn debug_snapshot_current(&self) -> UiDebugSnapshot {
        let mut snapshot = self.debug.snapshot.clone();
        snapshot.needs_render = self.render.needs_render;
        snapshot.needs_compose = self.render.needs_compose;
        snapshot.full_redraw = self.render.full_redraw;
        snapshot.focused_id = self.input.owners.keyboard_focus_string();
        snapshot.active_id = self.input.owners.pointer_active_string();
        snapshot.hovered_id = hovered_id(&self.input.interactions);
        snapshot.active_animation_count = self
            .animation
            .animations
            .values()
            .filter(|animation| animation.is_active())
            .count();
        snapshot.invalidations = self.invalidation.snapshot();
        snapshot.pass_flags = self.invalidation.pass_flags();
        snapshot.events = self.debug.events.clone();
        snapshot.layers = self.layers.debug_records.clone();
        snapshot.layer_dismissals = self.layers.dismissal_records.clone();
        snapshot.layer_pointer = self.layers.pointer_records.clone();
        snapshot
    }
}

pub(super) struct RuntimeDebugSnapshotInput<'a> {
    pub(super) diagnostics_enabled: bool,
    pub(super) dirty_scopes: &'a ScopeSet,
    pub(super) normalized_dirty_ids: &'a ScopeSet,
    pub(super) scope_debug_records: Vec<RetainedDebugRecord>,
    pub(super) element_debug_records: Vec<ElementDebugRecord>,
    pub(super) retained_events: Vec<RetainedComposeEvent>,
    pub(super) scope_compose_records: Vec<ScopeComposeRecord>,
    pub(super) layout_mode: LayoutMode,
}

pub(super) fn build_runtime_debug_snapshot(
    runtime: &Runtime,
    source: RuntimeDebugSnapshotInput<'_>,
) -> UiDebugSnapshot {
    if !source.diagnostics_enabled {
        return UiDebugSnapshot {
            frame_index: runtime.tree.frame_index,
            screen: runtime.tree.screen,
            retained_stats: runtime.tree.retained_stats,
            layout_mode: source.layout_mode,
            needs_render: runtime.render.needs_render,
            needs_compose: runtime.render.needs_compose,
            full_redraw: runtime.render.full_redraw,
            invalidations: runtime.invalidation.snapshot(),
            pass_flags: runtime.invalidation.pass_flags(),
            events: runtime.debug.events.clone(),
            layers: runtime.layers.debug_records.clone(),
            layer_dismissals: runtime.layers.dismissal_records.clone(),
            layer_pointer: runtime.layers.pointer_records.clone(),
            ..UiDebugSnapshot::default()
        };
    }

    UiDebugSnapshot {
        frame_index: runtime.tree.frame_index,
        screen: runtime.tree.screen,
        dirty_ids: sorted_scope_set(source.dirty_scopes),
        normalized_dirty_ids: sorted_scope_set(source.normalized_dirty_ids),
        live_ids: sorted_scope_set(&runtime.tree.live_ids),
        clock_ids: sorted_scope_set(&runtime.tree.clock_ids),
        retained: source.scope_debug_records,
        elements: source.element_debug_records,
        events: runtime.debug.events.clone(),
        layers: runtime.layers.debug_records.clone(),
        layer_dismissals: runtime.layers.dismissal_records.clone(),
        layer_pointer: runtime.layers.pointer_records.clone(),
        retained_events: source.retained_events,
        scope_compose: source.scope_compose_records,
        retained_stats: runtime.tree.retained_stats,
        layout_mode: source.layout_mode,
        needs_render: runtime.render.needs_render,
        needs_compose: runtime.render.needs_compose,
        full_redraw: runtime.render.full_redraw,
        focused_id: runtime.input.owners.keyboard_focus_string(),
        active_id: runtime.input.owners.pointer_active_string(),
        hovered_id: hovered_id(&runtime.input.interactions),
        active_animation_count: runtime
            .animation
            .animations
            .values()
            .filter(|animation| animation.is_active())
            .count(),
        invalidations: runtime.invalidation.snapshot(),
        pass_flags: runtime.invalidation.pass_flags(),
    }
}

pub(super) fn neo_debug_trace_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("SKY_NEO_DEBUG_TRACE").is_some())
}

pub(super) fn neo_diagnostics_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED
        .get_or_init(|| cfg!(debug_assertions) || std::env::var_os("SKY_NEO_DIAGNOSTICS").is_some())
}

pub(super) fn neo_structure_trace_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("SKY_NEO_STRUCTURE_TRACE").is_some())
}

pub(super) fn trace_debug_snapshot(snapshot: &UiDebugSnapshot) {
    eprintln!(
        "[eui-neo debug] frame={} layout={:?} dirty={:?} normalized_dirty={:?} invalidations={} pass_flags={:?} live={:?} clock={:?} built={} reused={} animations={} focused={:?} hovered={:?} active={:?}",
        snapshot.frame_index,
        snapshot.layout_mode,
        snapshot.dirty_ids,
        snapshot.normalized_dirty_ids,
        snapshot.invalidations.len(),
        snapshot.pass_flags,
        snapshot.live_ids,
        snapshot.clock_ids,
        snapshot.retained_stats.built,
        snapshot.retained_stats.reused,
        snapshot.active_animation_count,
        snapshot.focused_id,
        snapshot.hovered_id,
        snapshot.active_id,
    );
    for event in &snapshot.retained_events {
        eprintln!("[eui-neo retained] {:?} {}", event.action, event.id);
    }
    for event in &snapshot.events {
        eprintln!(
            "[eui-neo event] raw={} command={} target={}:{} callback={} invalidation={}",
            event.raw_event,
            event.command,
            event.target.role(),
            event.target.id(),
            event.callback,
            event.invalidation.is_some(),
        );
    }
    for layer in &snapshot.layers {
        eprintln!(
            "[eui-neo layer] {:?} id={} owner={} anchor={:?} source={:?} open={} kind={:?} placement={:?} z={}",
            layer.action,
            layer.id,
            layer.owner,
            layer.anchor,
            layer.anchor_source,
            layer.open,
            layer.kind,
            layer.placement,
            layer.z_index,
        );
    }
    for dismissal in &snapshot.layer_dismissals {
        eprintln!(
            "[eui-neo layer-dismiss] id={} owner={} policy={:?}",
            dismissal.id, dismissal.owner, dismissal.policy,
        );
    }
    for pointer in &snapshot.layer_pointer {
        eprintln!(
            "[eui-neo layer-pointer] action={:?} layer={:?} hit={:?} policy={:?}",
            pointer.action, pointer.layer, pointer.hit_layer, pointer.policy,
        );
    }
    trace_debug_filters(snapshot);
}

pub(super) fn trace_debug_filters(snapshot: &UiDebugSnapshot) {
    if std::env::var_os("SKY_NEO_DEBUG_DIRTY").is_some() {
        for record in snapshot.retained.iter().filter(|record| record.dirty) {
            eprintln!(
                "[eui-neo dirty] id={} raw={} normalized={} reasons={:?} action={:?} compose_reason={:?} roots={}->{} anchor={:?}",
                record.id,
                record.raw_dirty,
                record.normalized_dirty_root,
                record.dirty_reasons,
                record.action,
                record.compose_reason,
                record.previous_roots,
                record.current_roots,
                record.layout_anchor,
            );
        }
    }
    if let Ok(filter) = std::env::var("SKY_NEO_DEBUG_RETAINED") {
        for record in snapshot
            .retained
            .iter()
            .filter(|record| record.id.contains(&filter))
        {
            eprintln!(
                "[eui-neo retained-debug] id={} parent={:?} dirty={} raw={} normalized={} reasons={:?} action={:?} compose_reason={:?} scroll={:?} clip={:?} anchor={:?}",
                record.id,
                record.parent_id,
                record.dirty,
                record.raw_dirty,
                record.normalized_dirty_root,
                record.dirty_reasons,
                record.action,
                record.compose_reason,
                record.scroll_ancestor,
                record.clip_ancestor,
                record.layout_anchor,
            );
        }
    }
    if let Ok(filter) = std::env::var("SKY_NEO_DEBUG_ELEMENT") {
        for element in snapshot
            .elements
            .iter()
            .filter(|element| element.id.contains(&filter))
        {
            eprintln!(
                "[eui-neo element] id={} parent={:?} boundary={:?} scroll={:?} clip={:?} target={:?} draw={:?}",
                element.id,
                element.parent,
                element.retained_boundary,
                element.scroll_ancestor,
                element.clip_ancestor,
                element.target_frame,
                element.draw_frame,
            );
        }
    }
}

pub(super) fn sorted_scope_set(scopes: &ScopeSet) -> Vec<String> {
    let mut scopes: Vec<_> = scopes
        .iter()
        .map(|scope| scope.as_str().to_string())
        .collect();
    scopes.sort();
    scopes
}

pub(super) fn hovered_id(interactions: &FxHashMap<String, InteractionState>) -> Option<String> {
    interactions
        .iter()
        .find_map(|(id, state)| state.hovered.then(|| id.clone()))
}

pub(super) fn collect_scope_debug_records(
    current_scope_roots: &ScopeRoots,
    previous_scope_roots: &ScopeRoots,
    input_dirty_ids: &ScopeSet,
    live_dirty_ids: &ScopeSet,
    previous_clock_ids: &ScopeSet,
    dirty_ids: &ScopeSet,
    normalized_dirty_ids: &ScopeSet,
    element_records: &[ElementDebugRecord],
    retained_events: &[RetainedComposeEvent],
) -> Vec<RetainedDebugRecord> {
    let mut scope_ids: ScopeSet = ScopeSet::default();
    scope_ids.extend(current_scope_roots.keys().cloned());
    scope_ids.extend(previous_scope_roots.keys().cloned());
    scope_ids.extend(dirty_ids.iter().cloned());
    scope_ids.extend(normalized_dirty_ids.iter().cloned());

    let action_by_scope: FxHashMap<_, _> = retained_events
        .iter()
        .map(|event| (event.id.clone(), event.action))
        .collect();
    let reason_by_scope: FxHashMap<_, _> = retained_events
        .iter()
        .map(|event| (event.id.clone(), event.reason))
        .collect();
    let element_by_id: FxHashMap<_, _> = element_records
        .iter()
        .map(|record| (record.id.as_str(), record))
        .collect();
    let scope_ids_sorted = sorted_scope_set(&scope_ids);

    scope_ids_sorted
        .into_iter()
        .map(|scope| {
            let scope_id = ScopeId::new(scope.clone());
            let first_current_root = current_scope_roots
                .get(scope.as_str())
                .and_then(|roots| roots.first())
                .map(|element| element.id.as_str());
            let first_previous_root = previous_scope_roots
                .get(scope.as_str())
                .and_then(|roots| roots.first());
            let current_record = first_current_root.and_then(|id| element_by_id.get(id).copied());
            RetainedDebugRecord {
                parent_id: retained_parent_for_scope(&scope, current_scope_roots, &element_by_id),
                dirty: dirty_ids.contains(scope.as_str()),
                raw_dirty: input_dirty_ids.contains(scope.as_str()),
                normalized_dirty_root: normalized_dirty_ids.contains(scope.as_str()),
                dirty_reasons: dirty_reasons_for_scope(
                    &scope,
                    input_dirty_ids,
                    live_dirty_ids,
                    previous_clock_ids,
                    dirty_ids,
                    previous_scope_roots,
                ),
                action: action_by_scope.get(&scope_id).copied(),
                compose_reason: reason_by_scope.get(&scope_id).copied(),
                previous_roots: previous_scope_roots
                    .get(scope.as_str())
                    .map_or(0, |roots| roots.len()),
                current_roots: current_scope_roots
                    .get(scope.as_str())
                    .map_or(0, |roots| roots.len()),
                layout_anchor: first_previous_root
                    .map(|element| element.frame)
                    .or_else(|| current_record.map(|record| record.target_frame)),
                scroll_ancestor: current_record.and_then(|record| record.scroll_ancestor.clone()),
                clip_ancestor: current_record.and_then(|record| record.clip_ancestor.clone()),
                id: scope,
            }
        })
        .collect()
}

pub(super) fn retained_parent_for_scope(
    scope: &str,
    current_scope_roots: &ScopeRoots,
    element_by_id: &FxHashMap<&str, &ElementDebugRecord>,
) -> Option<String> {
    let root_id = current_scope_roots
        .get(scope)
        .and_then(|roots| roots.first())
        .map(|element| element.id.as_str())?;
    let parent_id = element_by_id.get(root_id)?.parent.as_deref()?;
    let parent_boundary = element_by_id.get(parent_id)?.retained_boundary.as_deref()?;
    (parent_boundary != scope).then(|| parent_boundary.to_string())
}

pub(super) fn dirty_reasons_for_scope(
    scope: &str,
    input_dirty_ids: &ScopeSet,
    live_dirty_ids: &ScopeSet,
    previous_clock_ids: &ScopeSet,
    dirty_ids: &ScopeSet,
    previous_scope_roots: &ScopeRoots,
) -> Vec<DirtyReason> {
    let mut reasons = Vec::new();
    if input_dirty_ids.contains(scope) {
        reasons.push(DirtyReason::External);
    }
    if live_dirty_ids.contains(scope) {
        if previous_clock_ids.contains(scope) {
            reasons.push(DirtyReason::Clock);
        } else {
            reasons.push(DirtyReason::Live);
        }
    }
    if !dirty_ids.contains(scope)
        && scope_contains_dirty_dependency(scope, dirty_ids, previous_scope_roots)
    {
        reasons.push(DirtyReason::Descendant);
    }
    reasons
}

pub(super) fn scope_contains_dirty_dependency(
    scope: &str,
    dirty_ids: &ScopeSet,
    previous_scope_roots: &ScopeRoots,
) -> bool {
    let Some(scope_roots) = previous_scope_roots.get(scope) else {
        return false;
    };
    dirty_ids.iter().any(|dirty| {
        previous_scope_roots.get(dirty).is_some_and(|dirty_roots| {
            dirty_roots
                .iter()
                .any(|root| retained_roots_contain_id(scope_roots, &root.id))
        })
    })
}

pub(super) fn retained_roots_contain_id(elements: &[RetainedRoot], id: &str) -> bool {
    elements
        .iter()
        .any(|element| element.id == id || retained_roots_contain_id(&element.children, id))
}

pub(super) fn collect_element_debug_records(
    roots: &[Element],
    scope_roots: &ScopeRoots,
    callbacks: &UiCallbacks,
    animations: &FxHashMap<String, ElementAnimation>,
) -> Vec<ElementDebugRecord> {
    let mut records = Vec::new();
    let scope_by_root_id = scope_by_root_id(scope_roots);
    for root in roots {
        collect_element_debug_record(
            root,
            None,
            None,
            None,
            None,
            &scope_by_root_id,
            callbacks,
            animations,
            &mut records,
        );
    }
    records
}

#[allow(clippy::too_many_arguments)]
pub(super) fn collect_element_debug_record(
    element: &Element,
    parent: Option<&str>,
    scroll_ancestor: Option<&str>,
    clip_ancestor: Option<&str>,
    retained_boundary: Option<&str>,
    scope_by_root_id: &FxHashMap<String, String>,
    callbacks: &UiCallbacks,
    animations: &FxHashMap<String, ElementAnimation>,
    records: &mut Vec<ElementDebugRecord>,
) {
    let retained_boundary = scope_by_root_id
        .get(&element.id)
        .map(String::as_str)
        .or(retained_boundary);
    let draw_frame = animations
        .get(&element.id)
        .and_then(|animation| animation.frame.as_ref())
        .map(AnimatedValue::current)
        .unwrap_or(element.frame);
    records.push(ElementDebugRecord {
        id: element.id.clone(),
        parent: parent.map(str::to_string),
        retained_boundary: retained_boundary.map(str::to_string),
        scroll_ancestor: scroll_ancestor.map(str::to_string),
        clip_ancestor: clip_ancestor.map(str::to_string),
        target_frame: element.frame,
        draw_frame: Some(draw_frame),
    });

    let next_scroll_ancestor = if callbacks.has_scroll(&element.id) {
        Some(element.id.as_str())
    } else {
        scroll_ancestor
    };
    let next_clip_ancestor = if element.clip {
        Some(element.id.as_str())
    } else {
        clip_ancestor
    };
    for child in &element.children {
        collect_element_debug_record(
            child,
            Some(&element.id),
            next_scroll_ancestor,
            next_clip_ancestor,
            retained_boundary,
            scope_by_root_id,
            callbacks,
            animations,
            records,
        );
    }
}

pub(super) fn scope_by_root_id(scope_roots: &ScopeRoots) -> FxHashMap<String, String> {
    let mut candidates: FxHashMap<String, Vec<String>> = FxHashMap::default();
    for (scope, roots) in scope_roots {
        for root in roots {
            candidates
                .entry(root.id.clone())
                .or_default()
                .push(scope.as_str().to_string());
        }
    }
    let mut map = FxHashMap::default();
    for (root_id, mut scopes) in candidates {
        scopes.sort();
        let owner = scopes
            .iter()
            .find(|scope| *scope == &root_id)
            .cloned()
            .unwrap_or_else(|| {
                scopes
                    .into_iter()
                    .next()
                    .expect("scope root candidate list should not be empty")
            });
        map.insert(root_id, owner);
    }
    map
}
