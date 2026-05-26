//! App-facing capability plugins.
//!
//! These plugins are the configuration surface for app-owned capabilities.
//! They install lightweight declarations/resources into `World`; the app
//! runner later discovers those declarations and owns the heavy backend
//! lifetimes.

use std::path::PathBuf;

use crate::app::config::{RunnerOptions, WindowOptions};
use crate::asset::AssetConfig;
use crate::ecs::World;
use crate::logging::{LogConsole, LogOptions};
use crate::plugin::{Plugin, PluginResult};
use crate::render::RenderPipelineAsset;

pub(crate) struct InputEnabled;

#[cfg(feature = "video")]
pub(crate) struct VideoEnabled;

/// Installs the window capability used by the windowed app runner.
pub struct WindowPlugin {
    options: WindowOptions,
}

impl WindowPlugin {
    pub fn new(title: impl Into<String>, width: u32, height: u32) -> Self {
        Self {
            options: WindowOptions::new(title, width, height),
        }
    }

    pub fn with_options(options: WindowOptions) -> Self {
        Self { options }
    }

    #[inline]
    pub fn with_vsync(mut self, vsync: bool) -> Self {
        self.options.vsync = vsync;
        self
    }

    #[inline]
    pub fn with_resizable(mut self, resizable: bool) -> Self {
        self.options.resizable = resizable;
        self
    }

    #[inline]
    pub fn with_physical_window_size(mut self, physical: bool) -> Self {
        self.options = self.options.with_physical_window_size(physical);
        self
    }
}

impl Plugin for WindowPlugin {
    fn name(&self) -> &'static str {
        "WindowPlugin"
    }

    fn install(self, world: &mut World) -> PluginResult {
        world.insert_resource(self.options);
        Ok(())
    }
}

/// Installs frame pacing, ticking, and runner policy.
#[derive(Default)]
pub struct RunnerPlugin {
    options: RunnerOptions,
}

impl RunnerPlugin {
    #[inline]
    pub fn game() -> Self {
        Self {
            options: RunnerOptions::game(),
        }
    }

    #[inline]
    pub fn reactive() -> Self {
        Self {
            options: RunnerOptions::reactive(),
        }
    }

    #[inline]
    pub fn with_options(options: RunnerOptions) -> Self {
        Self { options }
    }

    #[inline]
    pub fn with_exit_on_escape(mut self, exit: bool) -> Self {
        self.options.exit_on_escape = exit;
        self
    }

    #[inline]
    pub fn with_max_delta(mut self, max_delta: f32) -> Self {
        self.options.max_delta = max_delta;
        self
    }

    #[inline]
    pub fn with_auto_tick(mut self, auto_tick: bool) -> Self {
        self.options.auto_tick = auto_tick;
        self
    }

    #[inline]
    pub fn with_frame_rate_limit(mut self, fps: f64) -> Self {
        self.options.frame_rate_limit = (fps > 0.0).then_some(fps);
        self
    }
}

impl Plugin for RunnerPlugin {
    fn name(&self) -> &'static str {
        "RunnerPlugin"
    }

    fn install(self, world: &mut World) -> PluginResult {
        world.insert_resource(self.options);
        Ok(())
    }
}

/// Installs app-owned log capture and console mirroring options.
#[derive(Default)]
pub struct LogPlugin {
    options: LogOptions,
}

impl LogPlugin {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    pub fn off() -> Self {
        Self {
            options: LogOptions::off(),
        }
    }

    #[inline]
    pub fn with_options(options: LogOptions) -> Self {
        Self { options }
    }

    #[inline]
    pub fn with_level(mut self, level: log::LevelFilter) -> Self {
        self.options.level = level;
        self
    }

    #[inline]
    pub fn with_console(mut self, console: LogConsole) -> Self {
        self.options.console = console;
        self
    }

    #[inline]
    pub fn with_capacity(mut self, capacity: usize) -> Self {
        self.options.capacity = capacity;
        self
    }

    #[inline]
    pub fn with_collapse(mut self, collapse: bool) -> Self {
        self.options.collapse = collapse;
        self
    }
}

impl Plugin for LogPlugin {
    fn name(&self) -> &'static str {
        "LogPlugin"
    }

    fn install(self, world: &mut World) -> PluginResult {
        world.insert_resource(self.options);
        Ok(())
    }
}

/// Enables syncing window input into ECS resources.
#[derive(Default)]
pub struct InputPlugin;

impl Plugin for InputPlugin {
    fn name(&self) -> &'static str {
        "InputPlugin"
    }

    fn install(self, world: &mut World) -> PluginResult {
        world.insert_resource(InputEnabled);
        Ok(())
    }
}

/// Installs the app-owned asset facade capability.
pub struct AssetPlugin {
    config: AssetConfig,
}

impl AssetPlugin {
    pub fn new(asset_root: impl Into<PathBuf>) -> Self {
        Self {
            config: AssetConfig::new(asset_root, AssetConfig::default_target())
                .with_background_loading(true),
        }
    }

    #[inline]
    pub fn from_config(config: AssetConfig) -> Self {
        Self { config }
    }

    #[inline]
    pub fn with_background_loading(mut self, enabled: bool) -> Self {
        self.config.background_loading = enabled;
        self
    }

    #[inline]
    pub fn with_install_budget_per_update(mut self, budget: usize) -> Self {
        self.config.install_budget_per_update = Some(budget);
        self
    }

    #[inline]
    pub fn with_io_worker_threads(mut self, worker_threads: usize) -> Self {
        self.config = self.config.with_io_worker_threads(worker_threads);
        self
    }

    #[inline]
    pub fn with_io_queue_capacity(mut self, queue_capacity: usize) -> Self {
        self.config = self.config.with_io_queue_capacity(queue_capacity);
        self
    }
}

impl Default for AssetPlugin {
    fn default() -> Self {
        Self::from_config(AssetConfig::default().with_background_loading(true))
    }
}

impl Plugin for AssetPlugin {
    fn name(&self) -> &'static str {
        "AssetPlugin"
    }

    fn install(self, world: &mut World) -> PluginResult {
        world.insert_resource(self.config);
        Ok(())
    }
}

/// Installs the render pipeline declaration consumed by the app runner.
pub struct RenderPlugin {
    pipeline: RenderPipelineAsset,
}

impl RenderPlugin {
    #[inline]
    pub fn pipeline(pipeline: RenderPipelineAsset) -> Self {
        Self { pipeline }
    }

    #[inline]
    pub fn forward_2d() -> Self {
        Self::pipeline(RenderPipelineAsset::forward_2d())
    }

    #[inline]
    pub fn forward_3d() -> Self {
        Self::pipeline(RenderPipelineAsset::forward_3d())
    }

    #[inline]
    pub fn modern_3d() -> Self {
        Self::pipeline(RenderPipelineAsset::modern_3d())
    }

    #[cfg(feature = "live2d")]
    #[inline]
    pub fn live2d_2d() -> Self {
        Self::pipeline(RenderPipelineAsset::live2d_2d())
    }

    #[inline]
    pub fn kajiya_3d() -> Self {
        Self::pipeline(RenderPipelineAsset::kajiya_3d())
    }

    #[inline]
    pub fn renderling_3d() -> Self {
        Self::pipeline(RenderPipelineAsset::renderling_3d())
    }
}

impl Plugin for RenderPlugin {
    fn name(&self) -> &'static str {
        "RenderPlugin"
    }

    fn install(self, world: &mut World) -> PluginResult {
        world.insert_resource(self.pipeline);
        Ok(())
    }
}

/// Installs the audio service declaration consumed by the app runner.
#[cfg(feature = "audio")]
pub struct AudioPlugin {
    config: crate::audio::AudioConfig,
}

#[cfg(feature = "audio")]
impl AudioPlugin {
    #[inline]
    pub fn from_config(config: crate::audio::AudioConfig) -> Self {
        Self { config }
    }
}

#[cfg(feature = "audio")]
impl Default for AudioPlugin {
    fn default() -> Self {
        Self::from_config(crate::audio::AudioConfig::default())
    }
}

#[cfg(feature = "audio")]
impl Plugin for AudioPlugin {
    fn name(&self) -> &'static str {
        "AudioPlugin"
    }

    fn install(self, world: &mut World) -> PluginResult {
        world.require_plugin::<AssetPlugin>(self.name())?;
        world.insert_resource(self.config);
        Ok(())
    }
}

/// Installs the video service declaration consumed by the app runner.
#[cfg(feature = "video")]
#[derive(Default)]
pub struct VideoPlugin;

#[cfg(feature = "video")]
impl Plugin for VideoPlugin {
    fn name(&self) -> &'static str {
        "VideoPlugin"
    }

    fn install(self, world: &mut World) -> PluginResult {
        world.require_plugin::<AssetPlugin>(self.name())?;
        world.insert_resource(VideoEnabled);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_plugin_installs_window_options() {
        let mut world = World::new();
        world
            .install(WindowPlugin::new("Plugin Window", 800, 600).with_vsync(false))
            .unwrap();

        let options = world.get_resource::<WindowOptions>().unwrap();
        assert_eq!(options.title, "Plugin Window");
        assert_eq!(options.width, 800);
        assert_eq!(options.height, 600);
        assert!(!options.vsync);
        assert!(world.has_plugin::<WindowPlugin>());
    }

    #[test]
    fn duplicate_plugin_install_is_reported() {
        let mut world = World::new();
        world.install(InputPlugin).unwrap();

        let error = world.install(InputPlugin).unwrap_err();
        assert_eq!(error.plugin, "InputPlugin");
        assert!(error.message.contains("already installed"));
    }

    #[test]
    fn render_plugin_installs_pipeline_declaration() {
        let mut world = World::new();
        world.install(RenderPlugin::forward_2d()).unwrap();
        assert!(world.contains_resource::<RenderPipelineAsset>());
    }

    #[test]
    fn log_plugin_installs_log_options() {
        let mut world = World::new();
        world
            .install(
                LogPlugin::new()
                    .with_level(log::LevelFilter::Debug)
                    .with_console(LogConsole::All)
                    .with_capacity(64),
            )
            .unwrap();

        let options = world.get_resource::<LogOptions>().unwrap();
        assert_eq!(options.level, log::LevelFilter::Debug);
        assert_eq!(options.console, LogConsole::All);
        assert_eq!(options.capacity, 64);
    }
}
