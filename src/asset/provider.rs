use std::path::{Path, PathBuf};

use super::types::{AssetConfig, AssetError, AssetId, AssetManifestEntry};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AssetSourceLocation {
    Raw(PathBuf),
    Cooked(PathBuf),
}

impl AssetSourceLocation {
    pub(crate) fn path(&self) -> &Path {
        match self {
            Self::Raw(path) | Self::Cooked(path) => path,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ResolvedAssetSource {
    entry: AssetManifestEntry,
    location: AssetSourceLocation,
}

impl ResolvedAssetSource {
    pub(crate) fn new(entry: AssetManifestEntry, location: AssetSourceLocation) -> Self {
        Self { entry, location }
    }

    pub(crate) fn entry(&self) -> &AssetManifestEntry {
        &self.entry
    }

    pub(crate) fn location(&self) -> &AssetSourceLocation {
        &self.location
    }

    pub(crate) fn read_bytes(&self, id: AssetId) -> Result<Vec<u8>, AssetError> {
        std::fs::read(self.location.path()).map_err(|error| match &self.location {
            AssetSourceLocation::Raw(path) => AssetError::Io {
                path: path.clone(),
                message: error.to_string(),
            },
            AssetSourceLocation::Cooked(path) if error.kind() == std::io::ErrorKind::NotFound => {
                AssetError::MissingCookedArtifact {
                    id,
                    path: path.clone(),
                }
            }
            AssetSourceLocation::Cooked(path) => AssetError::Io {
                path: path.clone(),
                message: error.to_string(),
            },
        })
    }
}

pub(crate) trait AssetProvider: Send + Sync {
    fn resolve(
        &self,
        id: AssetId,
        entry: AssetManifestEntry,
        raw_source_path: Option<PathBuf>,
    ) -> Result<ResolvedAssetSource, AssetError>;
}

#[derive(Clone, Debug)]
pub(crate) struct LocalAssetProvider {
    cooked_root: PathBuf,
}

impl LocalAssetProvider {
    pub(crate) fn new(config: &AssetConfig) -> Self {
        Self {
            cooked_root: config.cooked_root(),
        }
    }
}

impl AssetProvider for LocalAssetProvider {
    fn resolve(
        &self,
        _id: AssetId,
        entry: AssetManifestEntry,
        raw_source_path: Option<PathBuf>,
    ) -> Result<ResolvedAssetSource, AssetError> {
        let location = match raw_source_path {
            Some(path) => AssetSourceLocation::Raw(path),
            None => AssetSourceLocation::Cooked(self.cooked_root.join(&entry.cooked_path)),
        };
        Ok(ResolvedAssetSource::new(entry, location))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset::{AssetConfig, ASSET_SYSTEM_VERSION};
    use tempfile::tempdir;

    fn entry(id: AssetId) -> AssetManifestEntry {
        AssetManifestEntry {
            asset_id: id,
            asset_type: "dummy".to_string(),
            importer: "dummy".to_string(),
            cooker: "dummy".to_string(),
            version: ASSET_SYSTEM_VERSION,
            source_path: "clip.dummy".to_string(),
            cooked_path: "clip.dummyc".to_string(),
            dependencies: Vec::new(),
            import_settings: serde_json::Value::Null,
        }
    }

    #[test]
    fn local_provider_resolves_cooked_paths_under_cooked_root() {
        let dir = tempdir().expect("temporary asset root");
        let config = AssetConfig::new(dir.path(), "native");
        let provider = LocalAssetProvider::new(&config);
        let id = AssetId::new();

        let source = provider
            .resolve(id, entry(id), None)
            .expect("source should resolve");

        assert_eq!(
            source.location(),
            &AssetSourceLocation::Cooked(config.cooked_root().join("clip.dummyc"))
        );
    }

    #[test]
    fn local_provider_prefers_raw_source_path() {
        let dir = tempdir().expect("temporary asset root");
        let config = AssetConfig::new(dir.path(), "native");
        let provider = LocalAssetProvider::new(&config);
        let id = AssetId::new();
        let raw = dir.path().join("clip.dummy");

        let source = provider
            .resolve(id, entry(id), Some(raw.clone()))
            .expect("source should resolve");

        assert_eq!(source.location(), &AssetSourceLocation::Raw(raw));
    }
}
