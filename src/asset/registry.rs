use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::Arc;

use super::types::{
    normalize_source_key, Asset, AssetError, AssetId, AssetInstallContext, AssetLoadContext,
    AssetManifestEntry, AssetRegistryManifest, LoadedAsset,
};

pub trait AssetRuntimeFactory: Send + Sync + 'static {
    type Asset: Asset;
    type Loaded: Send + Sync + 'static;

    fn asset_type(&self) -> &'static str {
        <Self::Asset as Asset>::TYPE
    }

    fn load(&self, ctx: AssetLoadContext<'_>) -> Result<LoadedAsset<Self::Loaded>, AssetError>;

    fn install(
        &self,
        loaded: &Self::Loaded,
        ctx: AssetInstallContext<'_>,
    ) -> Result<Self::Asset, AssetError>;
}

pub(crate) trait ErasedAssetFactory: Send + Sync {
    fn asset_type(&self) -> &'static str;
    fn product_type_id(&self) -> TypeId;
    fn load(
        &self,
        ctx: AssetLoadContext<'_>,
    ) -> Result<LoadedAsset<Arc<dyn Any + Send + Sync>>, AssetError>;
    fn install(
        &self,
        loaded: &Arc<dyn Any + Send + Sync>,
        ctx: AssetInstallContext<'_>,
    ) -> Result<Arc<dyn Any + Send + Sync>, AssetError>;
}

pub(crate) struct FactoryAdapter<F>(pub F);

impl<F> ErasedAssetFactory for FactoryAdapter<F>
where
    F: AssetRuntimeFactory,
{
    fn asset_type(&self) -> &'static str {
        self.0.asset_type()
    }

    fn product_type_id(&self) -> TypeId {
        TypeId::of::<F::Asset>()
    }

    fn load(
        &self,
        ctx: AssetLoadContext<'_>,
    ) -> Result<LoadedAsset<Arc<dyn Any + Send + Sync>>, AssetError> {
        let loaded = self.0.load(ctx)?;
        Ok(LoadedAsset {
            loaded: Arc::new(loaded.loaded),
            dependencies: loaded.dependencies,
        })
    }

    fn install(
        &self,
        loaded: &Arc<dyn Any + Send + Sync>,
        ctx: AssetInstallContext<'_>,
    ) -> Result<Arc<dyn Any + Send + Sync>, AssetError> {
        let typed = loaded
            .downcast_ref::<F::Loaded>()
            .ok_or_else(|| AssetError::Internal {
                message: format!(
                    "factory `{}` received unexpected loaded payload type",
                    self.asset_type()
                ),
            })?;
        let asset = self.0.install(typed, ctx)?;
        Ok(Arc::new(asset))
    }
}

pub(crate) struct ManifestIndex {
    pub manifest: AssetRegistryManifest,
    by_id: HashMap<AssetId, usize>,
    pub source_to_id: HashMap<String, AssetId>,
}

impl ManifestIndex {
    pub fn new(manifest: AssetRegistryManifest) -> Self {
        let mut by_id = HashMap::default();
        let mut source_to_id = HashMap::default();

        for (index, entry) in manifest.assets.iter().enumerate() {
            by_id.insert(entry.asset_id, index);
            source_to_id.insert(normalize_source_key(&entry.source_path), entry.asset_id);
        }

        Self {
            manifest,
            by_id,
            source_to_id,
        }
    }

    pub fn entry(&self, id: AssetId) -> Option<&AssetManifestEntry> {
        self.by_id
            .get(&id)
            .map(|index| &self.manifest.assets[*index])
    }
}
