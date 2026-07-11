#[cfg(feature = "render-timings")]
use std::time::Instant;

/// Populated only when the `render-timings` feature is enabled.
/// Without that feature, timing fields remain zeroed.
#[derive(Debug, Clone, Copy, Default)]
pub struct RenderTimingStats {
    pub frame_ms: f64,
    pub extract_ms: f64,
    pub resize_ms: f64,
    pub prepare_ms: f64,
    pub upload_ms: f64,
    pub execute_ms: f64,
}

#[cfg(feature = "render-timings")]
pub(crate) type TimingStart = Instant;

#[cfg(not(feature = "render-timings"))]
#[derive(Clone, Copy)]
pub(crate) struct TimingStart;

#[inline]
pub(crate) fn timing_start() -> TimingStart {
    #[cfg(feature = "render-timings")]
    {
        Instant::now()
    }

    #[cfg(not(feature = "render-timings"))]
    {
        TimingStart
    }
}

#[inline]
#[cfg(feature = "render-timings")]
pub(crate) fn elapsed_ms(start: TimingStart) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

#[inline]
#[cfg(not(feature = "render-timings"))]
pub(crate) fn elapsed_ms(_start: TimingStart) -> f64 {
    0.0
}
