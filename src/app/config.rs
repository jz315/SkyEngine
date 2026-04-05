//! Application configuration.

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

/// Configuration for creating a SkyEngine application window.
pub struct AppConfig {
    /// Window title.
    pub title: String,
    /// Initial window width in logical pixels.
    pub width: u32,
    /// Initial window height in logical pixels.
    pub height: u32,
    /// Enable vsync (default: true).
    pub vsync: bool,
    /// Allow window resizing (default: true).
    pub resizable: bool,
    /// Automatically exit on Escape key (default: true).
    pub exit_on_escape: bool,
    /// Maximum frame delta in seconds (default: 0.1).
    ///
    /// Prevents physics explosions caused by debugger breakpoints or OS stalls.
    pub max_delta: f32,
    /// Automatically call `world.tick_with_delta(dt)` each frame (default: true).
    ///
    /// When enabled the runner feeds wall-clock delta into the ECS schedule
    /// before calling `AppState::update`.  Disable for manual control.
    pub auto_tick: bool,
    /// How redraws are scheduled (default: [`RedrawMode::Continuous`]).
    pub redraw_mode: RedrawMode,
}

impl AppConfig {
    /// Create a new config with the given title and dimensions.
    pub fn new(title: impl Into<String>, width: u32, height: u32) -> Self {
        Self {
            title: title.into(),
            width,
            height,
            vsync: true,
            resizable: true,
            exit_on_escape: true,
            max_delta: 0.1,
            auto_tick: true,
            redraw_mode: RedrawMode::Continuous,
        }
    }

    /// Set vsync mode.
    #[inline]
    pub fn with_vsync(mut self, vsync: bool) -> Self {
        self.vsync = vsync;
        self
    }

    /// Set window resizability.
    #[inline]
    pub fn with_resizable(mut self, resizable: bool) -> Self {
        self.resizable = resizable;
        self
    }

    /// Set whether pressing Escape exits the application.
    #[inline]
    pub fn with_exit_on_escape(mut self, exit: bool) -> Self {
        self.exit_on_escape = exit;
        self
    }

    /// Set the maximum frame delta used by automatic ticking.
    #[inline]
    pub fn with_max_delta(mut self, max_delta: f32) -> Self {
        self.max_delta = max_delta;
        self
    }

    /// Set whether the runner automatically advances the ECS schedule.
    #[inline]
    pub fn with_auto_tick(mut self, auto_tick: bool) -> Self {
        self.auto_tick = auto_tick;
        self
    }

    /// Set how redraws are scheduled.
    #[inline]
    pub fn with_redraw_mode(mut self, redraw_mode: RedrawMode) -> Self {
        self.redraw_mode = redraw_mode;
        self
    }
}
