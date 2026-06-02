use super::interaction::FrameInputPassReport;
use super::*;
use crate::draw::collect_element_render_debug;

impl Runtime {
    pub fn diagnostics(&self) -> crate::expert::RuntimeDiagnostics<'_> {
        crate::expert::RuntimeDiagnostics::new(self)
    }

    pub(crate) fn diagnostic_draw_trace(&self) -> crate::diagnostics::UiDrawDebugTrace {
        crate::diagnostics::UiDrawDebugTrace::from_runtime(self)
    }

    pub fn retained_compose_stats(&self) -> RetainedComposeStats {
        self.tree.retained_stats
    }

    pub(crate) fn diagnostic_committed_snapshot(&self) -> &UiDebugSnapshot {
        &self.debug.snapshot
    }

    pub(crate) fn diagnostic_current_snapshot(&self) -> UiDebugSnapshot {
        let mut snapshot = self.debug.snapshot.clone();
        snapshot.needs_render = self.render.needs_render;
        snapshot.needs_compose = self.render.needs_compose;
        snapshot.full_redraw = self.render.full_redraw;
        snapshot.focused_id = readable_node_id(self.input.owners.keyboard_focus_node_id());
        snapshot.active_id = readable_node_id(self.input.owners.pointer_active_node_id());
        fill_input_owner_debug_fields(&mut snapshot, &self.input.owners, &self.input.interactions);
        snapshot.active_animation_count = self
            .animation
            .animations
            .values()
            .filter(|animation| animation.is_active())
            .count();
        snapshot.invalidations = self.invalidation.snapshot();
        snapshot.pass_flags = self.invalidation.pass_flags();
        snapshot.events = self.debug.events.clone();
        snapshot.input_pass = self.debug.input_pass.clone();
        snapshot.platform_effects = self.debug.platform_effects.clone();
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

pub(super) struct CompositionDebugInput<'a> {
    pub(super) diagnostics_enabled: bool,
    pub(super) debug_trace: bool,
    pub(super) previous_scope_roots: ScopeRoots,
    pub(super) input_dirty_scopes: &'a ScopeSet,
    pub(super) live_dirty_scopes: &'a ScopeSet,
    pub(super) previous_clock_ids: &'a ScopeSet,
    pub(super) dirty_scopes: &'a ScopeSet,
    pub(super) normalized_dirty_ids: &'a ScopeSet,
    pub(super) retained_events: Vec<RetainedComposeEvent>,
    pub(super) scope_compose_records: Vec<ScopeComposeRecord>,
    pub(super) layout_mode: LayoutMode,
}

pub(super) fn finish_composition_debug(runtime: &mut Runtime, source: CompositionDebugInput<'_>) {
    let element_debug_records = collect_elements_for_debug(runtime, source.diagnostics_enabled);
    let scope_debug_records = collect_scopes_for_debug(
        runtime,
        ScopeDebugSource {
            diagnostics_enabled: source.diagnostics_enabled,
            previous_scope_roots: &source.previous_scope_roots,
            input_dirty_scopes: source.input_dirty_scopes,
            live_dirty_scopes: source.live_dirty_scopes,
            previous_clock_ids: source.previous_clock_ids,
            dirty_scopes: source.dirty_scopes,
            normalized_dirty_ids: source.normalized_dirty_ids,
            element_debug_records: &element_debug_records,
            retained_events: &source.retained_events,
        },
    );
    runtime.debug.snapshot = build_runtime_debug_snapshot(
        runtime,
        RuntimeDebugSnapshotInput {
            diagnostics_enabled: source.diagnostics_enabled,
            dirty_scopes: source.dirty_scopes,
            normalized_dirty_ids: source.normalized_dirty_ids,
            scope_debug_records,
            element_debug_records,
            retained_events: source.retained_events,
            scope_compose_records: source.scope_compose_records,
            layout_mode: source.layout_mode,
        },
    );
    if source.debug_trace {
        trace_debug_snapshot(&runtime.debug.snapshot);
    }
    runtime.clear_committed_invalidations();
    runtime.clear_committed_event_debug_records();
    runtime.clear_committed_input_pass_debug_record();
}

pub(super) fn build_runtime_debug_snapshot(
    runtime: &Runtime,
    source: RuntimeDebugSnapshotInput<'_>,
) -> UiDebugSnapshot {
    #[cfg(feature = "profile")]
    ::profiling::scope!("eui_neo.runtime.snapshot");
    if !source.diagnostics_enabled {
        let mut snapshot = UiDebugSnapshot {
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
            input_pass: runtime.debug.input_pass.clone(),
            platform_effects: runtime.debug.platform_effects.clone(),
            layers: runtime.layers.debug_records.clone(),
            layer_dismissals: runtime.layers.dismissal_records.clone(),
            layer_pointer: runtime.layers.pointer_records.clone(),
            ..UiDebugSnapshot::default()
        };
        fill_input_owner_debug_fields(
            &mut snapshot,
            &runtime.input.owners,
            &runtime.input.interactions,
        );
        return snapshot;
    }

    let mut snapshot = UiDebugSnapshot {
        frame_index: runtime.tree.frame_index,
        screen: runtime.tree.screen,
        dirty_ids: sorted_scope_set(source.dirty_scopes),
        normalized_dirty_ids: sorted_scope_set(source.normalized_dirty_ids),
        live_ids: sorted_scope_set(&runtime.tree.live_ids),
        clock_ids: sorted_scope_set(&runtime.tree.clock_ids),
        dirty_scope_ids: sorted_scope_ids(source.dirty_scopes),
        normalized_dirty_scope_ids: sorted_scope_ids(source.normalized_dirty_ids),
        live_scope_ids: sorted_scope_ids(&runtime.tree.live_ids),
        clock_scope_ids: sorted_scope_ids(&runtime.tree.clock_ids),
        retained: source.scope_debug_records,
        elements: source.element_debug_records,
        events: runtime.debug.events.clone(),
        input_pass: runtime.debug.input_pass.clone(),
        platform_effects: runtime.debug.platform_effects.clone(),
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
        focused_id: readable_node_id(runtime.input.owners.keyboard_focus_node_id()),
        active_id: readable_node_id(runtime.input.owners.pointer_active_node_id()),
        active_animation_count: runtime
            .animation
            .animations
            .values()
            .filter(|animation| animation.is_active())
            .count(),
        invalidations: runtime.invalidation.snapshot(),
        pass_flags: runtime.invalidation.pass_flags(),
        ..UiDebugSnapshot::default()
    };
    fill_input_owner_debug_fields(
        &mut snapshot,
        &runtime.input.owners,
        &runtime.input.interactions,
    );
    snapshot
}

struct ScopeDebugSource<'a> {
    diagnostics_enabled: bool,
    previous_scope_roots: &'a ScopeRoots,
    input_dirty_scopes: &'a ScopeSet,
    live_dirty_scopes: &'a ScopeSet,
    previous_clock_ids: &'a ScopeSet,
    dirty_scopes: &'a ScopeSet,
    normalized_dirty_ids: &'a ScopeSet,
    element_debug_records: &'a [ElementDebugRecord],
    retained_events: &'a [RetainedComposeEvent],
}

fn collect_elements_for_debug(
    runtime: &Runtime,
    diagnostics_enabled: bool,
) -> Vec<ElementDebugRecord> {
    if !diagnostics_enabled {
        return Vec::new();
    }
    #[cfg(feature = "profile")]
    ::profiling::scope!("eui_neo.runtime.element_debug");
    collect_element_debug_records(
        &runtime.tree.roots,
        &runtime.tree.scope_roots,
        &runtime.input.callbacks,
        &collect_element_render_debug(runtime),
    )
}

fn collect_scopes_for_debug(
    runtime: &Runtime,
    source: ScopeDebugSource<'_>,
) -> Vec<RetainedDebugRecord> {
    if !source.diagnostics_enabled {
        return Vec::new();
    }
    #[cfg(feature = "profile")]
    ::profiling::scope!("eui_neo.runtime.scope_debug");
    collect_scope_debug_records(
        &runtime.tree.scope_roots,
        source.previous_scope_roots,
        source.input_dirty_scopes,
        source.live_dirty_scopes,
        source.previous_clock_ids,
        source.dirty_scopes,
        source.normalized_dirty_ids,
        source.element_debug_records,
        source.retained_events,
    )
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
        "[eui-neo debug] frame={} layout={:?} dirty={:?} normalized_dirty={:?} invalidations={} pass_flags={:?} live={:?} clock={:?} built={} reused={} animations={} keyboard={:?} text={:?} ime={:?} hover={:?} active={:?} capture={:?} scroll={:?} drag={:?}",
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
        snapshot.keyboard_focus_id,
        snapshot.text_focus_id,
        snapshot.ime_owner_id,
        snapshot.pointer_hover_id,
        snapshot.pointer_active_id,
        snapshot.pointer_capture_id,
        snapshot.scroll_owner_id,
        snapshot.drag_owner_id,
    );
    if let Some(input) = &snapshot.input_pass {
        eprintln!(
            "[eui-neo input-pass] input_changed={} timer_render={} commands={} callbacks={} invalidations={} pass_flags={:?}",
            input.input_state_changed,
            input.timer_render_requested,
            input.command_count,
            input.callback_count,
            input.invalidation_count,
            input.pass_flags,
        );
    }
    for effect in &snapshot.platform_effects {
        eprintln!("[eui-neo platform] {effect:?}");
    }
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
                "[eui-neo element] id={} parent={:?} boundary={:?} scroll={:?} clip={:?} target={:?} draw={:?} transformed={:?} transform={:?} active_clip={:?} visible={}",
                element.id,
                element.parent,
                element.retained_boundary,
                element.scroll_ancestor,
                element.clip_ancestor,
                element.target_frame,
                element.draw_frame,
                element.transformed_draw_frame,
                element.draw_transform,
                element.active_clip,
                element.draw_visible,
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

pub(super) fn sorted_scope_ids(scopes: &ScopeSet) -> Vec<ScopeId> {
    let mut scopes: Vec<_> = scopes.iter().cloned().collect();
    scopes.sort_by(|left, right| left.as_str().cmp(right.as_str()));
    scopes
}

pub(super) fn hovered_node_id(
    interactions: &FxHashMap<NodeId, InteractionState>,
) -> Option<NodeId> {
    interactions
        .iter()
        .find_map(|(id, state)| state.hovered.then(|| id.clone()))
}

fn fill_input_owner_debug_fields(
    snapshot: &mut UiDebugSnapshot,
    owners: &InputOwners,
    interactions: &FxHashMap<NodeId, InteractionState>,
) {
    let input_owners = InputOwnerDebugSnapshot {
        pointer_hover: owners.pointer_hover_node_id(),
        pointer_active: owners.pointer_active_node_id(),
        pointer_capture: owners.pointer_capture_node_id(),
        keyboard_focus: owners.keyboard_focus_node_id(),
        text_focus: owners.text_focus_node_id(),
        ime_owner: owners.ime_owner_node_id(),
        scroll_owner: owners.scroll_owner_node_id(),
        drag_owner: owners.drag_owner_node_id(),
    };
    snapshot.pointer_hover_id = readable_node_id_ref(input_owners.pointer_hover.as_ref());
    snapshot.pointer_active_id = readable_node_id_ref(input_owners.pointer_active.as_ref());
    snapshot.pointer_capture_id = readable_node_id_ref(input_owners.pointer_capture.as_ref());
    snapshot.keyboard_focus_id = readable_node_id_ref(input_owners.keyboard_focus.as_ref());
    snapshot.text_focus_id = readable_node_id_ref(input_owners.text_focus.as_ref());
    snapshot.ime_owner_id = readable_node_id_ref(input_owners.ime_owner.as_ref());
    snapshot.scroll_owner_id = readable_node_id_ref(input_owners.scroll_owner.as_ref());
    snapshot.drag_owner_id = readable_node_id_ref(input_owners.drag_owner.as_ref());
    snapshot.focused_id = snapshot.keyboard_focus_id.clone();
    snapshot.active_id = snapshot.pointer_active_id.clone();
    let hovered_node_id = hovered_node_id(interactions);
    snapshot.hovered_id = readable_node_id_ref(hovered_node_id.as_ref());
    snapshot.hovered_node_id = hovered_node_id;
    snapshot.input_owners = input_owners;
}

fn readable_node_id(id: Option<NodeId>) -> Option<String> {
    id.map(|id| id.as_str().to_string())
}

fn readable_node_id_ref(id: Option<&NodeId>) -> Option<String> {
    id.map(|id| id.as_str().to_string())
}

impl Runtime {
    pub(super) fn record_input_pass_debug(&mut self, report: &FrameInputPassReport) {
        self.debug.input_pass = Some(FrameInputDebugRecord {
            input_state_changed: report.input_state_changed,
            timer_render_requested: report.timer_render_requested,
            command_count: report.command_report.command_count,
            callback_count: report.command_report.callback_count,
            invalidation_count: report.command_report.invalidation_count,
            pass_flags: report.command_report.pass_flags,
        });
    }

    pub(super) fn clear_committed_input_pass_debug_record(&mut self) {
        self.debug.input_pass = None;
    }
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
    let element_by_id: FxHashMap<NodeId, _> = element_records
        .iter()
        .map(|record| (record.node_id().clone(), record))
        .collect();
    let scope_ids_sorted = sorted_scope_ids(&scope_ids);

    scope_ids_sorted
        .into_iter()
        .map(|scope_id| {
            let first_current_root = current_scope_roots
                .get(&scope_id)
                .and_then(|roots| roots.first())
                .map(|element| &element.id);
            let first_previous_root = previous_scope_roots
                .get(&scope_id)
                .and_then(|roots| roots.first());
            let current_record = first_current_root.and_then(|id| element_by_id.get(id).copied());
            let parent_scope_id =
                retained_parent_for_scope(&scope_id, current_scope_roots, &element_by_id);
            let scroll_ancestor_id =
                current_record.and_then(|record| record.scroll_ancestor_id().cloned());
            let clip_ancestor_id =
                current_record.and_then(|record| record.clip_ancestor_id().cloned());
            RetainedDebugRecord {
                scope_id: scope_id.clone(),
                parent_scope_id: parent_scope_id.clone(),
                scroll_ancestor_id: scroll_ancestor_id.clone(),
                clip_ancestor_id: clip_ancestor_id.clone(),
                parent_id: parent_scope_id
                    .as_ref()
                    .map(|scope| scope.as_str().to_string()),
                dirty: dirty_ids.contains(&scope_id),
                raw_dirty: input_dirty_ids.contains(&scope_id),
                normalized_dirty_root: normalized_dirty_ids.contains(&scope_id),
                dirty_reasons: dirty_reasons_for_scope(
                    &scope_id,
                    input_dirty_ids,
                    live_dirty_ids,
                    previous_clock_ids,
                    dirty_ids,
                    previous_scope_roots,
                ),
                action: action_by_scope.get(&scope_id).copied(),
                compose_reason: reason_by_scope.get(&scope_id).copied(),
                previous_roots: previous_scope_roots
                    .get(&scope_id)
                    .map_or(0, |roots| roots.len()),
                current_roots: current_scope_roots
                    .get(&scope_id)
                    .map_or(0, |roots| roots.len()),
                layout_anchor: first_previous_root
                    .map(|element| element.frame)
                    .or_else(|| current_record.map(|record| record.target_frame)),
                scroll_ancestor: scroll_ancestor_id
                    .as_ref()
                    .map(|id| id.as_str().to_string()),
                clip_ancestor: clip_ancestor_id.as_ref().map(|id| id.as_str().to_string()),
                id: scope_id.as_str().to_string(),
            }
        })
        .collect()
}

pub(super) fn retained_parent_for_scope(
    scope: &ScopeId,
    current_scope_roots: &ScopeRoots,
    element_by_id: &FxHashMap<NodeId, &ElementDebugRecord>,
) -> Option<ScopeId> {
    let root_id = current_scope_roots
        .get(scope)
        .and_then(|roots| roots.first())
        .map(|element| element.id.clone())?;
    let parent_id = element_by_id.get(&root_id)?.parent_id()?;
    let parent_boundary = element_by_id.get(parent_id)?.retained_boundary_id()?;
    (parent_boundary != scope).then(|| parent_boundary.clone())
}

pub(super) fn dirty_reasons_for_scope(
    scope: &ScopeId,
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
    scope: &ScopeId,
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

pub(super) fn retained_roots_contain_id(elements: &[RetainedRoot], id: &NodeId) -> bool {
    elements
        .iter()
        .any(|element| &element.id == id || retained_roots_contain_id(&element.children, id))
}

pub(super) fn collect_element_debug_records(
    roots: &[Element],
    scope_roots: &ScopeRoots,
    callbacks: &UiCallbacks,
    render_debug: &FxHashMap<NodeId, crate::draw::ElementRenderDebug>,
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
            render_debug,
            &mut records,
        );
    }
    records
}

#[allow(clippy::too_many_arguments)]
pub(super) fn collect_element_debug_record(
    element: &Element,
    parent: Option<&NodeId>,
    scroll_ancestor: Option<&NodeId>,
    clip_ancestor: Option<&NodeId>,
    retained_boundary: Option<&ScopeId>,
    scope_by_root_id: &FxHashMap<NodeId, ScopeId>,
    callbacks: &UiCallbacks,
    render_debug: &FxHashMap<NodeId, crate::draw::ElementRenderDebug>,
    records: &mut Vec<ElementDebugRecord>,
) {
    let node_id = NodeId::new(element.id.as_str());
    let retained_boundary = scope_by_root_id.get(&node_id).or(retained_boundary);
    let render_record = render_debug.get(&node_id);
    records.push(ElementDebugRecord {
        node_id: node_id.clone(),
        parent_id: parent.cloned(),
        retained_boundary_id: retained_boundary.cloned(),
        scroll_ancestor_id: scroll_ancestor.cloned(),
        clip_ancestor_id: clip_ancestor.cloned(),
        id: element.id.clone(),
        parent: parent.map(|id| id.as_str().to_string()),
        retained_boundary: retained_boundary.map(|id| id.as_str().to_string()),
        scroll_ancestor: scroll_ancestor.map(|id| id.as_str().to_string()),
        clip_ancestor: clip_ancestor.map(|id| id.as_str().to_string()),
        target_frame: element.frame,
        draw_frame: render_record.map(|record| record.draw_frame),
        transformed_draw_frame: render_record.map(|record| record.transformed_draw_frame),
        draw_transform: render_record.map_or(element.transform, |record| record.draw_transform),
        active_clip: render_record.and_then(|record| record.active_clip),
        draw_visible: render_record.is_none_or(|record| record.visible),
    });

    let next_scroll_ancestor = if callbacks.has_scroll(&node_id) {
        Some(&node_id)
    } else {
        scroll_ancestor
    };
    let next_clip_ancestor = if element.clip {
        Some(&node_id)
    } else {
        clip_ancestor
    };
    for child in &element.children {
        collect_element_debug_record(
            child,
            Some(&node_id),
            next_scroll_ancestor,
            next_clip_ancestor,
            retained_boundary,
            scope_by_root_id,
            callbacks,
            render_debug,
            records,
        );
    }
}

pub(super) fn scope_by_root_id(scope_roots: &ScopeRoots) -> FxHashMap<NodeId, ScopeId> {
    let mut candidates: FxHashMap<NodeId, Vec<ScopeId>> = FxHashMap::default();
    for (scope, roots) in scope_roots {
        for root in roots {
            candidates
                .entry(root.id.clone())
                .or_default()
                .push(scope.clone());
        }
    }
    let mut map = FxHashMap::default();
    for (root_id, mut scopes) in candidates {
        scopes.sort();
        let owner = scopes
            .iter()
            .find(|scope| scope.as_str() == root_id.as_str())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DirtyFlags;

    #[test]
    fn finish_composition_debug_builds_snapshot_and_clears_committed_records() {
        let mut runtime = Runtime::new("page");
        runtime.tree.screen = Screen::new(320.0, 200.0);
        let root = Element::new(ElementKind::Rect, "page.root");
        runtime.tree.roots = vec![root.clone()];
        runtime.tree.scope_roots.insert(
            ScopeId::new("page.scope"),
            RetainedRoot::from_elements(&[root]),
        );
        runtime.record_committed_invalidation_trace(Invalidation::dirty_input(&DirtyInput::new(
            "page.scope",
            DirtyFlags::DRAW,
        )));
        runtime.tree.live_ids.insert(ScopeId::new("page.live"));
        runtime.tree.clock_ids.insert(ScopeId::new("page.clock"));
        runtime.record_event_debug(EventDebugRecord {
            source: EventDebugSource::Runtime {
                raw_event: "test",
                command: "test",
            },
            raw_event: "test",
            target: EventTargetId::node("page.root"),
            command: "test",
            callback: false,
            invalidation: None,
        });
        runtime.debug.input_pass = Some(FrameInputDebugRecord {
            input_state_changed: true,
            timer_render_requested: false,
            command_count: 1,
            callback_count: 0,
            invalidation_count: 0,
            pass_flags: PassFlags::default(),
        });

        let dirty_scopes = scopes(&["page.scope"]);
        let empty_scopes = ScopeSet::default();
        finish_composition_debug(
            &mut runtime,
            CompositionDebugInput {
                diagnostics_enabled: true,
                debug_trace: false,
                previous_scope_roots: ScopeRoots::default(),
                input_dirty_scopes: &dirty_scopes,
                live_dirty_scopes: &empty_scopes,
                previous_clock_ids: &empty_scopes,
                dirty_scopes: &dirty_scopes,
                normalized_dirty_ids: &dirty_scopes,
                retained_events: Vec::new(),
                scope_compose_records: Vec::new(),
                layout_mode: LayoutMode::Full(FullLayoutReason::DirtyRetainedLayoutFailed),
            },
        );

        let snapshot = runtime.diagnostics().committed_snapshot();
        assert_eq!(snapshot.dirty_ids, vec!["page.scope"]);
        assert_eq!(snapshot.normalized_dirty_ids, vec!["page.scope"]);
        assert_eq!(snapshot.live_ids, vec!["page.live"]);
        assert_eq!(snapshot.clock_ids, vec!["page.clock"]);
        assert_eq!(snapshot.dirty_scope_ids, vec![ScopeId::new("page.scope")]);
        assert_eq!(
            snapshot.normalized_dirty_scope_ids,
            vec![ScopeId::new("page.scope")]
        );
        assert_eq!(snapshot.live_scope_ids, vec![ScopeId::new("page.live")]);
        assert_eq!(snapshot.clock_scope_ids, vec![ScopeId::new("page.clock")]);
        assert_eq!(snapshot.elements.len(), 1);
        assert_eq!(snapshot.invalidations.len(), 1);
        assert_eq!(snapshot.events.len(), 1);
        assert_eq!(
            snapshot.events[0].source,
            EventDebugSource::Runtime {
                raw_event: "test",
                command: "test",
            }
        );
        assert_eq!(
            snapshot
                .input_pass
                .as_ref()
                .map(|record| record.command_count),
            Some(1)
        );
        assert!(runtime
            .diagnostics()
            .current_snapshot()
            .invalidations
            .is_empty());
        assert!(runtime.diagnostics().current_snapshot().events.is_empty());
        assert!(runtime
            .diagnostics()
            .current_snapshot()
            .input_pass
            .is_none());
    }

    #[test]
    fn dirty_reasons_use_exact_typed_scope_identity() {
        let scope = ScopeId::new("page.nav");
        let prefixed_dirty = scopes(&["page.nav.extra"]);
        let empty_scopes = ScopeSet::default();

        assert!(dirty_reasons_for_scope(
            &scope,
            &prefixed_dirty,
            &empty_scopes,
            &empty_scopes,
            &prefixed_dirty,
            &ScopeRoots::default(),
        )
        .is_empty());
    }

    fn scopes(ids: &[&str]) -> ScopeSet {
        ids.iter().map(|id| ScopeId::new(*id)).collect()
    }
}
