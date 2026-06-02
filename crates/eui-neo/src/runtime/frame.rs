use super::interaction::FrameInputPassReport;
use super::invalidation::NormalizedDirtyInput;
use super::reconcile::{begin_scope_frame, ScopeFrame};
use super::*;

impl Runtime {
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

    pub fn frame_state<T, R>(
        &mut self,
        input: FrameInput,
        state: &crate::State<T>,
        compose: impl FnOnce(&mut Ui, Screen) -> R,
    ) -> FrameResult<R> {
        self.run_frame(input, Some(|| state.take_dirty()), compose)
    }

    pub(crate) fn frame_incremental<R>(
        &mut self,
        input: FrameInput,
        dirty: impl FnOnce() -> Vec<DirtyInput>,
        compose: impl FnOnce(&mut Ui, Screen) -> R,
    ) -> FrameResult<R> {
        self.run_frame(input, Some(dirty), compose)
    }

    /// Dispatch host input through the frame event-command path without composing UI.
    ///
    /// This is intended for benchmarks, tests, and host code that needs to inject
    /// queued input before a later compose pass. Retained dirty records and
    /// diagnostic full-compose requests on [`FrameInput`] are consumed by
    /// [`Runtime::frame`] and [`Runtime::frame_state`].
    pub fn dispatch_frame_input(&mut self, input: FrameInput) -> bool {
        let mut frame_pass = FramePass::from_input(input);
        self.run_input_pass(&mut frame_pass);
        frame_pass.input_report.changed()
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
        let should_full_compose = frame_pass.should_full_compose(self);
        if should_full_compose {
            frame_pass.record_full_compose_invalidations(self);
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
        self.run_platform_effects_pass();
        FrameResult {
            value: value.expect("frame compose closure did not run"),
            frame: self.run_output_pass(),
        }
    }

    fn run_input_pass(&mut self, pass: &mut FramePass) {
        let keyboard = std::mem::take(&mut pass.keyboard);
        pass.input_report = if pass.pointer_events.is_empty() {
            self.update_events_and_timers(pass.pointer, pass.scroll, keyboard, pass.delta_seconds)
        } else {
            self.update_events_and_timers_from_pointer_events(
                &pass.pointer_events,
                pass.scroll,
                keyboard,
                pass.delta_seconds,
            )
        };
    }

    fn run_compose_pass(
        &mut self,
        pass: &FramePass,
        dirty: Option<NormalizedDirtyInput>,
        compose: impl FnOnce(&mut Ui, Screen),
    ) {
        self.compose_tree_with_normalized_dirty(
            pass.screen.width,
            pass.screen.height,
            dirty,
            compose,
        );
    }

    fn run_animation_pass(&mut self, pass: &FramePass) {
        self.tick_animations(pass.delta_seconds);
    }

    fn run_output_pass(&self) -> Frame {
        self.current_frame()
    }
}

pub(super) struct ComposeFrameState {
    pub(super) previous_clock_ids: ScopeSet,
    pub(super) scope_frame: ScopeFrame,
    pub(super) layout_dirty_scopes: ScopeSet,
    previous_clock_periods_for_reuse: Option<ClockPeriodMap>,
}

pub(super) fn prepare_compose_ui(
    runtime: &Runtime,
    profile_timing: bool,
    diagnostics_enabled: bool,
) -> Ui {
    #[cfg(feature = "profile")]
    ::profiling::scope!("eui_neo.runtime.ui_setup");
    let mut ui = Ui::new(runtime.tree.page_id.as_str());
    ui.set_skins(runtime.resources.skins.clone());
    ui.set_focused_id(runtime.input.owners.keyboard_focus_node_id());
    ui.set_clock(runtime.timing.clock_seconds, runtime.tree.frame_index);
    ui.set_profile_timing(profile_timing);
    ui.set_diagnostics_enabled(diagnostics_enabled);
    ui
}

impl ComposeFrameState {
    pub(super) fn begin(
        runtime: &mut Runtime,
        screen: Screen,
        dirty: Option<NormalizedDirtyInput>,
        diagnostics_enabled: bool,
    ) -> Self {
        #[cfg(feature = "profile")]
        ::profiling::scope!("eui_neo.runtime.begin_frame");
        runtime.render.needs_compose = false;
        let dirty_ids = dirty.as_ref().map(|dirty| dirty.compose_scopes.clone());
        let layout_dirty_scopes = dirty
            .as_ref()
            .map(|dirty| dirty.layout_scopes.clone())
            .unwrap_or_default();
        let can_reuse_scopes = dirty_ids.is_some() && runtime.tree.screen == screen;
        if let Some(dirty) = dirty {
            for invalidation in dirty.invalidations {
                runtime.record_committed_invalidation_trace(invalidation);
            }
        }
        if can_reuse_scopes {
            runtime.mark_due_clock_periods();
        }
        let previous_clock_periods_for_reuse = can_reuse_scopes
            .then(|| runtime.tree.clock_periods.clone())
            .flatten();
        let previous_clock_ids = if diagnostics_enabled {
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
            scope_frame,
            layout_dirty_scopes,
            previous_clock_periods_for_reuse,
        }
    }

    pub(super) fn can_reuse_scopes(&self) -> bool {
        self.scope_frame.can_reuse_scopes
    }

    pub(super) fn dirty_scopes(&self) -> &ScopeSet {
        &self.scope_frame.dirty_scopes
    }

    pub(super) fn attach_previous_frame(&mut self, runtime: &mut Runtime, ui: &mut Ui) {
        #[cfg(feature = "profile")]
        ::profiling::scope!("eui_neo.runtime.attach_previous_frame");
        ui.set_previous_roots(std::mem::take(&mut runtime.tree.roots));
        if self.can_reuse_scopes() {
            ui.set_scope_reuse(
                std::mem::take(&mut self.scope_frame.previous_scope_roots),
                std::mem::take(&mut runtime.tree.scope_layer_roots),
                std::mem::take(&mut runtime.tree.scope_layer_intents),
                self.scope_frame.dirty_scopes.clone(),
                std::mem::take(&mut runtime.input.callbacks),
                self.previous_clock_periods_for_reuse.take(),
            );
        }
    }
}

struct FramePass {
    screen: Screen,
    delta_seconds: f32,
    pointer: PointerEvent,
    pointer_events: Vec<PointerEvent>,
    scroll: ScrollEvent,
    keyboard: KeyboardEvent,
    dirty: Option<NormalizedDirtyInput>,
    force_full_compose: bool,
    input_report: FrameInputPassReport,
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
        let dirty = NormalizedDirtyInput::from_optional_dirty_inputs(dirty);
        Self {
            screen,
            delta_seconds: delta_seconds.max(0.0),
            pointer,
            pointer_events,
            scroll,
            keyboard,
            dirty,
            force_full_compose,
            input_report: FrameInputPassReport::default(),
        }
    }

    fn collect_dirty(&mut self, dirty_after_input: Option<impl FnOnce() -> Vec<DirtyInput>>) {
        if let Some(collect) = dirty_after_input {
            let collected = NormalizedDirtyInput::from_dirty_inputs(collect());
            if let Some(dirty) = self.dirty.as_mut() {
                dirty.merge(collected);
            } else {
                self.dirty = Some(collected);
            }
        }
    }

    fn should_full_compose(&self, runtime: &Runtime) -> bool {
        let dirty_pass_flags = self
            .dirty
            .as_ref()
            .map_or_else(PassFlags::default, |dirty| dirty.pass_flags);
        self.force_full_compose
            || (runtime.needs_compose() && !dirty_pass_flags.request_compose_ui)
            || self.dirty.is_none()
    }

    fn record_full_compose_invalidations(&mut self, runtime: &mut Runtime) {
        if self.force_full_compose {
            runtime.request_invalidation(Invalidation::runtime(
                runtime.runtime_invalidation_target(),
                "force_full_compose",
                DirtyFlags::COMPOSE | DirtyFlags::DRAW,
            ));
        }
        let Some(dirty) = self.dirty.take() else {
            return;
        };
        for invalidation in dirty.invalidations {
            runtime.record_committed_invalidation_trace(invalidation);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;
    use std::time::Duration;

    use crate::callbacks::ClickCallbackId;
    use crate::retained::{RetainedRoot, ScopeId};

    use super::*;

    #[test]
    fn compose_frame_state_records_dirty_and_prepares_reuse_context() {
        let mut runtime = Runtime::new("page");
        let screen = Screen::new(320.0, 200.0);
        runtime.tree.screen = screen;
        runtime.render.needs_compose = true;
        runtime.tree.scope_roots.insert(
            ScopeId::new("page.scope"),
            RetainedRoot::from_elements(&[Element::new(ElementKind::Rect, "page.scope.root")]),
        );
        runtime.tree.live_ids.insert(ScopeId::new("page.live"));
        runtime.tree.clock_ids.insert(ScopeId::new("page.clock"));
        let mut clock_periods = ClockPeriodMap::default();
        clock_periods.insert(ScopeId::new("page.clock"), Duration::from_secs(1));
        runtime.tree.clock_periods = Some(clock_periods);
        let dirty = NormalizedDirtyInput::from_dirty_inputs(vec![DirtyInput::new(
            "page.scope",
            DirtyFlags::COMPOSE | DirtyFlags::LAYOUT,
        )]);

        let state = ComposeFrameState::begin(&mut runtime, screen, Some(dirty), true);

        assert!(!runtime.needs_compose());
        assert_eq!(runtime.invalidation.snapshot().len(), 1);
        assert!(state.can_reuse_scopes());
        assert!(state.scope_frame.input_dirty_scopes.contains("page.scope"));
        assert!(state.scope_frame.live_dirty_scopes.contains("page.live"));
        assert!(state.dirty_scopes().contains("page.scope"));
        assert!(state.dirty_scopes().contains("page.live"));
        assert!(state.layout_dirty_scopes.contains("page.scope"));
        assert!(state
            .scope_frame
            .previous_scope_roots
            .contains_key("page.scope"));
        assert!(runtime.tree.scope_roots.is_empty());
        assert!(state.previous_clock_ids.contains("page.clock"));
        assert!(state.previous_clock_periods_for_reuse.is_some());
    }

    #[test]
    fn compose_frame_state_disables_reuse_without_dirty_input() {
        let mut runtime = Runtime::new("page");
        runtime.tree.screen = Screen::new(320.0, 200.0);
        runtime.tree.scope_roots.insert(
            ScopeId::new("page.previous"),
            RetainedRoot::from_elements(&[Element::new(ElementKind::Rect, "page.previous.root")]),
        );
        runtime.tree.live_ids.insert(ScopeId::new("page.live"));

        let state = ComposeFrameState::begin(&mut runtime, Screen::new(320.0, 200.0), None, false);

        assert!(!state.can_reuse_scopes());
        assert!(state.scope_frame.input_dirty_scopes.is_empty());
        assert!(state.scope_frame.live_dirty_scopes.is_empty());
        assert!(state.dirty_scopes().is_empty());
        assert!(state.scope_frame.previous_scope_roots.is_empty());
        assert!(runtime.tree.scope_roots.contains_key("page.previous"));
        assert!(runtime.tree.live_ids.is_empty());
        assert!(state.previous_clock_ids.is_empty());
        assert!(state.previous_clock_periods_for_reuse.is_none());
    }

    #[test]
    fn frame_pass_merges_input_and_post_input_dirty_records() {
        let mut pass =
            FramePass::from_input(FrameInput::new(Screen::new(320.0, 200.0), 0.0).dirty([
                DirtyInput::new("page.compose", DirtyFlags::COMPOSE | DirtyFlags::DRAW),
            ]));

        pass.collect_dirty(Some(|| {
            vec![DirtyInput::new("page.layout", DirtyFlags::LAYOUT)]
        }));

        let dirty = pass.dirty.as_ref().expect("dirty input should be retained");
        assert_eq!(dirty.invalidations.len(), 2);
        assert!(dirty.compose_scopes.contains("page.compose"));
        assert!(dirty.layout_scopes.contains("page.layout"));
        assert!(dirty.pass_flags.request_compose_ui);
        assert!(dirty.pass_flags.request_reconcile);
        assert!(dirty.pass_flags.request_layout);
        assert!(dirty.pass_flags.request_hit);
        assert!(dirty.pass_flags.request_draw);
    }

    #[test]
    fn frame_input_pass_records_event_command_report() {
        let mut runtime = Runtime::new("page");
        let clicks = Rc::new(Cell::new(0));
        let click_callback = clicks.clone();
        runtime.input.callbacks.on_click.insert(
            ClickCallbackId::new(NodeId::new("page.button")),
            Box::new(move || click_callback.set(click_callback.get() + 1)),
        );
        let mut button = Element::new(ElementKind::Rect, "page.button");
        button.interactive = true;
        button.frame = LayoutRect::new(0.0, 0.0, 40.0, 30.0);
        runtime.tree.roots = vec![button];
        let mut pass = FramePass::from_input(
            FrameInput::new(Screen::new(100.0, 100.0), 0.0).pointer_events([
                PointerEvent::pressed_at(10.0, 10.0),
                PointerEvent::released_at(10.0, 10.0),
            ]),
        );

        runtime.run_input_pass(&mut pass);

        assert_eq!(clicks.get(), 1);
        assert!(pass.input_report.input_state_changed);
        assert!(pass.input_report.command_report.changed());
        assert_eq!(pass.input_report.command_report.command_count, 2);
        assert_eq!(pass.input_report.command_report.callback_count, 1);
        assert_eq!(pass.input_report.command_report.invalidation_count, 1);
        assert!(
            pass.input_report
                .command_report
                .pass_flags
                .request_compose_ui
        );
        let snapshot = runtime.diagnostics().current_snapshot();
        let input_pass = snapshot
            .input_pass
            .as_ref()
            .expect("input pass report should be visible in current debug snapshot");
        assert!(input_pass.input_state_changed);
        assert_eq!(input_pass.command_count, 2);
        assert_eq!(input_pass.callback_count, 1);
    }

    #[test]
    fn frame_input_pass_reports_timer_render_request() {
        let mut runtime = Runtime::new("page");
        runtime.mark_rendered();
        let mut timer = Element::new(ElementKind::Rect, "page.timer");
        timer.timer_seconds = 1.0;
        runtime.tree.roots = vec![timer];
        let mut pass = FramePass::from_input(FrameInput::new(Screen::new(100.0, 100.0), 0.25));

        runtime.run_input_pass(&mut pass);

        assert!(pass.input_report.timer_render_requested);
        assert_eq!(pass.input_report.command_report.command_count, 0);
        assert!(!pass.input_report.input_state_changed);
        assert!(!pass.input_report.command_report.changed());
        assert!(runtime.needs_render());
        let snapshot = runtime.diagnostics().current_snapshot();
        let input_pass = snapshot
            .input_pass
            .as_ref()
            .expect("timer render request should be visible in current debug snapshot");
        assert!(input_pass.timer_render_requested);
        assert_eq!(input_pass.command_count, 0);
    }

    #[test]
    fn force_full_compose_records_runtime_invalidation() {
        let mut runtime = Runtime::new("page");
        runtime.frame(
            FrameInput::new(Screen::new(320.0, 200.0), 0.0).force_full_compose(true),
            |ui, _| {
                ui.rect("root").build();
            },
        );

        let snapshot = runtime.diagnostics().committed_snapshot();
        let invalidation = snapshot
            .invalidations
            .iter()
            .find(|invalidation| {
                invalidation.target.id() == "page"
                    && invalidation.source == InvalidationSource::Runtime("force_full_compose")
            })
            .expect("force full compose should be traceable as a runtime invalidation");
        assert_eq!(invalidation.flags, DirtyFlags::COMPOSE | DirtyFlags::DRAW);
        assert!(invalidation.pass_flags.request_compose_ui);
        assert!(invalidation.pass_flags.request_reconcile);
        assert!(invalidation.pass_flags.request_draw);
    }

    #[test]
    fn prepare_compose_ui_carries_runtime_focus_and_clock() {
        let mut runtime = Runtime::new("page");
        runtime
            .input
            .owners
            .set_keyboard_focus(Some(NodeId::new("page.hit")), false);
        runtime.timing.clock_seconds = 1.5;
        runtime.tree.frame_index = 7;

        let mut ui = prepare_compose_ui(&runtime, true, true);

        assert!(ui.is_focused("hit"));
        assert_eq!(ui.clock().seconds(), 1.5);
        assert_eq!(ui.clock().frame_index(), 7);
    }
}
