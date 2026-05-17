/// Public font source descriptor. The glyph renderer consumes these without
/// exposing glyphon types through SkyEngine's API.
#[derive(Debug, Clone)]
pub enum UiFontSource {
    SystemFonts,
    Bytes { name: String, bytes: Vec<u8> },
}

/// Font registry owned by the UI system.
#[derive(Debug, Clone)]
pub struct UiFontBook {
    sources: Vec<UiFontSource>,
    revision: u64,
    diagnostics: Vec<String>,
}

impl UiFontBook {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn empty() -> Self {
        Self {
            sources: Vec::new(),
            revision: 0,
            diagnostics: Vec::new(),
        }
    }

    pub fn load_system_fonts(&mut self) -> &mut Self {
        if !self
            .sources
            .iter()
            .any(|source| matches!(source, UiFontSource::SystemFonts))
        {
            self.sources.push(UiFontSource::SystemFonts);
            self.revision = self.revision.wrapping_add(1);
        }
        self
    }

    pub fn add_font_bytes(
        &mut self,
        name: impl Into<String>,
        bytes: impl Into<Vec<u8>>,
    ) -> &mut Self {
        self.sources.push(UiFontSource::Bytes {
            name: name.into(),
            bytes: bytes.into(),
        });
        self.revision = self.revision.wrapping_add(1);
        self
    }

    pub fn sources(&self) -> &[UiFontSource] {
        &self.sources
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn diagnostics(&self) -> &[String] {
        &self.diagnostics
    }
}

impl Default for UiFontBook {
    fn default() -> Self {
        Self {
            sources: vec![UiFontSource::SystemFonts],
            revision: 1,
            diagnostics: Vec::new(),
        }
    }
}

pub(crate) fn preferred_text_size(text: &str, font_size: f32) -> [f32; 2] {
    let mut max_chars = 0usize;
    let mut lines = 0usize;
    for line in text.lines() {
        max_chars = max_chars.max(line.chars().count());
        lines += 1;
    }
    if lines == 0 {
        lines = 1;
    }
    [
        max_chars as f32 * font_size * 0.56,
        lines as f32 * font_size * 1.25,
    ]
}

#[cfg(test)]
mod tests {
    use super::preferred_text_size;

    #[test]
    fn preferred_size_without_fonts_does_not_panic() {
        let size = preferred_text_size("Hello\nSky", 20.0);
        assert!(size[0] > 0.0);
        assert!(size[1] > 20.0);
    }
}
