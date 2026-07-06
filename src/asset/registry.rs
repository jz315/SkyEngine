use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use super::events::AssetEventLog;
use super::failure;
use super::install::{
    AssetInstallBudget, AssetInstallContext, AssetInstallPoll, AssetInstallResult,
    AssetInstallTask, AssetUninstallContext,
};
use super::request::AssetRequests;
use super::store::AssetStore;
use super::types::{
    normalize_source_key, Asset, AssetConfig, AssetCookedSchema, AssetError, AssetFailurePhase,
    AssetId, AssetLoadContext, AssetManifestEntry, AssetMetadata, AssetPath, AssetRegistryManifest,
    AssetWatchPaths, LoadedAsset, ASSET_SYSTEM_VERSION,
};

pub trait AssetRuntimeFactory: Send + Sync + 'static {
    type Asset: Asset;
    type Loaded: Send + Sync + 'static;

    fn asset_type(&self) -> &'static str {
        <Self::Asset as Asset>::TYPE
    }

    fn cooked_schema(&self) -> Option<AssetCookedSchema> {
        None
    }

    fn load(&self, ctx: AssetLoadContext<'_>) -> Result<LoadedAsset<Self::Loaded>, AssetError>;

    fn begin_install(
        &self,
        loaded: &Self::Loaded,
        ctx: AssetInstallContext<'_>,
    ) -> Result<AssetInstallResult<Self::Asset>, AssetError>;

    fn uninstall(
        &self,
        _installed: &Self::Asset,
        _ctx: AssetUninstallContext<'_>,
    ) -> Result<(), AssetError> {
        Ok(())
    }
}

pub(crate) trait ErasedAssetFactory: Send + Sync {
    fn asset_type(&self) -> &'static str;
    fn product_type_id(&self) -> TypeId;
    fn cooked_schema(&self) -> Option<AssetCookedSchema> {
        None
    }
    fn load(
        &self,
        ctx: AssetLoadContext<'_>,
    ) -> Result<LoadedAsset<Arc<dyn Any + Send + Sync>>, AssetError>;
    fn begin_install(
        &self,
        loaded: &Arc<dyn Any + Send + Sync>,
        ctx: AssetInstallContext<'_>,
    ) -> Result<AssetInstallResult<Arc<dyn Any + Send + Sync>>, AssetError>;
    fn uninstall(
        &self,
        _installed: &Arc<dyn Any + Send + Sync>,
        _ctx: AssetUninstallContext<'_>,
    ) -> Result<(), AssetError> {
        Ok(())
    }
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

    fn cooked_schema(&self) -> Option<AssetCookedSchema> {
        self.0.cooked_schema()
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

    fn begin_install(
        &self,
        loaded: &Arc<dyn Any + Send + Sync>,
        ctx: AssetInstallContext<'_>,
    ) -> Result<AssetInstallResult<Arc<dyn Any + Send + Sync>>, AssetError> {
        let typed = loaded
            .downcast_ref::<F::Loaded>()
            .ok_or_else(|| AssetError::Internal {
                message: format!(
                    "factory `{}` received unexpected loaded payload type",
                    self.asset_type()
                ),
            })?;
        match self.0.begin_install(typed, ctx)? {
            AssetInstallResult::Ready(asset) => Ok(AssetInstallResult::Ready(Arc::new(asset))),
            AssetInstallResult::Pending(task) => {
                Ok(AssetInstallResult::Pending(Box::new(ErasedInstallTask {
                    inner: task,
                })))
            }
        }
    }

    fn uninstall(
        &self,
        installed: &Arc<dyn Any + Send + Sync>,
        ctx: AssetUninstallContext<'_>,
    ) -> Result<(), AssetError> {
        let typed = installed
            .downcast_ref::<F::Asset>()
            .ok_or_else(|| AssetError::Internal {
                message: format!(
                    "factory `{}` received unexpected installed payload type",
                    self.asset_type()
                ),
            })?;
        self.0.uninstall(typed, ctx)
    }
}

#[derive(Default)]
pub(crate) struct AssetFactories {
    factories: HashMap<String, Arc<dyn ErasedAssetFactory>>,
}

impl AssetFactories {
    pub(crate) fn register<F>(&mut self, factory: F)
    where
        F: AssetRuntimeFactory,
    {
        self.factories.insert(
            factory.asset_type().to_string(),
            Arc::new(FactoryAdapter(factory)),
        );
    }

    pub(crate) fn get(&self, asset_type: &str) -> Result<Arc<dyn ErasedAssetFactory>, AssetError> {
        self.factories
            .get(asset_type)
            .cloned()
            .ok_or_else(|| AssetError::FactoryNotRegistered {
                asset_type: asset_type.to_string(),
            })
    }

    pub(crate) fn for_entry(
        &self,
        entry: &AssetManifestEntry,
    ) -> Result<Arc<dyn ErasedAssetFactory>, AssetError> {
        self.get(&entry.asset_type)
    }

    pub(crate) fn validate_entry_product<T: Asset>(
        &self,
        id: AssetId,
        entry: &AssetManifestEntry,
    ) -> Result<(), AssetError> {
        let factory = self.for_entry(entry)?;
        if factory.product_type_id() != TypeId::of::<T>() {
            return Err(AssetError::AssetTypeMismatch {
                id,
                expected: T::TYPE,
                actual: entry.asset_type.clone(),
            });
        }
        Ok(())
    }

    pub(crate) fn ensure_registered_product<T: Asset>(&self) -> Result<(), AssetError> {
        let factory = self.get(T::TYPE)?;
        if factory.product_type_id() != TypeId::of::<T>() {
            return Err(AssetError::Internal {
                message: format!(
                    "registered `{}` factory product type does not match requested asset type",
                    T::TYPE
                ),
            });
        }
        Ok(())
    }
}

struct ErasedInstallTask<T: Asset> {
    inner: Box<dyn AssetInstallTask<Output = T>>,
}

impl<T: Asset> AssetInstallTask for ErasedInstallTask<T> {
    type Output = Arc<dyn Any + Send + Sync>;

    fn progress(&self) -> Option<super::types::AssetRequestProgress> {
        self.inner.progress()
    }

    fn poll_install(
        &mut self,
        ctx: AssetInstallContext<'_>,
        budget: AssetInstallBudget,
    ) -> Result<AssetInstallPoll<Self::Output>, AssetError> {
        match self.inner.poll_install(ctx, budget)? {
            AssetInstallPoll::Pending => Ok(AssetInstallPoll::Pending),
            AssetInstallPoll::Ready(asset) => Ok(AssetInstallPoll::Ready(Arc::new(asset))),
        }
    }
}

pub(crate) struct ManifestIndex {
    manifest: AssetRegistryManifest,
    by_id: HashMap<AssetId, usize>,
    source_to_id: HashMap<String, AssetId>,
    cooked_to_id: HashMap<String, AssetId>,
}

pub(crate) type LocalManifestRegistry = ManifestIndex;

pub(crate) trait AssetRegistry {
    fn entry(&self, id: AssetId) -> Option<&AssetManifestEntry>;

    fn metadata(&self, id: AssetId) -> Option<AssetMetadata> {
        self.entry(id).map(AssetMetadata::from)
    }

    fn source_path(&self, id: AssetId) -> Option<PathBuf> {
        self.entry(id)
            .map(|entry| PathBuf::from(&entry.source_path))
    }

    fn asset_type(&self, id: AssetId) -> Option<String> {
        self.metadata(id).map(|metadata| metadata.asset_type)
    }

    fn dependencies(&self, id: AssetId) -> Option<&[AssetId]> {
        self.entry(id).map(|entry| entry.dependencies.as_slice())
    }

    fn resolve_location(&self, config: &AssetConfig, id: AssetId) -> Option<AssetWatchPaths> {
        let entry = self.entry(id)?;
        Some(AssetWatchPaths {
            source_path: config.asset_root.join(&entry.source_path),
            cooked_path: config.cooked_root().join(&entry.cooked_path),
            package_paths: config
                .package_roots()
                .into_iter()
                .map(|root| root.join(&entry.cooked_path))
                .collect(),
            package_files: config.package_files(),
        })
    }

    fn lookup_source_asset(&self, _config: &AssetConfig, _path: &Path) -> Option<AssetId> {
        None
    }

    fn lookup_cooked_asset(&self, _config: &AssetConfig, _path: &Path) -> Option<AssetId> {
        None
    }

    fn lookup_package_asset(&self, _config: &AssetConfig, _path: &Path) -> Option<AssetId> {
        None
    }

    fn lookup_watch_asset(&self, config: &AssetConfig, path: &Path) -> Option<AssetId> {
        self.lookup_source_asset(config, path)
            .or_else(|| self.lookup_cooked_asset(config, path))
            .or_else(|| self.lookup_package_asset(config, path))
    }

    #[allow(dead_code)]
    fn watch_key(&self, config: &AssetConfig, id: AssetId) -> Option<AssetWatchPaths> {
        self.resolve_location(config, id)
    }
}

pub(crate) fn validate_typed_asset_request<T, R>(
    registry: &R,
    factories: &AssetFactories,
    id: AssetId,
) -> Result<(), AssetError>
where
    T: Asset,
    R: AssetRegistry + ?Sized,
{
    let entry = registry.entry(id).ok_or(AssetError::AssetNotFound { id })?;
    factories.validate_entry_product::<T>(id, entry)
}

pub(crate) fn resolve_typed_source_asset<T, R>(
    config: &AssetConfig,
    registry: &R,
    factories: &AssetFactories,
    path: &Path,
) -> Result<AssetId, AssetError>
where
    T: Asset,
    R: AssetRegistry + ?Sized,
{
    let id = registry.lookup_source_asset(config, path).ok_or_else(|| {
        AssetError::AssetPathNotFound {
            path: path.to_path_buf(),
        }
    })?;
    validate_typed_asset_request::<T, R>(registry, factories, id)?;
    Ok(id)
}

pub(crate) fn typed_asset_path<T, R>(
    registry: &R,
    factories: &AssetFactories,
    id: AssetId,
) -> Result<AssetPath<T>, AssetError>
where
    T: Asset,
    R: AssetRegistry + ?Sized,
{
    validate_typed_asset_request::<T, R>(registry, factories, id)?;
    let source_path = registry
        .source_path(id)
        .ok_or(AssetError::AssetNotFound { id })?;
    Ok(AssetPath::new(source_path))
}

pub(crate) trait AssetRegistryLoader: Send + Sync {
    fn load(&self, config: &AssetConfig) -> Result<AssetRegistryManifest, AssetError>;
}

#[derive(Debug, Default)]
pub(crate) struct LocalManifestRegistryLoader;

impl AssetRegistryLoader for LocalManifestRegistryLoader {
    fn load(&self, config: &AssetConfig) -> Result<AssetRegistryManifest, AssetError> {
        load_local_manifest(config)
    }
}

impl ManifestIndex {
    pub fn new(manifest: AssetRegistryManifest) -> Self {
        let mut by_id = HashMap::default();
        let mut source_to_id = HashMap::default();
        let mut cooked_to_id = HashMap::default();

        for (index, entry) in manifest.assets.iter().enumerate() {
            by_id.insert(entry.asset_id, index);
            source_to_id.insert(normalize_source_key(&entry.source_path), entry.asset_id);
            cooked_to_id.insert(normalize_source_key(&entry.cooked_path), entry.asset_id);
        }

        Self {
            manifest,
            by_id,
            source_to_id,
            cooked_to_id,
        }
    }

    pub fn entry(&self, id: AssetId) -> Option<&AssetManifestEntry> {
        self.by_id
            .get(&id)
            .map(|index| &self.manifest.assets[*index])
    }

    pub(crate) fn manifest_snapshot(&self) -> AssetRegistryManifest {
        self.manifest.clone()
    }

    pub(crate) fn lookup_source_asset(&self, config: &AssetConfig, path: &Path) -> Option<AssetId> {
        let key = config.source_key(path);
        self.source_to_id.get(&key).copied()
    }

    pub(crate) fn lookup_cooked_asset(&self, config: &AssetConfig, path: &Path) -> Option<AssetId> {
        let cooked_root = config.cooked_root();
        let relative = if path.is_absolute() {
            path.strip_prefix(&cooked_root).ok()?
        } else {
            path
        };
        self.lookup_cooked_key(relative)
    }

    pub(crate) fn lookup_package_asset(
        &self,
        config: &AssetConfig,
        path: &Path,
    ) -> Option<AssetId> {
        if !path.is_absolute() {
            return self.lookup_cooked_key(path);
        }

        config.package_roots().into_iter().find_map(|root| {
            path.strip_prefix(root)
                .ok()
                .and_then(|relative| self.lookup_cooked_key(relative))
        })
    }

    fn lookup_cooked_key(&self, path: &Path) -> Option<AssetId> {
        let key = normalize_source_key(&path.to_string_lossy());
        self.cooked_to_id.get(&key).copied()
    }
}

pub(crate) fn manifest_snapshot(registry: &LocalManifestRegistry) -> AssetRegistryManifest {
    registry.manifest_snapshot()
}

pub(crate) fn lookup_source_asset(
    config: &AssetConfig,
    registry: &impl AssetRegistry,
    path: &Path,
) -> Option<AssetId> {
    registry.lookup_source_asset(config, path)
}

pub(crate) fn source_path(registry: &impl AssetRegistry, id: AssetId) -> Option<PathBuf> {
    registry.source_path(id)
}

pub(crate) fn metadata(registry: &impl AssetRegistry, id: AssetId) -> Option<AssetMetadata> {
    registry.metadata(id)
}

pub(crate) fn watch_paths(
    config: &AssetConfig,
    registry: &impl AssetRegistry,
    id: AssetId,
) -> Option<AssetWatchPaths> {
    registry.resolve_location(config, id)
}

impl AssetRegistry for ManifestIndex {
    fn entry(&self, id: AssetId) -> Option<&AssetManifestEntry> {
        ManifestIndex::entry(self, id)
    }

    fn lookup_source_asset(&self, config: &AssetConfig, path: &Path) -> Option<AssetId> {
        ManifestIndex::lookup_source_asset(self, config, path)
    }

    fn lookup_cooked_asset(&self, config: &AssetConfig, path: &Path) -> Option<AssetId> {
        ManifestIndex::lookup_cooked_asset(self, config, path)
    }

    fn lookup_package_asset(&self, config: &AssetConfig, path: &Path) -> Option<AssetId> {
        ManifestIndex::lookup_package_asset(self, config, path)
    }
}

#[derive(Default)]
pub(crate) struct ManifestRefresh {
    pub(crate) missing_records: Vec<AssetId>,
}

pub(crate) fn refresh_manifest_records(
    manifest: &mut LocalManifestRegistry,
    store: &mut AssetStore,
    next_manifest: AssetRegistryManifest,
) -> ManifestRefresh {
    *manifest = LocalManifestRegistry::new(next_manifest);

    let mut refresh = ManifestRefresh::default();
    for id in store.manifest_refresh_record_ids() {
        if let Some(entry) = manifest.entry(id) {
            store.set_manifest_record_asset_type(id, entry.asset_type.clone());
        } else {
            refresh.missing_records.push(id);
        }
    }

    refresh
}

pub(crate) fn refresh_manifest_records_or_fail(
    manifest: &mut LocalManifestRegistry,
    store: &mut AssetStore,
    events: &mut AssetEventLog,
    requests: &mut AssetRequests,
    next_manifest: AssetRegistryManifest,
    dependency_priority: i32,
    failed_at: Instant,
) -> ManifestRefresh {
    let refresh = refresh_manifest_records(manifest, store, next_manifest);
    for id in &refresh.missing_records {
        failure::fail_record_and_request(
            store,
            events,
            requests,
            *id,
            AssetError::AssetNotFound { id: *id },
            AssetFailurePhase::Lookup,
            dependency_priority,
            failed_at,
            |dependency| manifest.asset_type(dependency),
        );
    }
    refresh
}

#[cfg(test)]
pub(crate) fn load_manifest(config: &AssetConfig) -> Result<AssetRegistryManifest, AssetError> {
    LocalManifestRegistryLoader.load(config)
}

fn load_local_manifest(config: &AssetConfig) -> Result<AssetRegistryManifest, AssetError> {
    let manifest_path = config.manifest_path();
    if !manifest_path.exists() {
        return if config.cooked_root().exists() {
            Err(AssetError::ManifestMissing {
                path: manifest_path,
            })
        } else {
            Ok(AssetRegistryManifest {
                version: ASSET_SYSTEM_VERSION,
                target: config.target.clone(),
                provenance: Vec::new(),
                assets: Vec::new(),
            })
        };
    }

    let bytes = std::fs::read(&manifest_path).map_err(|error| AssetError::Io {
        path: manifest_path.clone(),
        message: error.to_string(),
    })?;
    let manifest: AssetRegistryManifest =
        serde_json::from_slice(&bytes).map_err(|error| AssetError::Json {
            path: manifest_path.clone(),
            message: error.to_string(),
        })?;

    if manifest.version != ASSET_SYSTEM_VERSION {
        return Err(AssetError::VersionMismatch {
            path: manifest_path,
            expected: ASSET_SYSTEM_VERSION,
            actual: manifest.version,
        });
    }

    Ok(manifest)
}
