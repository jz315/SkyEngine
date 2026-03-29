/// Timing information updated automatically by [`World::tick`].
#[derive(Debug, Clone)]
pub struct Time {
    /// Delta time for the current group.
    /// Equals the real frame delta for normal groups,
    /// or the fixed step size for `.fixed()` groups.
    pub delta: f32,
    /// Total elapsed time since the first tick (affected by time_scale).
    pub elapsed: f32,
    /// Number of ticks since the schedule started.
    pub frame_count: u64,
    /// Time multiplier. 1.0 = normal, 0.5 = slow-mo, 0.0 = paused.
    pub time_scale: f32,
}

impl Default for Time {
    fn default() -> Self {
        Self {
            delta: 0.0,
            elapsed: 0.0,
            frame_count: 0,
            time_scale: 1.0,
        }
    }
}
