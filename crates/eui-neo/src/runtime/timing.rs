use super::event_command::UiEventCommand;
#[cfg(test)]
use super::interaction::FrameInputCommandBatch;
use super::reconcile::ElementIdSet;
use super::*;

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct TimerState {
    seconds: f32,
    elapsed: f32,
    seen: bool,
    active: bool,
}

#[derive(Debug, Clone, Default)]
pub(super) struct TimerCommandCollection {
    pub(super) commands: Vec<UiEventCommand>,
    pub(super) render_requested: bool,
}

impl Runtime {
    #[cfg(test)]
    pub(crate) fn tick_timers(&mut self, delta_seconds: f32) -> bool {
        let collection = self.collect_timer_commands(delta_seconds);
        let report = self.execute_frame_input_command_batch(FrameInputCommandBatch {
            commands: collection.commands,
            input_state_changed: false,
            timer_render_requested: collection.render_requested,
        });
        let changed = report.changed();
        self.record_input_pass_debug(&report);
        changed
    }

    pub(super) fn collect_timer_commands(&mut self, delta_seconds: f32) -> TimerCommandCollection {
        for state in self.timing.timers.values_mut() {
            state.seen = false;
        }
        let mut fired = Vec::new();
        collect_timer_ids(&self.tree.roots, &mut fired);
        let mut commands = Vec::new();
        let mut render_requested = false;
        for (id, seconds) in fired {
            let state = self.timing.timers.entry(id.clone()).or_default();
            state.seen = true;
            if !state.active || (state.seconds - seconds).abs() > 0.001 {
                state.seconds = seconds;
                state.elapsed = 0.0;
                state.active = true;
            }
            state.elapsed += delta_seconds.max(0.0);
            if state.active && state.elapsed >= state.seconds {
                state.active = false;
                commands.push(UiEventCommand::Timer { target: id });
            } else if state.active {
                render_requested = true;
            }
        }
        self.timing.timers.retain(|_, state| state.seen);
        if render_requested {
            self.request_render();
        }
        TimerCommandCollection {
            commands,
            render_requested,
        }
    }

    pub(super) fn mark_due_clock_periods(&mut self) {
        let Some(clock_periods) = self.tree.clock_periods.as_ref() else {
            return;
        };
        let ticks = self
            .tree
            .clock_period_ticks
            .get_or_insert_with(FxHashMap::default);
        for (scope, period) in clock_periods {
            let next_tick = clock_period_tick(self.timing.clock_seconds, *period);
            let previous_tick = ticks.entry(scope.clone()).or_insert(next_tick);
            if *previous_tick != next_tick {
                *previous_tick = next_tick;
                self.tree.live_ids.insert(scope.clone());
            }
        }
    }
}

pub(super) fn cleanup_stale_timer_state(
    runtime: &mut Runtime,
    existing_ids: &ElementIdSet,
) -> bool {
    let previous_timers = runtime.timing.timers.len();
    runtime
        .timing
        .timers
        .retain(|id, _| existing_ids.contains(id));
    runtime.timing.timers.len() != previous_timers
}

pub(super) fn sync_clock_period_ticks(
    seconds: f64,
    periods: Option<&ClockPeriodMap>,
    ticks: &mut Option<FxHashMap<ScopeId, u64>>,
) {
    let Some(periods) = periods else {
        *ticks = None;
        return;
    };
    let ticks = ticks.get_or_insert_with(FxHashMap::default);
    ticks.retain(|scope, _| periods.contains_key(scope));
    for (scope, period) in periods {
        ticks
            .entry(scope.clone())
            .or_insert_with(|| clock_period_tick(seconds, *period));
    }
}

pub(super) fn clock_period_tick(seconds: f64, period: Duration) -> u64 {
    if period.is_zero() {
        return 0;
    }
    let period_seconds = period.as_secs_f64().max(f64::EPSILON);
    (seconds.max(0.0) / period_seconds).floor() as u64
}

pub(super) fn collect_timer_ids(elements: &[Element], timers: &mut Vec<(NodeId, f32)>) {
    for element in elements {
        if element.timer_seconds > 0.0 {
            timers.push((NodeId::new(&element.id), element.timer_seconds));
        }
        collect_timer_ids(&element.children, timers);
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use crate::callbacks::TimerCallbackId;

    use super::*;

    #[test]
    fn cleanup_stale_timer_state_keeps_only_existing_ids() {
        let mut runtime = Runtime::new("page");
        runtime.timing.timers.insert(
            NodeId::new("page.kept"),
            TimerState {
                seconds: 1.0,
                elapsed: 0.5,
                seen: true,
                active: true,
            },
        );
        runtime.timing.timers.insert(
            NodeId::new("page.removed"),
            TimerState {
                seconds: 1.0,
                elapsed: 0.5,
                seen: true,
                active: true,
            },
        );
        let existing_ids = element_ids(&["page.kept"]);

        assert!(cleanup_stale_timer_state(&mut runtime, &existing_ids));

        assert!(runtime
            .timing
            .timers
            .contains_key(&NodeId::new("page.kept")));
        assert!(!runtime
            .timing
            .timers
            .contains_key(&NodeId::new("page.removed")));
    }

    #[test]
    fn collect_timer_commands_defers_callback_execution() {
        let fired = Rc::new(Cell::new(false));
        let fired_callback = fired.clone();
        let mut runtime = Runtime::new("page");
        runtime.mark_rendered();
        runtime.input.callbacks.on_timer.insert(
            TimerCallbackId::new(NodeId::new("page.timer")),
            Box::new(move || fired_callback.set(true)),
        );
        let mut timer = Element::new(ElementKind::Rect, "page.timer");
        timer.timer_seconds = 0.1;
        runtime.tree.roots = vec![timer];

        let active = runtime.collect_timer_commands(0.05);

        assert!(active.commands.is_empty());
        assert!(active.render_requested);
        assert!(!fired.get());
        assert!(runtime.diagnostics().current_snapshot().events.is_empty());
        assert!(runtime
            .diagnostics()
            .current_snapshot()
            .invalidations
            .is_empty());

        runtime.mark_rendered();
        let due = runtime.collect_timer_commands(0.05);

        assert_eq!(due.commands.len(), 1);
        assert!(!due.render_requested);
        assert!(!fired.get());
        assert!(runtime.diagnostics().current_snapshot().events.is_empty());
        assert!(runtime
            .diagnostics()
            .current_snapshot()
            .invalidations
            .is_empty());

        let report = runtime.execute_event_commands(due.commands);

        assert!(fired.get());
        assert_eq!(report.command_count, 1);
        assert_eq!(report.callback_count, 1);
        assert_eq!(report.invalidation_count, 1);
        assert!(report.pass_flags.request_compose_ui);
    }

    #[test]
    fn tick_timers_records_input_pass_debug() {
        let fired = Rc::new(Cell::new(false));
        let fired_callback = fired.clone();
        let mut runtime = Runtime::new("page");
        runtime.mark_rendered();
        runtime.input.callbacks.on_timer.insert(
            TimerCallbackId::new(NodeId::new("page.timer")),
            Box::new(move || fired_callback.set(true)),
        );
        let mut timer = Element::new(ElementKind::Rect, "page.timer");
        timer.timer_seconds = 0.1;
        runtime.tree.roots = vec![timer];

        assert!(!runtime.tick_timers(0.05));
        let snapshot = runtime.diagnostics().current_snapshot();
        let input_pass = snapshot
            .input_pass
            .as_ref()
            .expect("active timer tick should record input pass debug");
        assert!(input_pass.timer_render_requested);
        assert_eq!(input_pass.command_count, 0);
        assert_eq!(input_pass.callback_count, 0);
        assert_eq!(input_pass.invalidation_count, 0);
        assert!(runtime.needs_render());
        assert!(!fired.get());

        runtime.mark_rendered();
        assert!(runtime.tick_timers(0.05));
        let snapshot = runtime.diagnostics().current_snapshot();
        let input_pass = snapshot
            .input_pass
            .as_ref()
            .expect("due timer tick should record input pass debug");
        assert!(!input_pass.timer_render_requested);
        assert_eq!(input_pass.command_count, 1);
        assert_eq!(input_pass.callback_count, 1);
        assert_eq!(input_pass.invalidation_count, 1);
        assert!(input_pass.pass_flags.request_compose_ui);
        assert!(fired.get());
    }

    fn element_ids(ids: &[&str]) -> ElementIdSet {
        ids.iter().map(|id| NodeId::new(*id)).collect()
    }
}
