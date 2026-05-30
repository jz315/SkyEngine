use super::*;

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct TimerState {
    seconds: f32,
    elapsed: f32,
    seen: bool,
    active: bool,
}

impl Runtime {
    pub fn tick_timers(&mut self, delta_seconds: f32) -> bool {
        for state in self.timing.timers.values_mut() {
            state.seen = false;
        }
        let mut fired = Vec::new();
        collect_timer_ids(&self.tree.roots, &mut fired);
        let mut changed = false;
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
                if let Some(callback) = self.input.callbacks.on_timer.get_mut(&id) {
                    callback();
                    self.record_invalidation(Invalidation::timer(id.clone()));
                    self.mark_compose_dirty();
                    changed = true;
                }
            } else if state.active {
                self.request_render();
            }
        }
        self.timing.timers.retain(|_, state| state.seen);
        changed
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

pub(super) fn sync_clock_period_ticks(
    seconds: f64,
    periods: Option<&ClockPeriodMap>,
    ticks: &mut Option<FxHashMap<String, u64>>,
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

pub(super) fn collect_timer_ids(elements: &[Element], timers: &mut Vec<(String, f32)>) {
    for element in elements {
        if element.timer_seconds > 0.0 {
            timers.push((element.id.clone(), element.timer_seconds));
        }
        collect_timer_ids(&element.children, timers);
    }
}
