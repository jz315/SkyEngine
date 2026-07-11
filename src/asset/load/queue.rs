use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use super::timing::AssetLoadTimingAccumulator;
use super::{AssetLoadPhaseTracker, AssetLoadTimingSample, AssetSourceLoadPhase, CompletedLoad};
use crate::asset::io::{AssetIoCancelToken, AssetIoPriority, AssetIoService, AssetIoSubmitError};
use crate::asset::store::AssetStore;
use crate::asset::types::{AssetError, AssetId, AssetLoadTimingStats, AssetSourceLoadPhaseCounts};

pub(crate) struct AssetLoadQueue {
    load_tx: Sender<CompletedLoad>,
    load_rx: Receiver<CompletedLoad>,
    io: AssetIoService,
    inflight_loads: HashMap<(AssetId, u64), InflightLoad>,
    timings: AssetLoadTimingAccumulator,
    deferred_submissions: usize,
}

struct InflightLoad {
    cancel_token: AssetIoCancelToken,
    phase: AssetLoadPhaseTracker,
}

impl AssetLoadQueue {
    #[cfg(test)]
    pub(crate) fn new(worker_threads: usize, queue_capacity: usize) -> Self {
        Self::with_shutdown_timeout(worker_threads, queue_capacity, None)
    }

    pub(crate) fn with_shutdown_timeout(
        worker_threads: usize,
        queue_capacity: usize,
        shutdown_timeout: Option<Duration>,
    ) -> Self {
        let (load_tx, load_rx) = mpsc::channel();
        Self {
            load_tx,
            load_rx,
            io: AssetIoService::with_shutdown_timeout(
                worker_threads,
                queue_capacity,
                shutdown_timeout,
            ),
            inflight_loads: HashMap::default(),
            timings: AssetLoadTimingAccumulator::default(),
            deferred_submissions: 0,
        }
    }

    pub(crate) fn inflight_len(&self) -> usize {
        self.inflight_loads.len()
    }

    pub(crate) fn queued_len(&self) -> usize {
        self.io.queued_len()
    }

    pub(crate) fn oldest_queued_age(&self, now: Instant) -> Option<Duration> {
        self.io.oldest_queued_age(now)
    }

    pub(crate) fn source_load_phase_counts(&self) -> AssetSourceLoadPhaseCounts {
        let mut counts = AssetSourceLoadPhaseCounts::default();
        for inflight in self.inflight_loads.values() {
            match inflight.phase.phase() {
                AssetSourceLoadPhase::Queued => counts.queued += 1,
                AssetSourceLoadPhase::Reading => counts.reading += 1,
                AssetSourceLoadPhase::Decoding => counts.decoding += 1,
            }
        }
        counts
    }

    pub(crate) fn running_len(&self) -> usize {
        self.io.running_len()
    }

    pub(crate) fn queue_capacity(&self) -> usize {
        self.io.queue_capacity()
    }

    pub(crate) fn deferred_submission_count(&self) -> usize {
        self.deferred_submissions
    }

    pub(crate) fn timing_stats(&self) -> AssetLoadTimingStats {
        self.timings.stats()
    }

    pub(crate) fn record_timing(&mut self, sample: AssetLoadTimingSample, succeeded: bool) {
        self.timings.record(sample, succeeded);
    }

    pub(crate) fn worker_count(&self) -> usize {
        self.io.worker_count()
    }

    pub(crate) fn contains(&self, id: AssetId, generation: u64) -> bool {
        self.inflight_loads.contains_key(&(id, generation))
    }

    pub(crate) fn phase(&self, id: AssetId, generation: u64) -> Option<AssetSourceLoadPhase> {
        self.inflight_loads
            .get(&(id, generation))
            .map(|inflight| inflight.phase.phase())
    }

    pub(crate) fn cancel(&mut self, id: AssetId, generation: u64) -> bool {
        let Some(inflight) = self.inflight_loads.remove(&(id, generation)) else {
            return false;
        };
        inflight.cancel_token.cancel();
        true
    }

    pub(crate) fn cancel_non_loading(&mut self, store: &AssetStore) -> usize {
        let stale = self
            .inflight_loads
            .keys()
            .copied()
            .filter(|(id, generation)| store.should_cancel_load(*id, *generation))
            .collect::<Vec<_>>();
        let count = stale.len();
        for (id, generation) in stale {
            self.cancel(id, generation);
        }
        count
    }

    #[cfg(test)]
    pub(crate) fn submit(
        &mut self,
        id: AssetId,
        generation: u64,
        priority: i32,
        job: impl FnOnce() -> CompletedLoad + Send + 'static,
    ) -> Result<bool, AssetError> {
        self.submit_with_phase(id, generation, priority, move |phase| {
            phase.set(AssetSourceLoadPhase::Reading);
            job()
        })
    }

    pub(crate) fn submit_with_phase(
        &mut self,
        id: AssetId,
        generation: u64,
        priority: i32,
        job: impl FnOnce(AssetLoadPhaseTracker) -> CompletedLoad + Send + 'static,
    ) -> Result<bool, AssetError> {
        if self.inflight_loads.contains_key(&(id, generation)) {
            return Ok(false);
        }
        let tx = self.load_tx.clone();
        let phase = AssetLoadPhaseTracker::new();
        let phase_for_job = phase.clone();
        match self
            .io
            .submit_cancelable(AssetIoPriority::new(priority), move || {
                let _ = tx.send(job(phase_for_job));
            }) {
            Ok(cancel_token) => {
                self.inflight_loads.insert(
                    (id, generation),
                    InflightLoad {
                        cancel_token,
                        phase,
                    },
                );
                Ok(true)
            }
            Err(AssetIoSubmitError::QueueFull) => {
                self.deferred_submissions = self.deferred_submissions.saturating_add(1);
                Ok(false)
            }
            Err(AssetIoSubmitError::Closed) => Err(AssetError::Internal {
                message: "asset io service is closed".to_string(),
            }),
        }
    }

    pub(crate) fn drain_ready(&mut self) -> Vec<CompletedLoad> {
        let mut completions = Vec::new();
        while let Ok(completion) = self.load_rx.try_recv() {
            self.inflight_loads
                .remove(&(completion.id, completion.generation));
            self.record_timing(completion.timings, completion.result.is_ok());
            completions.push(completion);
        }
        completions
    }
}
