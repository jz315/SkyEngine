//! Application configuration.

use crate::diagnostics::DiagnosticConsole;

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
    /// Interpret [`AppConfig::width`] and [`AppConfig::height`] as logical pixels.
    Logical,
    /// Interpret [`AppConfig::width`] and [`AppConfig::height`] as physical pixels.
    Physical,
}

/// Configuration for creating a SkyEngine application window.
pub struct AppConfig {
    /// Window title.
    pub title: String,
    /// Initial window width. See [`AppConfig::size_mode`] for units.
    pub width: u32,
    /// Initial window height. See [`AppConfig::size_mode`] for units.
    pub height: u32,
    /// Unit used for the initial window size.
    pub size_mode: WindowSizeMode,
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
    /// Automatically advance the ECS schedule each frame (default: true).
    ///
    /// When enabled the runner feeds clamped and raw wall-clock deltas into
    /// the ECS schedule before calling `AppState::update`.  Disable for
    /// manual control.
    pub auto_tick: bool,
    /// How redraws are scheduled (default: [`RedrawMode::Continuous`]).
    pub redraw_mode: RedrawMode,
    /// Optional frame-rate cap. `None` means redraw as fast as the platform allows.
    pub frame_rate_limit: Option<f64>,
    /// Which diagnostics are mirrored to stderr by the app runner.
    ///
    /// Diagnostics remain available as structured [`Diagnostics`](crate::diagnostics::Diagnostics)
    /// events regardless of this setting.
    pub diagnostic_console: DiagnosticConsole,
}

impl AppConfig {
    /// Create a new config with the given title and dimensions.
    pub fn new(title: impl Into<String>, width: u32, height: u32) -> Self {
        Self {
            title: title.into(),
            width,
            height,
            size_mode: WindowSizeMode::Logical,
            vsync: true,
            resizable: true,
            exit_on_escape: true,
            max_delta: 0.1,
            auto_tick: true,
            redraw_mode: RedrawMode::Continuous,
            frame_rate_limit: None,
            diagnostic_console: DiagnosticConsole::default(),
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

    /// Set whether the initial window size is interpreted as physical pixels.
    #[inline]
    pub fn with_physical_window_size(mut self, physical: bool) -> Self {
        self.size_mode = if physical {
            WindowSizeMode::Physical
        } else {
            WindowSizeMode::Logical
        };
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

    /// Set an application frame-rate cap. Values at or below zero disable the cap.
    #[inline]
    pub fn with_frame_rate_limit(mut self, fps: f64) -> Self {
        self.frame_rate_limit = (fps > 0.0).then_some(fps);
        self
    }

    /// Set which diagnostics are mirrored to stderr by the app runner.
    #[inline]
    pub fn with_diagnostic_console(mut self, diagnostic_console: DiagnosticConsole) -> Self {
        self.diagnostic_console = diagnostic_console;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_config_defaults_to_warning_and_error_diagnostics() {
        let config = AppConfig::new("test", 64, 64);
        assert_eq!(
            config.diagnostic_console,
            DiagnosticConsole::WarningsAndErrors
        );
    }

    #[test]
    fn app_config_can_disable_diagnostic_console() {
        let config = AppConfig::new("test", 64, 64).with_diagnostic_console(DiagnosticConsole::Off);
        assert_eq!(config.diagnostic_console, DiagnosticConsole::Off);
    }
}
