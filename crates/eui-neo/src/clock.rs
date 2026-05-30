use std::time::Duration;

use rustc_hash::FxHashMap;

use crate::retained::ScopeSet;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClockTick {
    pub seconds: f32,
    pub frame_index: u64,
    pub period: Duration,
}

pub(crate) type ClockPeriodMap = FxHashMap<String, Duration>;

#[derive(Debug)]
pub struct UiClock<'ui> {
    pub(crate) seconds: f64,
    pub(crate) frame_index: u64,
    pub(crate) owner: Option<String>,
    pub(crate) live_scopes: &'ui mut ScopeSet,
    pub(crate) clock_ids: &'ui mut ScopeSet,
    pub(crate) clock_periods: &'ui mut Option<ClockPeriodMap>,
}

impl UiClock<'_> {
    pub fn seconds(mut self) -> f32 {
        self.register_dependency();
        self.seconds as f32
    }

    pub fn frame_index(mut self) -> u64 {
        self.register_dependency();
        self.frame_index
    }

    pub fn every(mut self, period: Duration) -> ClockTick {
        self.register_periodic_dependency(period);
        ClockTick {
            seconds: self.seconds as f32,
            frame_index: self.frame_index,
            period,
        }
    }

    fn register_dependency(&mut self) {
        if let Some(owner) = self.owner.as_ref() {
            self.live_scopes.insert(owner.clone());
            self.clock_ids.insert(owner.clone());
        }
    }

    fn register_periodic_dependency(&mut self, period: Duration) {
        if let Some(owner) = self.owner.as_ref() {
            self.clock_ids.insert(owner.clone());
            if period.is_zero() {
                self.live_scopes.insert(owner.clone());
            } else {
                self.clock_periods
                    .get_or_insert_with(ClockPeriodMap::default)
                    .insert(owner.clone(), period);
            }
        }
    }
}
