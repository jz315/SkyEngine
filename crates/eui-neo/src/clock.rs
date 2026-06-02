use std::time::Duration;

use rustc_hash::FxHashMap;

use crate::retained::{ScopeId, ScopeSet};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClockTick {
    pub seconds: f32,
    pub frame_index: u64,
    pub period: Duration,
}

pub(crate) type ClockPeriodMap = FxHashMap<ScopeId, Duration>;

pub(crate) fn preserve_reused_scope_clock_dependency(
    id: &ScopeId,
    previous_clock_periods: Option<&ClockPeriodMap>,
    clock_ids: &mut ScopeSet,
    clock_periods: &mut Option<ClockPeriodMap>,
) -> bool {
    let Some(previous_clock_periods) = previous_clock_periods else {
        return false;
    };
    let Some(period) = previous_clock_periods.get(id).copied() else {
        return false;
    };
    clock_ids.insert(id.clone());
    clock_periods
        .get_or_insert_with(ClockPeriodMap::default)
        .insert(id.clone(), period);
    true
}

#[derive(Debug)]
pub struct UiClock<'ui> {
    pub(crate) seconds: f64,
    pub(crate) frame_index: u64,
    pub(crate) owner: Option<ScopeId>,
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

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::retained::{ScopeId, ScopeSet};

    use super::{preserve_reused_scope_clock_dependency, ClockPeriodMap};

    #[test]
    fn reused_scope_preserves_previous_periodic_clock_dependency() {
        let id = ScopeId::new("page.clock");
        let mut previous_clock_periods = ClockPeriodMap::default();
        previous_clock_periods.insert(id.clone(), Duration::from_millis(250));
        let mut clock_ids = ScopeSet::default();
        let mut clock_periods = None;

        let preserved = preserve_reused_scope_clock_dependency(
            &id,
            Some(&previous_clock_periods),
            &mut clock_ids,
            &mut clock_periods,
        );

        assert!(preserved);
        assert!(clock_ids.contains(&id));
        assert_eq!(
            clock_periods.as_ref().and_then(|periods| periods.get(&id)),
            Some(&Duration::from_millis(250))
        );
    }

    #[test]
    fn reused_scope_without_previous_clock_dependency_is_ignored() {
        let id = ScopeId::new("page.clean");
        let mut previous_clock_periods = ClockPeriodMap::default();
        previous_clock_periods.insert(ScopeId::new("page.other"), Duration::from_millis(250));
        let mut clock_ids = ScopeSet::default();
        let mut clock_periods = None;

        let preserved = preserve_reused_scope_clock_dependency(
            &id,
            Some(&previous_clock_periods),
            &mut clock_ids,
            &mut clock_periods,
        );

        assert!(!preserved);
        assert!(clock_ids.is_empty());
        assert!(clock_periods.is_none());
    }
}
