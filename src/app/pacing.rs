//! Frame pacing helpers for the app runner.

use std::time::{Duration, Instant};

pub(crate) fn frame_interval(frame_rate_limit: Option<f64>) -> Option<Duration> {
    frame_rate_limit
        .filter(|fps| *fps > 0.0)
        .map(|fps| Duration::from_secs_f64(1.0 / fps))
}

pub(crate) fn next_frame_deadline(
    last_frame_time: Option<Instant>,
    frame_rate_limit: Option<f64>,
    now: Instant,
) -> Option<Instant> {
    let last_frame_time = last_frame_time?;
    let interval = frame_interval(frame_rate_limit)?;
    let next_frame = last_frame_time + interval;
    (now < next_frame).then_some(next_frame)
}
