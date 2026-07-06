use std::time::Duration;

use super::AssetLoadTimingSample;
use crate::asset::types::AssetLoadTimingStats;
#[derive(Clone, Debug, Default)]
pub(super) struct AssetLoadTimingAccumulator {
    completed_source_loads: usize,
    failed_source_loads: usize,
    total_read_time: Duration,
    total_decode_time: Duration,
    total_load_time: Duration,
}
impl AssetLoadTimingAccumulator {
    pub(super) fn record(&mut self, sample: AssetLoadTimingSample, succeeded: bool) {
        if !sample.sampled {
            return;
        }
        if succeeded {
            self.completed_source_loads += 1;
        } else {
            self.failed_source_loads += 1;
        }
        self.total_read_time += sample.read_time;
        self.total_decode_time += sample.decode_time;
        self.total_load_time += sample.total_time;
    }
    pub(super) fn stats(&self) -> AssetLoadTimingStats {
        let samples = self.completed_source_loads + self.failed_source_loads;
        AssetLoadTimingStats {
            completed_source_loads: self.completed_source_loads,
            failed_source_loads: self.failed_source_loads,
            average_read_time: average_duration(self.total_read_time, samples),
            average_decode_time: average_duration(self.total_decode_time, samples),
            average_total_time: average_duration(self.total_load_time, samples),
        }
    }
}
fn average_duration(total: Duration, samples: usize) -> Option<Duration> {
    if samples == 0 {
        return None;
    }
    Some(Duration::from_secs_f64(
        total.as_secs_f64() / samples as f64,
    ))
}
