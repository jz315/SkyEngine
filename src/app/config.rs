//! Application configuration types installed by app capability plugins.

/// Frame scheduling policy for the application runner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedrawMode {
    /// Continuously request redraws while the window is visible.
    ///
    /// This is the default for game-style applications that want steady
    /// update/render cadence.
    Continuous,
    /// Only redraw when the app becomes dirty or explicitly requests it.
    ///
    /// This is useful for editors, tools, and UI-heavy apps that should
    /// sleep when nothing changes on screen.
    Reactive,
}

/// Unit used by the app runner when applying the initial window size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowSizeMode {
    /// Interpret the configured window width and height as logical pixels.
    Logical,
    /// Interpret the configured window width and height as physical pixels.
    Physical,
}

/// Window capability options installed by [`WindowPlugin`](crate::app::WindowPlugin).
#[derive(Clone, Debug)]
pub struct WindowOptions {
    /// Window title.
    pub title: String,
    /// Initial window width. See [`WindowSizeMode`] for units.
    pub width: u32,
    /// Initial window height. See [`WindowSizeMode`] for units.
    pub height: u32,
    /// Unit used for the initial window size.
    pub size_mode: WindowSizeMode,
    /// Enable vsync.
    pub vsync: bool,
    /// Allow window resizing.
    pub resizable: bool,
}

impl WindowOptions {
    pub fn new(title: impl Into<String>, width: u32, height: u32) -> Self {
        Self {
            title: title.into(),
            width,
            height,
            size_mode: WindowSizeMode::Logical,
            vsync: true,
            resizable: true,
        }
    }

    #[inline]
    pub fn with_vsync(mut self, vsync: bool) -> Self {
        self.vsync = vsync;
        self
    }

    #[inline]
    pub fn with_resizable(mut self, resizable: bool) -> Self {
        self.resizable = resizable;
        self
    }

    #[inline]
    pub fn with_physical_window_size(mut self, physical: bool) -> Self {
        self.size_mode = if physical {
            WindowSizeMode::Physical
        } else {
            WindowSizeMode::Logical
        };
        self
    }
}

impl Default for WindowOptions {
    fn default() -> Self {
        Self::new("SkyEngine", 1280, 720)
    }
}

/// Frame runner options installed by [`RunnerPlugin`](crate::app::RunnerPlugin).
#[derive(Clone, Debug)]
pub struct RunnerOptions {
    /// Automatically exit on Escape key.
    pub exit_on_escape: bool,
    /// Maximum frame delta in seconds.
    pub max_delta: f32,
    /// Automatically advance the ECS schedule each frame.
    pub auto_tick: bool,
    /// How redraws are scheduled.
    pub redraw_mode: RedrawMode,
    /// Optional frame-rate cap. `None` means redraw as fast as the platform allows.
    pub frame_rate_limit: Option<f64>,
}

impl Default for RunnerOptions {
    fn default() -> Self {
        Self {
            exit_on_escape: true,
            max_delta: 0.1,
            auto_tick: true,
            redraw_mode: RedrawMode::Continuous,
            frame_rate_limit: None,
        }
    }
}

impl RunnerOptions {
    /// Game-style continuous runner defaults.
    #[inline]
    pub fn game() -> Self {
        Self::default()
    }

    /// Tool/editor-style runner that redraws only when requested.
    #[inline]
    pub fn reactive() -> Self {
        Self {
            redraw_mode: RedrawMode::Reactive,
            ..Self::default()
        }
    }

    #[inline]
    pub fn with_exit_on_escape(mut self, exit: bool) -> Self {
        self.exit_on_escape = exit;
        self
    }

    #[inline]
    pub fn with_max_delta(mut self, max_delta: f32) -> Self {
        self.max_delta = max_delta;
        self
    }

    #[inline]
    pub fn with_auto_tick(mut self, auto_tick: bool) -> Self {
        self.auto_tick = auto_tick;
        self
    }

    #[inline]
    pub fn with_redraw_mode(mut self, redraw_mode: RedrawMode) -> Self {
        self.redraw_mode = redraw_mode;
        self
    }

    #[inline]
    pub fn with_frame_rate_limit(mut self, fps: f64) -> Self {
        self.frame_rate_limit = (fps > 0.0).then_some(fps);
        self
    }
}
