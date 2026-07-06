use std::path::Path;

use super::{AssetId, AssetManifestEntry};
pub struct AssetLoadContext<'a> {
    pub asset_id: AssetId,
    pub entry: &'a AssetManifestEntry,
    pub bytes: &'a [u8],
    pub asset_root: &'a Path,
    pub cooked_root: &'a Path,
}

pub struct LoadedAsset<T> {
    pub loaded: T,
    pub dependencies: Vec<AssetId>,
}

impl<T> LoadedAsset<T> {
    #[must_use]
    pub fn new(loaded: T) -> Self {
        Self {
            loaded,
            dependencies: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_dependencies(mut self, dependencies: impl Into<Vec<AssetId>>) -> Self {
        self.dependencies = dependencies.into();
        self
    }
}
