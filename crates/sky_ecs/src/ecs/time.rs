/// Timing information updated automatically by [`World`](crate::ecs::World).
///
/// `Time` is built into `World` because it is core frame state rather than a
/// user-owned resource.  Systems usually read [`delta`](Self::delta).  Render
/// and UI-facing code should prefer [`frame_delta`](Self::frame_delta), because
/// fixed stages temporarily set `delta` to their fixed step while they run.
#[derive(Debug, Clone)]
pub struct Time {
    /// Delta time for the currently running stage invocation.
    ///
    /// Equals [`frame_delta`](Self::frame_delta) for every-frame stages, or the
    /// fixed step size while a fixed stage is running.
    pub delta: f32,

    /// Delta for the current application frame after clamping and
    /// [`time_scale`](Self::time_scale).
    pub frame_delta: f32,

    /// Raw real-world frame delta before clamping and time scaling.
    pub raw_delta: f32,

    /// Total elapsed time since the first tick, affected by
    /// [`time_scale`](Self::time_scale).
    pub elapsed: f32,

    /// Real elapsed time since the first tick, unaffected by
    /// [`time_scale`](Self::time_scale).
    pub raw_elapsed: f32,

    /// Number of ticks since the schedule started.
    pub frame_count: u64,

    /// Fraction of the next built-in [`FixedUpdate`](crate::ecs::FixedUpdate)
    /// step accumulated after fixed execution.
    ///
    /// This is in `0.0..=1.0`; carried backlog is clamped to `1.0` until the
    /// fixed stage catches up. Render systems can use it for interpolation.
    /// Custom fixed stages expose their accumulator through schedule
    /// diagnostics and do not overwrite this value.
    pub fixed_alpha: f32,

    /// Time multiplier. 1.0 = normal, 0.5 = slow-mo, 0.0 = paused.
    pub time_scale: f32,

    // Ordinary systems may share Time but must not request ResMut<Time>.
    // Exclusive code can still mutate it through World between worker waves.
    _not_send: std::marker::PhantomData<std::rc::Rc<()>>,
}

// Safety: the marker is zero-sized and all actual Time fields are primitive
// values. Schedule execution permits only shared worker access to Time.
unsafe impl Sync for Time {}

impl Time {
    #[inline]
    pub fn delta(&self) -> f32 {
        self.delta
    }

    #[inline]
    pub fn frame_delta(&self) -> f32 {
        self.frame_delta
    }

    #[inline]
    pub fn raw_delta(&self) -> f32 {
        self.raw_delta
    }

    #[inline]
    pub fn elapsed(&self) -> f32 {
        self.elapsed
    }

    #[inline]
    pub fn raw_elapsed(&self) -> f32 {
        self.raw_elapsed
    }

    #[inline]
    pub fn fixed_alpha(&self) -> f32 {
        self.fixed_alpha
    }
}

impl Default for Time {
    fn default() -> Self {
        Self {
            delta: 0.0,
            frame_delta: 0.0,
            raw_delta: 0.0,
            elapsed: 0.0,
            raw_elapsed: 0.0,
            frame_count: 0,
            fixed_alpha: 0.0,
            time_scale: 1.0,
            _not_send: std::marker::PhantomData,
        }
    }
}
