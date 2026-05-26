use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

type AssetIoJob = Box<dyn FnOnce() + Send + 'static>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AssetIoSubmitError {
    QueueFull,
    Closed,
}

pub(crate) struct AssetIoService {
    sender: Option<SyncSender<AssetIoJob>>,
    workers: Vec<JoinHandle<()>>,
    #[cfg(test)]
    queue_capacity: usize,
}

impl AssetIoService {
    pub(crate) fn new(worker_threads: usize, queue_capacity: usize) -> Self {
        let worker_threads = worker_threads.max(1);
        let queue_capacity = queue_capacity.max(1);
        let (sender, receiver) = mpsc::sync_channel::<AssetIoJob>(queue_capacity);
        let receiver = Arc::new(Mutex::new(receiver));
        let mut workers = Vec::with_capacity(worker_threads);

        for _ in 0..worker_threads {
            let receiver = Arc::clone(&receiver);
            workers.push(thread::spawn(move || worker_loop(receiver)));
        }

        Self {
            sender: Some(sender),
            workers,
            #[cfg(test)]
            queue_capacity,
        }
    }

    #[cfg(test)]
    pub(crate) fn worker_count(&self) -> usize {
        self.workers.len()
    }

    #[cfg(test)]
    pub(crate) fn queue_capacity(&self) -> usize {
        self.queue_capacity
    }

    pub(crate) fn submit(
        &self,
        job: impl FnOnce() + Send + 'static,
    ) -> Result<(), AssetIoSubmitError> {
        let Some(sender) = &self.sender else {
            return Err(AssetIoSubmitError::Closed);
        };

        match sender.try_send(Box::new(job)) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => Err(AssetIoSubmitError::QueueFull),
            Err(TrySendError::Disconnected(_)) => Err(AssetIoSubmitError::Closed),
        }
    }
}

impl Drop for AssetIoService {
    fn drop(&mut self) {
        self.sender.take();
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

fn worker_loop(receiver: Arc<Mutex<Receiver<AssetIoJob>>>) {
    loop {
        let job = {
            let receiver = receiver.lock().expect("asset io worker receiver poisoned");
            receiver.recv()
        };

        match job {
            Ok(job) => job(),
            Err(_) => break,
        }
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
            .submit(move || {
                started_tx.send(()).expect("started receiver alive");
                release_rx.recv().expect("release sender alive");
            })
            .expect("first job should start");
        started_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("worker should start first job");

        let (queued_tx, queued_rx) = mpsc::channel();
        service
            .submit(move || {
                queued_tx.send(()).expect("queued receiver alive");
            })
            .expect("second job should fill queue");

        assert_eq!(
            service.submit(|| {}).unwrap_err(),
            AssetIoSubmitError::QueueFull
        );
        assert!(queued_rx.recv_timeout(Duration::from_millis(10)).is_err());

        release_tx.send(()).expect("worker should still wait");
    }
}
