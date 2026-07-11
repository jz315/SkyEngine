#[derive(Debug)]
pub struct TiledImportError(pub(crate) crate::render::features::tilemap::TiledImportError);

impl std::fmt::Display for TiledImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for TiledImportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.0.source()
    }
}

impl From<crate::render::features::tilemap::TiledImportError> for TiledImportError {
    fn from(value: crate::render::features::tilemap::TiledImportError) -> Self {
        Self(value)
    }
}
