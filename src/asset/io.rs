use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering as AtomicOrdering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

type AssetIoJob = Box<dyn FnOnce() + Send + 'static>;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct AssetIoPriority(i32);

impl AssetIoPriority {
    pub(crate) fn new(value: i32) -> Self {
        Self(value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AssetIoSubmitError {
    QueueFull,
    Closed,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct AssetIoCancelToken {
    canceled: Arc<AtomicBool>,
}

impl AssetIoCancelToken {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn cancel(&self) {
        self.canceled.store(true, AtomicOrdering::Release);
    }

    pub(crate) fn is_canceled(&self) -> bool {
        self.canceled.load(AtomicOrdering::Acquire)
    }
}

pub(crate) struct AssetIoService {
    shared: Arc<SharedQueue>,
    workers: Vec<WorkerThread>,
    shutdown_timeout: Option<Duration>,
}

impl AssetIoService {
    #[cfg(test)]
    pub(crate) fn new(worker_threads: usize, queue_capacity: usize) -> Self {
        Self::with_shutdown_timeout(worker_threads, queue_capacity, None)
    }

    pub(crate) fn with_shutdown_timeout(
        worker_threads: usize,
        queue_capacity: usize,
        shutdown_timeout: Option<Duration>,
    ) -> Self {
        let worker_threads = worker_threads.max(1);
        let queue_capacity = queue_capacity.max(1);
        let shared = Arc::new(SharedQueue {
            state: Mutex::new(QueueState {
                jobs: BinaryHeap::new(),
                capacity: queue_capacity,
                next_sequence: 0,
                closed: false,
            }),
            available: Condvar::new(),
            running_jobs: AtomicUsize::new(0),
        });
        let mut workers = Vec::with_capacity(worker_threads);

        for _ in 0..worker_threads {
            let (exited_tx, exited_rx) = mpsc::channel();
            let handle = thread::spawn({
                let shared = Arc::clone(&shared);
                move || {
                    worker_loop(shared);
                    let _ = exited_tx.send(());
                }
            });
            workers.push(WorkerThread {
                handle: Some(handle),
                exited_rx,
            });
        }

        Self {
            shared,
            workers,
            shutdown_timeout,
        }
    }

    pub(crate) fn worker_count(&self) -> usize {
        self.workers.len()
    }

    pub(crate) fn queue_capacity(&self) -> usize {
        self.shared
            .state
            .lock()
            .expect("asset io queue mutex poisoned")
            .capacity
    }

    #[cfg(test)]
    pub(crate) fn submit(
        &self,
        priority: AssetIoPriority,
        job: impl FnOnce() + Send + 'static,
    ) -> Result<(), AssetIoSubmitError> {
        self.submit_cancelable(priority, job).map(|_| ())
    }

    pub(crate) fn submit_cancelable(
        &self,
        priority: AssetIoPriority,
        job: impl FnOnce() + Send + 'static,
    ) -> Result<AssetIoCancelToken, AssetIoSubmitError> {
        let mut state = self
            .shared
            .state
            .lock()
            .expect("asset io queue mutex poisoned");
        if state.closed {
            return Err(AssetIoSubmitError::Closed);
        }
        state.prune_canceled_jobs();
        if state.jobs.len() >= state.capacity {
            return Err(AssetIoSubmitError::QueueFull);
        }

        let sequence = state.next_sequence;
        state.next_sequence = state.next_sequence.wrapping_add(1);
        let cancel_token = AssetIoCancelToken::new();
        state.jobs.push(QueuedJob {
            priority,
            sequence,
            queued_at: Instant::now(),
            cancel_token: cancel_token.clone(),
            job: Some(Box::new(job)),
        });
        drop(state);
        self.shared.available.notify_one();
        Ok(cancel_token)
    }

    pub(crate) fn queued_len(&self) -> usize {
        let mut state = self
            .shared
            .state
            .lock()
            .expect("asset io queue mutex poisoned");
        state.prune_canceled_jobs();
        state.jobs.len()
    }

    pub(crate) fn oldest_queued_age(&self, now: Instant) -> Option<Duration> {
        let mut state = self
            .shared
            .state
            .lock()
            .expect("asset io queue mutex poisoned");
        state.prune_canceled_jobs();
        state
            .jobs
            .iter()
            .map(|job| now.saturating_duration_since(job.queued_at))
            .max()
    }

    pub(crate) fn running_len(&self) -> usize {
        self.shared.running_jobs.load(AtomicOrdering::Acquire)
    }
}

impl Drop for AssetIoService {
    fn drop(&mut self) {
        {
            let mut state = self
                .shared
                .state
                .lock()
                .expect("asset io queue mutex poisoned");
            state.closed = true;
            state.jobs.clear();
        }
        self.shared.available.notify_all();
        if let Some(timeout) = self.shutdown_timeout {
            let deadline = Instant::now() + timeout;
            for mut worker in self.workers.drain(..) {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    continue;
                }
                if worker.exited_rx.recv_timeout(remaining).is_ok() {
                    if let Some(handle) = worker.handle.take() {
                        let _ = handle.join();
                    }
                }
            }
        } else {
            for mut worker in self.workers.drain(..) {
                if let Some(handle) = worker.handle.take() {
                    let _ = handle.join();
                }
            }
        }
    }
}

struct WorkerThread {
    handle: Option<JoinHandle<()>>,
    exited_rx: Receiver<()>,
}

struct SharedQueue {
    state: Mutex<QueueState>,
    available: Condvar,
    running_jobs: AtomicUsize,
}

struct QueueState {
    jobs: BinaryHeap<QueuedJob>,
    capacity: usize,
    next_sequence: u64,
    closed: bool,
}

impl QueueState {
    fn prune_canceled_jobs(&mut self) {
        self.jobs.retain(|job| !job.cancel_token.is_canceled());
    }
}

struct QueuedJob {
    priority: AssetIoPriority,
    sequence: u64,
    queued_at: Instant,
    cancel_token: AssetIoCancelToken,
    job: Option<AssetIoJob>,
}

impl Ord for QueuedJob {
    fn cmp(&self, other: &Self) -> Ordering {
        self.priority
            .cmp(&other.priority)
            .then_with(|| other.sequence.cmp(&self.sequence))
    }
}

impl PartialOrd for QueuedJob {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for QueuedJob {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority && self.sequence == other.sequence
    }
}

impl Eq for QueuedJob {}

fn worker_loop(shared: Arc<SharedQueue>) {
    loop {
        let job = {
            let mut state = shared.state.lock().expect("asset io queue mutex poisoned");
            loop {
                if let Some(mut queued) = state.jobs.pop() {
                    if queued.cancel_token.is_canceled() {
                        continue;
                    }
                    break queued.job.take().expect("queued asset io job missing");
                }
                if state.closed {
                    return;
                }
                state = shared
                    .available
                    .wait(state)
                    .expect("asset io queue mutex poisoned");
            }
        };

        let _running = RunningJobGuard::new(&shared.running_jobs);
        job();
    }
}

struct RunningJobGuard<'a> {
    running_jobs: &'a AtomicUsize,
}

impl<'a> RunningJobGuard<'a> {
    fn new(running_jobs: &'a AtomicUsize) -> Self {
        running_jobs.fetch_add(1, AtomicOrdering::AcqRel);
        Self { running_jobs }
    }
}

impl Drop for RunningJobGuard<'_> {
    fn drop(&mut self) {
        self.running_jobs.fetch_sub(1, AtomicOrdering::AcqRel);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn submit_reports_full_queue() {
        let service = AssetIoService::new(1, 1);
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();

        service
            .submit(AssetIoPriority::default(), move || {
                started_tx.send(()).expect("started receiver alive");
                release_rx.recv().expect("release sender alive");
            })
            .expect("first job should start");
        started_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("worker should start first job");
        assert_eq!(service.running_len(), 1);

        let (queued_tx, queued_rx) = mpsc::channel();
        service
            .submit(AssetIoPriority::default(), move || {
                queued_tx.send(()).expect("queued receiver alive");
            })
            .expect("second job should fill queue");
        assert_eq!(service.queued_len(), 1);
        assert!(service.oldest_queued_age(Instant::now()).is_some());

        assert_eq!(
            service
                .submit(AssetIoPriority::default(), || {})
                .unwrap_err(),
            AssetIoSubmitError::QueueFull
        );
        assert!(queued_rx.recv_timeout(Duration::from_millis(10)).is_err());

        release_tx.send(()).expect("worker should still wait");
    }

    #[test]
    fn higher_priority_jobs_run_before_lower_priority_queued_jobs(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let service = AssetIoService::new(1, 3);
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();

        service
            .submit(AssetIoPriority::new(0), move || {
                started_tx.send(()).expect("started receiver alive");
                release_rx.recv().expect("release sender alive");
            })
            .expect("first job should start");
        started_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("worker should start first job");

        for (priority, label) in [(0, "low"), (10, "high"), (5, "medium")] {
            let done_tx = done_tx.clone();
            service
                .submit(AssetIoPriority::new(priority), move || {
                    done_tx.send(label).expect("done receiver alive");
                })
                .expect("queued job should fit");
        }
        drop(done_tx);

        release_tx.send(()).expect("worker should still wait");
        assert_eq!(done_rx.recv_timeout(Duration::from_secs(1))?, "high");
        assert_eq!(done_rx.recv_timeout(Duration::from_secs(1))?, "medium");
        assert_eq!(done_rx.recv_timeout(Duration::from_secs(1))?, "low");
        Ok(())
    }

    #[test]
    fn canceled_queued_job_is_skipped_before_start() -> Result<(), Box<dyn std::error::Error>> {
        let service = AssetIoService::new(1, 2);
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (queued_tx, queued_rx) = mpsc::channel();

        service
            .submit(AssetIoPriority::default(), move || {
                started_tx.send(()).expect("started receiver alive");
                release_rx.recv().expect("release sender alive");
            })
            .expect("blocking job should submit");
        started_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("worker should start blocking job");

        let cancel = service
            .submit_cancelable(AssetIoPriority::default(), move || {
                queued_tx.send(()).expect("queued receiver alive");
            })
            .expect("queued job should submit");
        cancel.cancel();

        release_tx.send(()).expect("worker should still wait");
        assert!(queued_rx.recv_timeout(Duration::from_millis(20)).is_err());
        Ok(())
    }

    #[test]
    fn canceled_queued_job_frees_queue_capacity_before_worker_polls(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let service = AssetIoService::new(1, 1);
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (queued_tx, queued_rx) = mpsc::channel();

        service
            .submit(AssetIoPriority::default(), move || {
                started_tx.send(()).expect("started receiver alive");
                release_rx.recv().expect("release sender alive");
            })
            .expect("blocking job should submit");
        started_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("worker should start blocking job");

        let canceled = service
            .submit_cancelable(AssetIoPriority::default(), || {})
            .expect("queued job should fill capacity");
        assert_eq!(service.queued_len(), 1);
        canceled.cancel();
        assert_eq!(service.queued_len(), 0);

        service
            .submit(AssetIoPriority::default(), move || {
                queued_tx.send(()).expect("queued receiver alive");
            })
            .expect("canceled queued job should not keep capacity full");

        release_tx.send(()).expect("worker should still wait");
        queued_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("replacement queued job should run");
        Ok(())
    }

    #[test]
    fn drop_waits_for_running_worker_when_no_shutdown_timeout(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let service = AssetIoService::new(1, 1);
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();

        service
            .submit(AssetIoPriority::default(), move || {
                started_tx.send(()).expect("started receiver alive");
                release_rx.recv().expect("release sender alive");
                done_tx.send(()).expect("done receiver alive");
            })
            .expect("job should submit");
        started_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("worker should start blocking job");

        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(20));
            release_tx.send(()).expect("worker should still wait");
        });
        drop(service);

        done_rx
            .try_recv()
            .expect("service drop should wait for running worker");
        Ok(())
    }

    #[test]
    fn drop_with_timeout_discards_queued_jobs_without_waiting_for_blocked_worker(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let service = AssetIoService::with_shutdown_timeout(1, 2, Some(Duration::ZERO));
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (queued_tx, queued_rx) = mpsc::channel();

        service
            .submit(AssetIoPriority::default(), move || {
                started_tx.send(()).expect("started receiver alive");
                release_rx.recv().expect("release sender alive");
            })
            .expect("blocking job should submit");
        started_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("worker should start blocking job");
        service
            .submit(AssetIoPriority::default(), move || {
                queued_tx.send(()).expect("queued receiver alive");
            })
            .expect("queued job should submit");

        drop(service);

        assert!(queued_rx.recv_timeout(Duration::from_millis(20)).is_err());
        release_tx
            .send(())
            .expect("detached worker should still wait");
        Ok(())
    }
}
