/// Configuration for an EUI-NEO-style child window.
#[derive(Clone)]
pub struct NeoWindowConfig {
    pub title: String,
    pub page_id: String,
    pub width: u32,
    pub height: u32,
    pub modal: bool,
    pub clear_color: crate::render::Color,
}

impl NeoWindowConfig {
    pub fn new(title: impl Into<String>, width: u32, height: u32) -> Self {
        let title = title.into();
        Self {
            page_id: title.clone(),
            title,
            width,
            height,
            modal: false,
            clear_color: crate::render::Color::new(0.16, 0.18, 0.20, 1.0),
        }
    }

    pub fn page_id(mut self, value: impl Into<String>) -> Self {
        self.page_id = value.into();
        self
    }

    pub fn modal(mut self, value: bool) -> Self {
        self.modal = value;
        self
    }

    pub fn clear_color(mut self, value: impl Into<crate::render::Color>) -> Self {
        self.clear_color = value.into();
        self
    }
}

/// Configuration for the experimental neo UI backend.
#[derive(Debug, Clone)]
pub struct NeoUiConfig {
    pub page_id: String,
}

impl Default for NeoUiConfig {
    fn default() -> Self {
        Self {
            page_id: "neo".to_string(),
        }
    }
}
