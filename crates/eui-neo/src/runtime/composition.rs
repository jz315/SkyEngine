use super::debug::{
    finish_composition_debug, neo_debug_trace_enabled, neo_diagnostics_enabled,
    CompositionDebugInput,
};
use super::frame::{prepare_compose_ui, ComposeFrameState};
use super::interaction::replay_frame_responses;
use super::invalidation::NormalizedDirtyInput;
use super::layers::{
    apply_layer_frame_commit, prepare_layer_frame_commit, track_anchored_layer_roots,
};
use super::layout::{
    execute_runtime_layout, prepare_runtime_layout_input, RuntimeLayoutInput, RuntimeLayoutPlan,
    RuntimeLayoutResult,
};
use super::reconcile::{apply_removed_retained_scopes, refresh_scope_roots_from_tree};
use super::tree::{
    cleanup_stale_committed_state, collect_next_structure, commit_composed_tree, ComposedTreeCommit,
};
use super::*;
use crate::dsl::UiParts;

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
        let input = CompositionInput::new(width, height);

        let mut frame_state =
            ComposeFrameState::begin(self, input.screen, dirty, input.diagnostics_enabled);
        let mut ui = prepare_compose_ui(self, input.profile_timing, input.diagnostics_enabled);
        frame_state.attach_previous_frame(self, &mut ui);
        replay_frame_responses(self, &mut ui);
        compose_user_ui(&mut ui, input.screen, compose);

        let mut built_frame = BuiltUiFrame::from_parts(ui.into_parts());
        let layout_input = built_frame.layout_input(&frame_state, &input);
        let layout_result =
            built_frame.execute_layout(&layout_input, self.resources.text_system.as_mut());
        built_frame.track_anchored_layers(input.screen);
        built_frame.apply_layout_result(&layout_result);

        built_frame.refresh_scope_roots();
        let next_structure = collect_next_structure(self, input.screen, &built_frame.roots);
        let retained_context = built_frame.take_retained_context();
        cleanup_removed_scope_dependencies(&retained_context.previous_scope_roots, &built_frame);
        let layer_commit = prepare_layer_frame_commit(
            &mut self.layers.intents,
            built_frame.take_layer_intents(),
            &built_frame.previous_roots,
        );
        let callbacks = built_frame.take_callbacks();
        commit_composed_tree(
            self,
            built_frame.into_tree_commit(input.screen, next_structure),
        );
        self.commit_frame_callbacks(callbacks);
        apply_layer_frame_commit(self, layer_commit);
        cleanup_stale_committed_state(self);
        finish_composition_debug(
            self,
            CompositionDebugInput {
                diagnostics_enabled: input.diagnostics_enabled,
                debug_trace: input.debug_trace,
                previous_scope_roots: retained_context.previous_scope_roots,
                input_dirty_scopes: &frame_state.scope_frame.input_dirty_scopes,
                live_dirty_scopes: &frame_state.scope_frame.live_dirty_scopes,
                previous_clock_ids: &frame_state.previous_clock_ids,
                dirty_scopes: &frame_state.scope_frame.dirty_scopes,
                normalized_dirty_ids: &layout_input.normalized_dirty_ids,
                retained_events: retained_context.events,
                scope_compose_records: retained_context.scope_compose_records,
                layout_mode: layout_result.mode,
            },
        );
    }
}

struct CompositionInput {
    screen: Screen,
    profile_timing: bool,
    diagnostics_enabled: bool,
    debug_trace: bool,
}

impl CompositionInput {
    fn new(width: f32, height: f32) -> Self {
        let debug_trace = neo_debug_trace_enabled();
        Self {
            screen: Screen { width, height },
            profile_timing: cfg!(feature = "profile"),
            diagnostics_enabled: neo_diagnostics_enabled() || debug_trace,
            debug_trace,
        }
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
    scope_layer_roots: ScopeLayerRoots,
    scope_layer_intents: ScopeLayerIntents,
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
            scope_layer_roots: parts.scope_layer_roots,
            scope_layer_intents: parts.scope_layer_intents,
            previous_scope_roots: parts.previous_scope_roots,
            previous_roots: parts.previous_roots,
        }
    }

    fn layout_input(
        &self,
        frame: &ComposeFrameState,
        input: &CompositionInput,
    ) -> RuntimeLayoutInput {
        prepare_runtime_layout_input(
            input.screen,
            frame.can_reuse_scopes(),
            frame.dirty_scopes(),
            &frame.layout_dirty_scopes,
            &self.previous_scope_roots,
            &self.scope_roots,
        )
    }

    fn execute_layout(
        &mut self,
        input: &RuntimeLayoutInput,
        text_system: &mut dyn TextSystem,
    ) -> RuntimeLayoutResult {
        #[cfg(feature = "profile")]
        ::profiling::scope!("eui_neo.runtime.layout_execute");
        execute_runtime_layout(
            &mut self.roots,
            RuntimeLayoutPlan {
                blocker: input.blocker.clone(),
                can_reuse_scopes: input.can_reuse_scopes,
                normalized_dirty_ids: &input.normalized_dirty_ids,
                previous_scope_roots: &self.previous_scope_roots,
                previous_roots: &self.previous_roots,
                screen: input.screen,
            },
            text_system,
        )
    }

    fn apply_layout_result(&mut self, result: &RuntimeLayoutResult) {
        self.retained_stats.partial_layout = result.used_partial_layout;
        self.retained_stats.full_layout = !result.used_partial_layout;
    }

    fn track_anchored_layers(&mut self, screen: Screen) {
        track_anchored_layer_roots(&mut self.roots, &self.layer_intents, screen);
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

    fn take_layer_intents(&mut self) -> Vec<LayerIntent> {
        std::mem::take(&mut self.layer_intents)
    }

    fn take_callbacks(&mut self) -> UiCallbacks {
        std::mem::take(&mut self.callbacks)
    }

    fn into_tree_commit(
        self,
        screen: Screen,
        structure: Vec<ElementSnapshot>,
    ) -> ComposedTreeCommit {
        ComposedTreeCommit {
            screen,
            roots: self.roots,
            scope_roots: self.scope_roots,
            live_scopes: self.live_scopes,
            clock_ids: self.clock_ids,
            clock_periods: self.clock_periods,
            scope_layer_roots: self.scope_layer_roots,
            scope_layer_intents: self.scope_layer_intents,
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

fn cleanup_removed_scope_dependencies(previous_scope_roots: &ScopeRoots, frame: &BuiltUiFrame) {
    apply_removed_retained_scopes(previous_scope_roots, &frame.scope_roots);
}
