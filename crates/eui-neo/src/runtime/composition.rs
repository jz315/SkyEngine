use super::debug::{
    build_runtime_debug_snapshot, collect_element_debug_records, collect_scope_debug_records,
    neo_debug_trace_enabled, neo_diagnostics_enabled, neo_structure_trace_enabled,
    trace_debug_snapshot, RuntimeDebugSnapshotInput,
};
use super::timing::sync_clock_period_ticks;
use super::tree::{
    collect_structure, find_element, layout_structures_match, visual_structures_match,
};
use super::*;
use crate::dsl::UiParts;

impl Runtime {
    pub(crate) fn compose_tree_with_dirty(
        &mut self,
        width: f32,
        height: f32,
        dirty: Option<Vec<DirtyInput>>,
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
        commit_composition(self, built_frame.into_commit(input.screen, next_structure));
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

    pub fn current_frame(&self) -> Frame {
        Frame {
            screen: self.tree.screen,
            draw_list: self.draw_list(),
            needs_render: self.render.needs_render,
            needs_compose: self.render.needs_compose,
            full_redraw: self.render.full_redraw,
            focused_ime_rect: self.focused_ime_rect(),
        }
    }

    pub fn frame<R>(
        &mut self,
        input: FrameInput,
        compose: impl FnOnce(&mut Ui, Screen) -> R,
    ) -> FrameResult<R> {
        self.run_frame(input, None::<fn() -> Vec<DirtyInput>>, compose)
    }

    pub fn frame_incremental<R>(
        &mut self,
        input: FrameInput,
        dirty: impl FnOnce() -> Vec<DirtyInput>,
        compose: impl FnOnce(&mut Ui, Screen) -> R,
    ) -> FrameResult<R> {
        self.run_frame(input, Some(dirty), compose)
    }

    fn run_frame<R>(
        &mut self,
        input: FrameInput,
        dirty_after_input: Option<impl FnOnce() -> Vec<DirtyInput>>,
        compose: impl FnOnce(&mut Ui, Screen) -> R,
    ) -> FrameResult<R> {
        let mut frame_pass = FramePass::from_input(input);
        self.run_input_pass(&mut frame_pass);
        frame_pass.collect_dirty(dirty_after_input);

        let mut value = None;
        if frame_pass.should_full_compose(self) {
            self.run_compose_pass(&frame_pass, None, |ui, screen| {
                value = Some(compose(ui, screen));
            });
        } else {
            let dirty = frame_pass.dirty.take();
            self.run_compose_pass(&frame_pass, dirty, |ui, screen| {
                value = Some(compose(ui, screen));
            });
        }
        self.run_animation_pass(&frame_pass);
        FrameResult {
            value: value.expect("frame compose closure did not run"),
            frame: self.run_output_pass(),
        }
    }

    fn run_input_pass(&mut self, pass: &mut FramePass) {
        let keyboard = std::mem::take(&mut pass.keyboard);
        if let Some((last, leading)) = pass.pointer_events.split_last() {
            for event in leading {
                self.update_pointer(*event);
            }
            self.update_events_and_timers(*last, pass.scroll, keyboard, pass.delta_seconds);
        } else {
            self.update_events_and_timers(pass.pointer, pass.scroll, keyboard, pass.delta_seconds);
        }
    }

    fn run_compose_pass(
        &mut self,
        pass: &FramePass,
        dirty: Option<Vec<DirtyInput>>,
        compose: impl FnOnce(&mut Ui, Screen),
    ) {
        self.compose_tree_with_dirty(pass.screen.width, pass.screen.height, dirty, compose);
    }

    fn run_animation_pass(&mut self, pass: &FramePass) {
        self.tick_animations(pass.delta_seconds);
    }

    fn run_output_pass(&self) -> Frame {
        self.current_frame()
    }
}

struct FramePass {
    screen: Screen,
    delta_seconds: f32,
    pointer: PointerEvent,
    pointer_events: Vec<PointerEvent>,
    scroll: ScrollEvent,
    keyboard: KeyboardEvent,
    dirty: Option<Vec<DirtyInput>>,
    force_full_compose: bool,
}

impl FramePass {
    fn from_input(input: FrameInput) -> Self {
        let FrameInput {
            screen,
            delta_seconds,
            pointer,
            pointer_events,
            scroll,
            keyboard,
            dirty,
            force_full_compose,
        } = input;
        Self {
            screen,
            delta_seconds: delta_seconds.max(0.0),
            pointer,
            pointer_events,
            scroll,
            keyboard,
            dirty,
            force_full_compose,
        }
    }

    fn collect_dirty(&mut self, dirty_after_input: Option<impl FnOnce() -> Vec<DirtyInput>>) {
        self.dirty = dirty_after_input
            .map(|collect| collect())
            .or(self.dirty.take());
    }

    fn should_full_compose(&self, runtime: &Runtime) -> bool {
        self.force_full_compose
            || (runtime.needs_compose() && self.dirty.as_ref().is_none_or(Vec::is_empty))
            || self.dirty.is_none()
    }
}

struct CompositionInput {
    screen: Screen,
    dirty: Option<Vec<DirtyInput>>,
    profile_timing: bool,
    diagnostics_enabled: bool,
    debug_trace: bool,
}

impl CompositionInput {
    fn new(width: f32, height: f32, dirty: Option<Vec<DirtyInput>>) -> Self {
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
}

impl CompositionFrame {
    fn begin(runtime: &mut Runtime, input: &mut CompositionInput) -> Self {
        #[cfg(feature = "profile")]
        ::profiling::scope!("eui_neo.runtime.begin_frame");
        runtime.render.needs_compose = false;
        let dirty_records = input.dirty.take();
        let dirty_ids = dirty_records.as_ref().map(|records| {
            records
                .iter()
                .map(|record| record.id.clone())
                .collect::<FxHashSet<_>>()
        });
        let can_reuse_scopes = dirty_ids.is_some() && runtime.tree.screen == input.screen;
        if let Some(records) = dirty_records.as_ref() {
            for record in records {
                runtime.record_invalidation(Invalidation::signal(
                    record.id.clone(),
                    record
                        .source
                        .clone()
                        .unwrap_or_else(|| "dirty_input".to_string()),
                    record.flags,
                ));
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
    ui.set_focused_id(runtime.input.owners.keyboard_focus.clone());
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
        LayoutInput {
            screen: input.screen,
            can_reuse_scopes: frame.can_reuse_scopes(),
            normalized_dirty_ids: self.normalize_dirty_ids(frame.dirty_scopes()),
        }
    }

    fn layout_blocker(&self, input: &LayoutInput) -> Option<FullLayoutReason> {
        #[cfg(feature = "profile")]
        ::profiling::scope!("eui_neo.runtime.layout_plan");
        partial_layout_blocker_for_scope_reuse(
            input.can_reuse_scopes,
            &input.normalized_dirty_ids,
            &self.previous_scope_roots,
            &self.scope_roots,
        )
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
}

struct CompositionCommit {
    screen: Screen,
    roots: Vec<Element>,
    scope_roots: ScopeRoots,
    live_scopes: ScopeSet,
    clock_ids: ScopeSet,
    clock_periods: Option<ClockPeriodMap>,
    callbacks: UiCallbacks,
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
    runtime.tree.retained_stats = commit.retained_stats;
    runtime.tree.frame_index = runtime.tree.frame_index.saturating_add(1);
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

fn partial_layout_blocker_for_scope_reuse(
    can_reuse_scopes: bool,
    normalized_dirty_ids: &ScopeSet,
    previous_scope_roots: &ScopeRoots,
    scope_roots: &ScopeRoots,
) -> Option<FullLayoutReason> {
    partial_layout_blocker(can_reuse_scopes, normalized_dirty_ids, previous_scope_roots).or_else(
        || {
            let scopes = structurally_incompatible_dirty_scopes(
                normalized_dirty_ids,
                previous_scope_roots,
                scope_roots,
            );
            if !scopes.is_empty() && neo_structure_trace_enabled() {
                for report in structural_incompatibility_reports(
                    normalized_dirty_ids,
                    previous_scope_roots,
                    scope_roots,
                ) {
                    eprintln!("[eui-neo structure] {report}");
                }
            }
            (!scopes.is_empty()).then_some(FullLayoutReason::StructureChanged { ids: scopes })
        },
    )
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
    dirty_ids: &FxHashSet<String>,
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

pub(super) fn partial_layout_blocker(
    can_reuse_scopes: bool,
    layout_dirty_scopes: &ScopeSet,
    previous_scope_roots: &ScopeRoots,
) -> Option<FullLayoutReason> {
    if !can_reuse_scopes {
        return Some(FullLayoutReason::RetainedReuseUnavailable);
    }
    layout_dirty_scopes.iter().find_map(|scope| {
        (!previous_scope_roots.contains_key(scope))
            .then(|| FullLayoutReason::MissingPreviousRetainedRoot { id: scope.clone() })
    })
}

pub(super) fn refresh_scope_roots_from_tree(scope_roots: &mut ScopeRoots, roots: &[Element]) {
    let mut elements_by_id = FxHashMap::default();
    collect_elements_by_id(roots, &mut elements_by_id);
    for elements in scope_roots.values_mut() {
        for element in elements {
            if let Some(updated) = elements_by_id.get(element.id.as_str()) {
                refresh_retained_root_layout_frames(element, updated);
            }
        }
    }
}

pub(super) fn collect_elements_by_id<'a>(
    elements: &'a [Element],
    index: &mut FxHashMap<&'a str, &'a Element>,
) {
    for element in elements {
        index.entry(element.id.as_str()).or_insert(element);
        collect_elements_by_id(&element.children, index);
    }
}

pub(super) fn refresh_retained_root_layout_frames(element: &mut RetainedRoot, updated: &Element) {
    // Layout mutates only Element::frame. Retained scope roots keep the same
    // visual/callback data unless their structure changed, so refresh frames
    // in place and fall back to replacement only when the tree no longer matches.
    if element.kind != updated.kind
        || element.id != updated.id
        || element.children.len() != updated.children.len()
    {
        *element = RetainedRoot::from_element(updated);
        return;
    }
    element.frame = updated.frame;
    for (child, updated_child) in element.children.iter_mut().zip(&updated.children) {
        refresh_retained_root_layout_frames(child, updated_child);
    }
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
