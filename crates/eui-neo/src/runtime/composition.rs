use super::debug::{
    build_runtime_debug_snapshot, collect_element_debug_records, collect_scope_debug_records,
    neo_debug_trace_enabled, neo_diagnostics_enabled, neo_structure_trace_enabled,
    trace_debug_snapshot, RuntimeDebugSnapshotInput,
};
use super::invalidation::NormalizedDirtyInput;
use super::layers::{collect_layer_debug_records, layer_blocks_element_target};
use super::reconcile::{
    apply_removed_retained_scopes, refresh_scope_roots_from_tree, retained_layout_reuse_plan,
};
use super::timing::sync_clock_period_ticks;
use super::tree::{
    collect_structure, find_element, layout_structures_match, visual_structures_match,
};
use super::*;
use crate::dsl::UiParts;

type ElementIdSet = FxHashSet<String>;

impl Runtime {
    #[cfg(test)]
    pub(crate) fn compose_tree_with_dirty(
        &mut self,
        width: f32,
        height: f32,
        dirty: Option<Vec<DirtyInput>>,
        compose: impl FnOnce(&mut Ui, Screen),
    ) {
        self.compose_tree_with_normalized_dirty(
            width,
            height,
            NormalizedDirtyInput::from_optional_dirty_inputs(dirty),
            compose,
        );
    }

    pub(super) fn compose_tree_with_normalized_dirty(
        &mut self,
        width: f32,
        height: f32,
        dirty: Option<NormalizedDirtyInput>,
        compose: impl FnOnce(&mut Ui, Screen),
    ) {
        #[cfg(feature = "profile")]
        ::profiling::scope!("eui_neo.runtime.frame");
        let mut input = CompositionInput::new(width, height, dirty);

        let mut frame_state = CompositionFrame::begin(self, &mut input);
        let mut ui = new_frame_ui(self, &input);
        frame_state.attach_previous_frame(self, &mut ui);
        replay_responses(self, &mut ui);
        compose_user_ui(&mut ui, input.screen, compose);

        let mut built_frame = BuiltUiFrame::from_parts(ui.into_parts());
        let layout_input = built_frame.layout_input(&frame_state, &input);
        let partial_layout_blocker = built_frame.layout_blocker(&layout_input);
        let layout_result = built_frame.execute_layout(
            &layout_input,
            partial_layout_blocker,
            self.resources.text_system.as_mut(),
        );
        built_frame.apply_layout_result(&layout_result);

        built_frame.refresh_scope_roots();
        let next_structure = collect_next_structure(self, input.screen, &built_frame.roots);
        let retained_context = built_frame.take_retained_context();
        cleanup_removed_scope_dependencies(&retained_context.previous_scope_roots, &built_frame);
        let previous_layers = std::mem::take(&mut self.layers.intents);
        let layer_debug_records = collect_layer_debug_records(
            &previous_layers,
            &built_frame.layer_intents,
            &built_frame.previous_roots,
        );
        commit_composition(self, built_frame.into_commit(input.screen, next_structure));
        self.layers.debug_records = layer_debug_records;
        cleanup_stale_retained_state(self);
        finish_composition_diagnostics(
            self,
            DiagnosticsInput {
                diagnostics_enabled: input.diagnostics_enabled,
                debug_trace: input.debug_trace,
                frame_state: &frame_state,
                normalized_dirty_ids: &layout_input.normalized_dirty_ids,
                retained_context,
                layout_mode: layout_result.mode,
            },
        );
    }
}

struct CompositionInput {
    screen: Screen,
    dirty: Option<NormalizedDirtyInput>,
    profile_timing: bool,
    diagnostics_enabled: bool,
    debug_trace: bool,
}

impl CompositionInput {
    fn new(width: f32, height: f32, dirty: Option<NormalizedDirtyInput>) -> Self {
        let debug_trace = neo_debug_trace_enabled();
        Self {
            screen: Screen { width, height },
            dirty,
            profile_timing: cfg!(feature = "profile"),
            diagnostics_enabled: neo_diagnostics_enabled() || debug_trace,
            debug_trace,
        }
    }
}

struct CompositionFrame {
    previous_clock_ids: ScopeSet,
    previous_clock_periods_for_reuse: Option<ClockPeriodMap>,
    scope_frame: ScopeFrame,
    layout_dirty_scopes: ScopeSet,
}

impl CompositionFrame {
    fn begin(runtime: &mut Runtime, input: &mut CompositionInput) -> Self {
        #[cfg(feature = "profile")]
        ::profiling::scope!("eui_neo.runtime.begin_frame");
        runtime.render.needs_compose = false;
        let dirty_input = input.dirty.take();
        let dirty_ids = dirty_input
            .as_ref()
            .map(|dirty| dirty.compose_scopes.clone());
        let layout_dirty_scopes = dirty_input
            .as_ref()
            .map(|dirty| dirty.layout_scopes.clone())
            .unwrap_or_default();
        let can_reuse_scopes = dirty_ids.is_some() && runtime.tree.screen == input.screen;
        if let Some(dirty) = dirty_input {
            for invalidation in dirty.invalidations {
                runtime.record_invalidation(invalidation);
            }
        }
        if can_reuse_scopes {
            runtime.mark_due_clock_periods();
        }
        let previous_clock_periods_for_reuse = can_reuse_scopes
            .then(|| runtime.tree.clock_periods.clone())
            .flatten();
        let previous_clock_ids = if input.diagnostics_enabled {
            runtime.tree.clock_ids.clone()
        } else {
            ScopeSet::default()
        };
        let scope_frame = begin_scope_frame(
            can_reuse_scopes,
            dirty_ids,
            &mut runtime.tree.scope_roots,
            &mut runtime.tree.live_ids,
        );

        Self {
            previous_clock_ids,
            previous_clock_periods_for_reuse,
            scope_frame,
            layout_dirty_scopes,
        }
    }

    fn can_reuse_scopes(&self) -> bool {
        self.scope_frame.can_reuse_scopes
    }

    fn dirty_scopes(&self) -> &ScopeSet {
        &self.scope_frame.dirty_scopes
    }

    fn attach_previous_frame(&mut self, runtime: &mut Runtime, ui: &mut Ui) {
        #[cfg(feature = "profile")]
        ::profiling::scope!("eui_neo.runtime.attach_previous_frame");
        ui.set_previous_roots(std::mem::take(&mut runtime.tree.roots));
        if self.can_reuse_scopes() {
            ui.set_scope_reuse(
                std::mem::take(&mut self.scope_frame.previous_scope_roots),
                self.scope_frame.dirty_scopes.clone(),
                std::mem::take(&mut runtime.input.callbacks),
                self.previous_clock_periods_for_reuse.take(),
            );
        }
    }
}

fn new_frame_ui(runtime: &Runtime, input: &CompositionInput) -> Ui {
    #[cfg(feature = "profile")]
    ::profiling::scope!("eui_neo.runtime.ui_setup");
    let mut ui = Ui::new(runtime.tree.page_id.clone());
    ui.set_skins(runtime.resources.skins.clone());
    ui.set_focused_id(runtime.input.owners.keyboard_focus_string());
    ui.set_clock(runtime.timing.clock_seconds, runtime.tree.frame_index);
    ui.set_profile_timing(input.profile_timing);
    ui.set_diagnostics_enabled(input.diagnostics_enabled);
    ui
}

fn replay_responses(runtime: &Runtime, ui: &mut Ui) {
    #[cfg(feature = "profile")]
    ::profiling::scope!("eui_neo.runtime.replay_responses");
    for (id, response) in &runtime.input.responses {
        ui.set_response(id.clone(), *response);
    }
}

fn compose_user_ui(ui: &mut Ui, screen: Screen, compose: impl FnOnce(&mut Ui, Screen)) {
    #[cfg(feature = "profile")]
    ::profiling::scope!("eui_neo.runtime.build_ui");
    compose(ui, screen);
}

struct BuiltUiFrame {
    roots: Vec<Element>,
    callbacks: UiCallbacks,
    scope_roots: ScopeRoots,
    live_scopes: ScopeSet,
    clock_ids: ScopeSet,
    clock_periods: Option<ClockPeriodMap>,
    retained_stats: RetainedComposeStats,
    retained_events: Vec<RetainedComposeEvent>,
    scope_compose_records: Vec<ScopeComposeRecord>,
    layer_intents: Vec<LayerIntent>,
    previous_scope_roots: ScopeRoots,
    previous_roots: Vec<Element>,
}

impl BuiltUiFrame {
    fn from_parts(parts: UiParts) -> Self {
        #[cfg(feature = "profile")]
        ::profiling::scope!("eui_neo.runtime.into_parts");
        Self {
            roots: parts.roots,
            callbacks: parts.callbacks,
            scope_roots: parts.scope_roots,
            live_scopes: parts.live_scopes,
            clock_ids: parts.clock_ids,
            clock_periods: parts.clock_periods,
            retained_stats: parts.retained_stats,
            retained_events: parts.retained_events,
            scope_compose_records: parts.scope_compose_records,
            layer_intents: parts.layer_intents,
            previous_scope_roots: parts.previous_scope_roots,
            previous_roots: parts.previous_roots,
        }
    }

    fn normalize_dirty_ids(&self, dirty_scopes: &ScopeSet) -> ScopeSet {
        normalize_dirty_scopes_with_roots(dirty_scopes, &self.previous_scope_roots)
    }

    fn layout_input(&self, frame: &CompositionFrame, input: &CompositionInput) -> LayoutInput {
        #[cfg(feature = "profile")]
        ::profiling::scope!("eui_neo.runtime.layout_input");
        let mut layout_dirty_scopes = frame.dirty_scopes().clone();
        layout_dirty_scopes.extend(frame.layout_dirty_scopes.iter().cloned());
        LayoutInput {
            screen: input.screen,
            can_reuse_scopes: frame.can_reuse_scopes(),
            normalized_dirty_ids: self.normalize_dirty_ids(&layout_dirty_scopes),
        }
    }

    fn layout_blocker(&self, input: &LayoutInput) -> Option<FullLayoutReason> {
        #[cfg(feature = "profile")]
        ::profiling::scope!("eui_neo.runtime.layout_plan");
        let plan = retained_layout_reuse_plan(
            input.can_reuse_scopes,
            &input.normalized_dirty_ids,
            &self.previous_scope_roots,
            &self.scope_roots,
            neo_structure_trace_enabled(),
        );
        for report in plan.structural_reports {
            eprintln!("[eui-neo structure] {report}");
        }
        plan.blocker
    }

    fn execute_layout(
        &mut self,
        input: &LayoutInput,
        blocker: Option<FullLayoutReason>,
        text_system: &mut dyn TextSystem,
    ) -> LayoutResult {
        #[cfg(feature = "profile")]
        ::profiling::scope!("eui_neo.runtime.layout_execute");
        execute_layout_plan(
            &mut self.roots,
            LayoutPlan {
                blocker,
                can_reuse_scopes: input.can_reuse_scopes,
                normalized_dirty_ids: &input.normalized_dirty_ids,
                previous_scope_roots: &self.previous_scope_roots,
                previous_roots: &self.previous_roots,
                screen: input.screen,
            },
            text_system,
        )
    }

    fn apply_layout_result(&mut self, result: &LayoutResult) {
        self.retained_stats.partial_layout = result.used_partial_layout;
        self.retained_stats.full_layout = !result.used_partial_layout;
    }

    fn refresh_scope_roots(&mut self) {
        #[cfg(feature = "profile")]
        ::profiling::scope!("eui_neo.runtime.refresh_retained_roots");
        refresh_scope_roots_from_tree(&mut self.scope_roots, &self.roots);
    }

    fn take_retained_context(&mut self) -> RetainedFrameContext {
        RetainedFrameContext {
            previous_scope_roots: std::mem::take(&mut self.previous_scope_roots),
            events: std::mem::take(&mut self.retained_events),
            scope_compose_records: std::mem::take(&mut self.scope_compose_records),
        }
    }

    fn into_commit(self, screen: Screen, structure: Vec<ElementSnapshot>) -> CompositionCommit {
        CompositionCommit {
            screen,
            roots: self.roots,
            scope_roots: self.scope_roots,
            live_scopes: self.live_scopes,
            clock_ids: self.clock_ids,
            clock_periods: self.clock_periods,
            callbacks: self.callbacks,
            layer_intents: self.layer_intents,
            retained_stats: self.retained_stats,
            structure,
        }
    }
}

struct RetainedFrameContext {
    previous_scope_roots: ScopeRoots,
    events: Vec<RetainedComposeEvent>,
    scope_compose_records: Vec<ScopeComposeRecord>,
}

struct DiagnosticsInput<'a> {
    diagnostics_enabled: bool,
    debug_trace: bool,
    frame_state: &'a CompositionFrame,
    normalized_dirty_ids: &'a ScopeSet,
    retained_context: RetainedFrameContext,
    layout_mode: LayoutMode,
}

fn finish_composition_diagnostics(runtime: &mut Runtime, source: DiagnosticsInput<'_>) {
    let RetainedFrameContext {
        previous_scope_roots,
        events,
        scope_compose_records,
    } = source.retained_context;

    let element_debug_records = collect_elements_for_debug(runtime, source.diagnostics_enabled);
    let scope_debug_records = collect_scopes_for_debug(
        runtime,
        ScopeDebugSource {
            diagnostics_enabled: source.diagnostics_enabled,
            previous_scope_roots: &previous_scope_roots,
            input_dirty_scopes: &source.frame_state.scope_frame.input_dirty_scopes,
            live_dirty_scopes: &source.frame_state.scope_frame.live_dirty_scopes,
            previous_clock_ids: &source.frame_state.previous_clock_ids,
            dirty_scopes: &source.frame_state.scope_frame.dirty_scopes,
            normalized_dirty_ids: source.normalized_dirty_ids,
            element_debug_records: &element_debug_records,
            retained_events: &events,
        },
    );
    runtime.debug.snapshot = build_debug_snapshot(
        runtime,
        RuntimeDebugSnapshotInput {
            diagnostics_enabled: source.diagnostics_enabled,
            dirty_scopes: &source.frame_state.scope_frame.dirty_scopes,
            normalized_dirty_ids: source.normalized_dirty_ids,
            scope_debug_records,
            element_debug_records,
            retained_events: events,
            scope_compose_records,
            layout_mode: source.layout_mode,
        },
    );
    if source.debug_trace {
        trace_debug_snapshot(&runtime.debug.snapshot);
    }
    runtime.clear_committed_invalidations();
    runtime.clear_committed_event_debug_records();
}

struct CompositionCommit {
    screen: Screen,
    roots: Vec<Element>,
    scope_roots: ScopeRoots,
    live_scopes: ScopeSet,
    clock_ids: ScopeSet,
    clock_periods: Option<ClockPeriodMap>,
    callbacks: UiCallbacks,
    layer_intents: Vec<LayerIntent>,
    retained_stats: RetainedComposeStats,
    structure: Vec<ElementSnapshot>,
}

fn commit_composition(runtime: &mut Runtime, commit: CompositionCommit) {
    #[cfg(feature = "profile")]
    ::profiling::scope!("eui_neo.runtime.commit");
    runtime.tree.structure = commit.structure;
    runtime.tree.screen = commit.screen;
    runtime.tree.roots = commit.roots;
    runtime.tree.scope_roots = commit.scope_roots;
    runtime.tree.live_ids = commit.live_scopes;
    runtime.tree.clock_ids = commit.clock_ids;
    runtime.tree.clock_periods = commit.clock_periods;
    sync_clock_period_ticks(
        runtime.timing.clock_seconds,
        runtime.tree.clock_periods.as_ref(),
        &mut runtime.tree.clock_period_ticks,
    );
    runtime.input.callbacks = commit.callbacks;
    runtime.layers.intents = commit.layer_intents;
    runtime.tree.retained_stats = commit.retained_stats;
    runtime.tree.frame_index = runtime.tree.frame_index.saturating_add(1);
}

fn cleanup_removed_scope_dependencies(previous_scope_roots: &ScopeRoots, frame: &BuiltUiFrame) {
    apply_removed_retained_scopes(previous_scope_roots, &frame.scope_roots);
}

fn cleanup_stale_retained_state(runtime: &mut Runtime) {
    let existing_ids = collect_existing_element_ids(&runtime.tree.roots);
    let mut changed = cleanup_stale_input_owners(runtime, &existing_ids);
    changed |= cleanup_layer_blocked_focus(runtime, &existing_ids);
    changed |= cleanup_stale_animation_state(runtime, &existing_ids);
    changed |= cleanup_stale_timer_state(runtime, &existing_ids);
    if changed {
        runtime.mark_render_dirty();
    }
}

fn cleanup_stale_input_owners(runtime: &mut Runtime, existing_ids: &ElementIdSet) -> bool {
    let mut changed = runtime
        .input
        .owners
        .retain_existing(|id| existing_ids.contains(id));
    let previous_interactions = runtime.input.interactions.len();
    runtime
        .input
        .interactions
        .retain(|id, _| existing_ids.contains(id));
    changed |= runtime.input.interactions.len() != previous_interactions;
    let previous_responses = runtime.input.responses.len();
    runtime
        .input
        .responses
        .retain(|id, _| existing_ids.contains(id));
    changed |= runtime.input.responses.len() != previous_responses;
    changed
}

fn cleanup_layer_blocked_focus(runtime: &mut Runtime, existing_ids: &ElementIdSet) -> bool {
    let mut changed = false;
    if runtime
        .layers
        .focus_restore
        .as_ref()
        .is_some_and(|id| !existing_ids.contains(id))
    {
        runtime.layers.focus_restore = None;
        changed = true;
    }

    if let Some(focused_id) = runtime.input.owners.keyboard_focus_string() {
        if layer_blocks_element_target(&runtime.layers.intents, &runtime.tree.roots, &focused_id) {
            if runtime.layers.focus_restore.is_none() {
                runtime.layers.focus_restore = Some(focused_id);
            }
            runtime.input.owners.set_keyboard_focus(None, false);
            return true;
        }
        return changed;
    }

    let Some(restore_id) = runtime.layers.focus_restore.clone() else {
        return changed;
    };
    if layer_blocks_element_target(&runtime.layers.intents, &runtime.tree.roots, &restore_id) {
        return changed;
    }
    if !existing_ids.contains(&restore_id) {
        runtime.layers.focus_restore = None;
        return true;
    }

    let text_enabled = runtime.input.callbacks.has_text_input(&restore_id);
    runtime
        .input
        .owners
        .set_keyboard_focus(Some(restore_id), text_enabled);
    runtime.layers.focus_restore = None;
    true
}

fn cleanup_stale_animation_state(runtime: &mut Runtime, existing_ids: &ElementIdSet) -> bool {
    let previous_animations = runtime.animation.animations.len();
    runtime
        .animation
        .animations
        .retain(|id, _| existing_ids.contains(id));
    let previous_frame_targets = runtime.animation.frame_targets.len();
    runtime
        .animation
        .frame_targets
        .retain(|id, _| existing_ids.contains(id));
    runtime.animation.animations.len() != previous_animations
        || runtime.animation.frame_targets.len() != previous_frame_targets
}

fn cleanup_stale_timer_state(runtime: &mut Runtime, existing_ids: &ElementIdSet) -> bool {
    let previous_timers = runtime.timing.timers.len();
    runtime
        .timing
        .timers
        .retain(|id, _| existing_ids.contains(id));
    runtime.timing.timers.len() != previous_timers
}

fn collect_existing_element_ids(elements: &[Element]) -> ElementIdSet {
    let mut ids = ElementIdSet::default();
    collect_existing_element_ids_into(elements, &mut ids);
    ids
}

fn collect_existing_element_ids_into(elements: &[Element], ids: &mut ElementIdSet) {
    for element in elements {
        ids.insert(element.id.clone());
        collect_existing_element_ids_into(&element.children, ids);
    }
}

fn collect_next_structure(
    runtime: &mut Runtime,
    screen: Screen,
    roots: &[Element],
) -> Vec<ElementSnapshot> {
    #[cfg(feature = "profile")]
    ::profiling::scope!("eui_neo.runtime.collect_structure");
    let next_structure = collect_structure(roots, runtime.tree.structure.len());
    let layout_structure_changed = runtime.tree.screen != screen
        || !layout_structures_match(&next_structure, &runtime.tree.structure);
    let visual_structure_changed =
        !visual_structures_match(&next_structure, &runtime.tree.structure);
    if layout_structure_changed {
        runtime.mark_full_redraw_dirty();
    } else if visual_structure_changed {
        runtime.mark_render_dirty();
    }
    next_structure
}

struct LayoutPlan<'a> {
    blocker: Option<FullLayoutReason>,
    can_reuse_scopes: bool,
    normalized_dirty_ids: &'a ScopeSet,
    previous_scope_roots: &'a ScopeRoots,
    previous_roots: &'a [Element],
    screen: Screen,
}

struct LayoutInput {
    screen: Screen,
    can_reuse_scopes: bool,
    normalized_dirty_ids: ScopeSet,
}

struct LayoutResult {
    used_partial_layout: bool,
    mode: LayoutMode,
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
        &runtime.animation.animations,
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

fn build_debug_snapshot(
    runtime: &Runtime,
    input: RuntimeDebugSnapshotInput<'_>,
) -> UiDebugSnapshot {
    #[cfg(feature = "profile")]
    ::profiling::scope!("eui_neo.runtime.snapshot");
    build_runtime_debug_snapshot(runtime, input)
}

fn execute_layout_plan(
    roots: &mut Vec<Element>,
    plan: LayoutPlan<'_>,
    text_system: &mut dyn TextSystem,
) -> LayoutResult {
    let partial_layout = plan.blocker.is_none();
    let mut used_partial_layout = false;
    if partial_layout && plan.can_reuse_scopes {
        copy_previous_frames(roots, plan.previous_roots);
        used_partial_layout = layout_dirty_ids_with_text_system(
            roots,
            plan.normalized_dirty_ids,
            plan.previous_scope_roots,
            text_system,
        );
    }

    let mode = if used_partial_layout {
        LayoutMode::Partial
    } else if partial_layout {
        LayoutMode::Full(FullLayoutReason::DirtyRetainedLayoutFailed)
    } else {
        LayoutMode::Full(
            plan.blocker
                .expect("full layout blocker should be known here"),
        )
    };

    if !used_partial_layout {
        layout_roots_with_text_system(roots, plan.screen.width, plan.screen.height, text_system);
    }

    LayoutResult {
        used_partial_layout,
        mode,
    }
}

pub(super) fn layout_dirty_ids_with_text_system(
    roots: &mut [Element],
    dirty_ids: &ScopeSet,
    previous_scope_roots: &ScopeRoots,
    text_system: &mut dyn TextSystem,
) -> bool {
    for scope in dirty_ids {
        let Some(previous_roots) = previous_scope_roots.get(scope) else {
            return false;
        };
        for previous in previous_roots {
            let Some(current) = find_element_mut(roots, &previous.id) else {
                return false;
            };
            if !layout_element_in_frame_with_text_system(current, previous.frame, text_system) {
                return false;
            }
        }
    }
    true
}

pub(super) fn copy_previous_frames(elements: &mut [Element], previous_roots: &[Element]) {
    for element in elements {
        if let Some(previous) = find_element_in_slice(previous_roots, &element.id) {
            element.frame = previous.frame;
        }
        copy_previous_frames(&mut element.children, previous_roots);
    }
}

pub(super) fn find_element_in_slice<'a>(elements: &'a [Element], id: &str) -> Option<&'a Element> {
    elements
        .iter()
        .find_map(|element| find_element(element, id))
}

pub(super) fn find_element_mut<'a>(
    elements: &'a mut [Element],
    id: &str,
) -> Option<&'a mut Element> {
    for element in elements {
        if element.id == id {
            return Some(element);
        }
        if let Some(found) = find_element_mut(&mut element.children, id) {
            return Some(found);
        }
    }
    None
}
