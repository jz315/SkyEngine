//! Application configuration.

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
        }
    }
}
