use crate::asset::io::*;
use std::sync::mpsc;
use std::time::{Duration, Instant};

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
fn drop_waits_for_running_worker_when_no_shutdown_timeout() -> Result<(), Box<dyn std::error::Error>>
{
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
