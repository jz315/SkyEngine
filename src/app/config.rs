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
    /// Automatically advance the ECS schedule each frame (default: true).
    ///
    /// When enabled the runner feeds clamped and raw wall-clock deltas into
    /// the ECS schedule before calling `AppState::update`.  Disable for
    /// manual control.
    pub auto_tick: bool,
    /// How redraws are scheduled (default: [`RedrawMode::Continuous`]).
    pub redraw_mode: RedrawMode,
    /// Which diagnostics are mirrored to stderr by the app runner.
    ///
    /// Diagnostics remain available as structured [`Diagnostics`](crate::diagnostics::Diagnostics)
    /// events regardless of this setting.
    pub diagnostic_console: DiagnosticConsole,
    /// Native retained UI configuration.
    ///
    /// This is used by `FrameContext::update_ui` / `render_ui` when the UI
    /// runtime installs its resources lazily.
    #[cfg(feature = "ui")]
    pub ui: crate::ui::UiConfig,
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
            diagnostic_console: DiagnosticConsole::default(),
            #[cfg(feature = "ui")]
            ui: crate::ui::UiConfig::default(),
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

    /// Set which diagnostics are mirrored to stderr by the app runner.
    #[inline]
    pub fn with_diagnostic_console(mut self, diagnostic_console: DiagnosticConsole) -> Self {
        self.diagnostic_console = diagnostic_console;
        self
    }

    /// Set the native retained UI configuration.
    ///
    /// Requires `--features ui`.
    #[cfg(feature = "ui")]
    #[inline]
    pub fn with_ui_config(mut self, ui: crate::ui::UiConfig) -> Self {
        self.ui = ui;
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

    #[cfg(feature = "ui")]
    #[test]
    fn app_config_can_customize_ui_config() {
        let config = AppConfig::new("test", 64, 64).with_ui_config(crate::ui::UiConfig {
            load_system_fonts: false,
        });

        assert!(!config.ui.load_system_fonts);
    }
}
