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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use crate::asset::request::AssetRequestPhase;
    use crate::asset::store::AssetRecord;
    use crate::asset::types::{AssetEventKind, AssetRequestStatus, AssetState};
    use tempfile::tempdir;

    struct RegistryDummyAsset;

    impl Asset for RegistryDummyAsset {
        const TYPE: &'static str = "registry.dummy";
    }

    struct RegistryOtherAsset;

    impl Asset for RegistryOtherAsset {
        const TYPE: &'static str = "registry.other";
    }

    struct RegistryDummyFactory;

    impl AssetRuntimeFactory for RegistryDummyFactory {
        type Asset = RegistryDummyAsset;
        type Loaded = ();

        fn load(
            &self,
            _ctx: AssetLoadContext<'_>,
        ) -> Result<LoadedAsset<Self::Loaded>, AssetError> {
            Ok(LoadedAsset::new(()))
        }

        fn begin_install(
            &self,
            _loaded: &Self::Loaded,
            _ctx: AssetInstallContext<'_>,
        ) -> Result<AssetInstallResult<Self::Asset>, AssetError> {
            Ok(AssetInstallResult::Ready(RegistryDummyAsset))
        }
    }

    struct RegistryOtherFactory;

    impl AssetRuntimeFactory for RegistryOtherFactory {
        type Asset = RegistryOtherAsset;
        type Loaded = ();

        fn load(
            &self,
            _ctx: AssetLoadContext<'_>,
        ) -> Result<LoadedAsset<Self::Loaded>, AssetError> {
            Ok(LoadedAsset::new(()))
        }

        fn begin_install(
            &self,
            _loaded: &Self::Loaded,
            _ctx: AssetInstallContext<'_>,
        ) -> Result<AssetInstallResult<Self::Asset>, AssetError> {
            Ok(AssetInstallResult::Ready(RegistryOtherAsset))
        }
    }

    struct RegistryLyingFactory;

    impl AssetRuntimeFactory for RegistryLyingFactory {
        type Asset = RegistryOtherAsset;
        type Loaded = ();

        fn asset_type(&self) -> &'static str {
            RegistryDummyAsset::TYPE
        }

        fn load(
            &self,
            _ctx: AssetLoadContext<'_>,
        ) -> Result<LoadedAsset<Self::Loaded>, AssetError> {
            Ok(LoadedAsset::new(()))
        }

        fn begin_install(
            &self,
            _loaded: &Self::Loaded,
            _ctx: AssetInstallContext<'_>,
        ) -> Result<AssetInstallResult<Self::Asset>, AssetError> {
            Ok(AssetInstallResult::Ready(RegistryOtherAsset))
        }
    }

    fn test_entry(id: AssetId, asset_type: &'static str) -> AssetManifestEntry {
        AssetManifestEntry {
            asset_id: id,
            asset_type: asset_type.to_string(),
            importer: "registry.importer".to_string(),
            cooker: "registry.cooker".to_string(),
            version: 1,
            source_path: format!("{id}.source"),
            cooked_path: format!("{id}.cooked"),
            dependencies: Vec::new(),
            import_settings: serde_json::Value::Null,
        }
    }

    fn manifest(entries: Vec<AssetManifestEntry>) -> AssetRegistryManifest {
        AssetRegistryManifest {
            version: crate::asset::ASSET_SYSTEM_VERSION,
            target: "native".to_string(),
            provenance: Vec::new(),
            assets: entries,
        }
    }

    #[test]
    fn asset_factories_register_and_lookup_entry_factory() {
        let id = AssetId::new();
        let entry = test_entry(id, RegistryDummyAsset::TYPE);
        let mut factories = AssetFactories::default();

        factories.register(RegistryDummyFactory);

        let factory = factories
            .for_entry(&entry)
            .expect("dummy factory should be registered");
        assert_eq!(factory.asset_type(), RegistryDummyAsset::TYPE);
        assert_eq!(
            factory.product_type_id(),
            TypeId::of::<RegistryDummyAsset>()
        );
        factories
            .validate_entry_product::<RegistryDummyAsset>(id, &entry)
            .expect("entry product type should match typed request");
        factories
            .ensure_registered_product::<RegistryDummyAsset>()
            .expect("registered product type should match asset type");
    }

    #[test]
    fn asset_factories_report_missing_registered_asset_type() {
        let id = AssetId::new();
        let entry = test_entry(id, RegistryDummyAsset::TYPE);
        let factories = AssetFactories::default();

        match factories.for_entry(&entry) {
            Err(AssetError::FactoryNotRegistered { asset_type }) => {
                assert_eq!(asset_type, RegistryDummyAsset::TYPE);
            }
            Ok(_) => panic!("missing factory should not resolve"),
            Err(error) => panic!("unexpected error: {error}"),
        }

        match factories.ensure_registered_product::<RegistryDummyAsset>() {
            Err(AssetError::FactoryNotRegistered { asset_type }) => {
                assert_eq!(asset_type, RegistryDummyAsset::TYPE);
            }
            Ok(_) => panic!("missing factory should not validate"),
            Err(error) => panic!("unexpected error: {error}"),
        }
    }

    #[test]
    fn asset_factories_validate_entry_product_reports_type_mismatch() {
        let id = AssetId::new();
        let entry = test_entry(id, RegistryOtherAsset::TYPE);
        let mut factories = AssetFactories::default();

        factories.register(RegistryOtherFactory);

        match factories.validate_entry_product::<RegistryDummyAsset>(id, &entry) {
            Err(AssetError::AssetTypeMismatch {
                id: error_id,
                expected,
                actual,
            }) => {
                assert_eq!(error_id, id);
                assert_eq!(expected, RegistryDummyAsset::TYPE);
                assert_eq!(actual, RegistryOtherAsset::TYPE);
            }
            Ok(_) => panic!("mismatched product type should not validate"),
            Err(error) => panic!("unexpected error: {error}"),
        }
    }

    #[test]
    fn registry_validates_typed_request_from_manifest_entry() {
        let id = AssetId::new();
        let entry = test_entry(id, RegistryDummyAsset::TYPE);
        let registry = ManifestIndex::new(manifest(vec![entry]));
        let mut factories = AssetFactories::default();
        factories.register(RegistryDummyFactory);

        validate_typed_asset_request::<RegistryDummyAsset, _>(&registry, &factories, id)
            .expect("typed request should match manifest entry product");

        let missing = AssetId::new();
        match validate_typed_asset_request::<RegistryDummyAsset, _>(&registry, &factories, missing)
        {
            Err(AssetError::AssetNotFound { id: error_id }) => assert_eq!(error_id, missing),
            Ok(_) => panic!("missing typed request should fail"),
            Err(error) => panic!("unexpected error: {error}"),
        }
    }

    #[test]
    fn registry_resolves_typed_source_asset_and_asset_path() {
        let id = AssetId::new();
        let entry = test_entry(id, RegistryDummyAsset::TYPE);
        let source_path = entry.source_path.clone();
        let config = AssetConfig::new("assets", "native");
        let registry = ManifestIndex::new(manifest(vec![entry]));
        let mut factories = AssetFactories::default();
        factories.register(RegistryDummyFactory);

        let resolved = resolve_typed_source_asset::<RegistryDummyAsset, _>(
            &config,
            &registry,
            &factories,
            Path::new(&source_path),
        )
        .expect("source path should resolve and validate");
        assert_eq!(resolved, id);

        let typed_path = typed_asset_path::<RegistryDummyAsset, _>(&registry, &factories, id)
            .expect("typed asset path should validate");
        assert_eq!(typed_path.as_path(), Path::new(&source_path));

        match resolve_typed_source_asset::<RegistryDummyAsset, _>(
            &config,
            &registry,
            &factories,
            Path::new("missing.source"),
        ) {
            Err(AssetError::AssetPathNotFound { path }) => {
                assert_eq!(path, PathBuf::from("missing.source"));
            }
            Ok(_) => panic!("missing source path should fail"),
            Err(error) => panic!("unexpected error: {error}"),
        }
    }

    #[test]
    fn asset_factories_detect_registered_product_type_mismatch() {
        let mut factories = AssetFactories::default();

        factories.register(RegistryLyingFactory);

        match factories.ensure_registered_product::<RegistryDummyAsset>() {
            Err(AssetError::Internal { message }) => {
                assert_eq!(
                    message,
                    "registered `registry.dummy` factory product type does not match requested asset type"
                );
            }
            Ok(_) => panic!("factory product type mismatch should fail"),
            Err(error) => panic!("unexpected error: {error}"),
        }
    }

    #[test]
    fn refresh_manifest_records_updates_bound_records_and_reports_missing() {
        let retained = AssetId::new();
        let missing = AssetId::new();
        let raw = AssetId::new();
        let initial_entry = test_entry(retained, RegistryDummyAsset::TYPE);
        let config = AssetConfig::new("assets", "native");
        let mut index = LocalManifestRegistry::new(manifest(vec![initial_entry.clone()]));
        assert_eq!(
            index.lookup_source_asset(&config, Path::new(&initial_entry.source_path)),
            Some(retained)
        );
        let mut store = AssetStore::default();
        store.records.insert(
            retained,
            AssetRecord::new(retained, RegistryDummyAsset::TYPE.to_string()),
        );
        store.records.insert(
            missing,
            AssetRecord::new(missing, RegistryDummyAsset::TYPE.to_string()),
        );
        store.records.insert(
            raw,
            AssetRecord::new_raw_texture(PathBuf::from("loose.png")),
        );

        let refresh = refresh_manifest_records(
            &mut index,
            &mut store,
            manifest(vec![test_entry(retained, RegistryOtherAsset::TYPE)]),
        );

        assert_eq!(refresh.missing_records, vec![missing]);
        assert_eq!(
            store
                .records
                .get(&retained)
                .expect("retained record exists")
                .asset_type,
            RegistryOtherAsset::TYPE
        );
        assert!(store.records.contains_key(&raw));
        assert!(index.entry(missing).is_none());
    }

    #[test]
    fn refresh_manifest_records_or_fail_records_missing_manifest_failure() {
        let retained = AssetId::new();
        let missing = AssetId::new();
        let now = Instant::now();
        let initial_entry = test_entry(retained, RegistryDummyAsset::TYPE);
        let mut index = LocalManifestRegistry::new(manifest(vec![initial_entry]));
        let mut store = AssetStore::default();
        store.records.insert(
            retained,
            AssetRecord::new(retained, RegistryDummyAsset::TYPE.to_string()),
        );
        let mut missing_record = AssetRecord::new(missing, RegistryDummyAsset::TYPE.to_string());
        missing_record.state = AssetState::Installed;
        missing_record.strong_ref_count = 1;
        missing_record.load_generation = 9;
        store.records.insert(missing, missing_record);
        let mut events = AssetEventLog::new(8);
        let mut cursor = events.cursor();
        let mut requests = AssetRequests::default();
        requests.enqueue(missing, 9, None, 4, now);
        let request = requests.pop_queued().expect("queued request");
        requests.activate(request, AssetRequestPhase::Installing, 9, now);

        let refresh = refresh_manifest_records_or_fail(
            &mut index,
            &mut store,
            &mut events,
            &mut requests,
            manifest(vec![test_entry(retained, RegistryOtherAsset::TYPE)]),
            4,
            now,
        );

        assert_eq!(refresh.missing_records, vec![missing]);
        assert_eq!(store.records[&missing].state, AssetState::Failed);
        assert_eq!(
            store.records[&missing].failure_phase,
            Some(AssetFailurePhase::Lookup)
        );
        assert_eq!(
            store.records[&retained].asset_type,
            RegistryOtherAsset::TYPE
        );
        let emitted = events.events_since(&mut cursor);
        assert_eq!(emitted.len(), 1);
        assert_eq!(emitted[0].id, missing);
        assert_eq!(emitted[0].kind, AssetEventKind::Failed);
        assert_eq!(emitted[0].failure_phase, Some(AssetFailurePhase::Lookup));
        let failed = requests.failed_snapshots();
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].asset_id, missing);
        assert_eq!(failed[0].generation, 9);
        assert_eq!(failed[0].status, AssetRequestStatus::Failed);
    }

    #[test]
    fn manifest_index_resolves_source_cooked_and_package_watch_paths() {
        let dir = tempdir().expect("temporary asset root");
        let config = AssetConfig::new(dir.path(), "native").with_package_root("packages/base");
        let id = AssetId::new();
        let entry = AssetManifestEntry {
            asset_id: id,
            asset_type: RegistryDummyAsset::TYPE.to_string(),
            importer: "registry.importer".to_string(),
            cooker: "registry.cooker".to_string(),
            version: 1,
            source_path: "source/hero.dummy".to_string(),
            cooked_path: "hero.dummyc".to_string(),
            dependencies: Vec::new(),
            import_settings: serde_json::Value::Null,
        };
        let index = LocalManifestRegistry::new(manifest(vec![entry]));

        assert_eq!(
            AssetRegistry::lookup_watch_asset(
                &index,
                &config,
                &config.asset_root.join("source/hero.dummy")
            ),
            Some(id)
        );
        assert_eq!(
            AssetRegistry::lookup_watch_asset(
                &index,
                &config,
                &config.cooked_root().join("hero.dummyc")
            ),
            Some(id)
        );
        assert_eq!(
            AssetRegistry::lookup_watch_asset(
                &index,
                &config,
                &config.asset_root.join("packages/base/hero.dummyc")
            ),
            Some(id)
        );
        assert_eq!(
            AssetRegistry::lookup_watch_asset(
                &index,
                &config,
                &config.asset_root.join("untracked.dummy")
            ),
            None
        );
    }

    #[test]
    fn local_manifest_registry_resolves_metadata_dependencies_location_and_watch_key() {
        let dir = tempdir().expect("temporary asset root");
        let config = AssetConfig::new(dir.path(), "native")
            .with_package_root("packages/base")
            .with_package_file("bundles/base.skybundle");
        let id = AssetId::new();
        let dependency = AssetId::new();
        let entry = AssetManifestEntry {
            asset_id: id,
            asset_type: RegistryDummyAsset::TYPE.to_string(),
            importer: "registry.importer".to_string(),
            cooker: "registry.cooker".to_string(),
            version: 7,
            source_path: "source/hero.dummy".to_string(),
            cooked_path: "hero.dummyc".to_string(),
            dependencies: vec![dependency],
            import_settings: serde_json::Value::Null,
        };
        let registry = LocalManifestRegistry::new(manifest(vec![entry]));
        let registry_view: &dyn AssetRegistry = &registry;

        let metadata = registry_view.metadata(id).expect("metadata should resolve");
        assert_eq!(metadata.asset_id, id);
        assert_eq!(metadata.asset_type, RegistryDummyAsset::TYPE);
        assert_eq!(metadata.importer, "registry.importer");
        assert_eq!(metadata.cooker, "registry.cooker");
        assert_eq!(metadata.version, 7);
        assert_eq!(metadata.source_path, "source/hero.dummy");
        assert_eq!(metadata.cooked_path, "hero.dummyc");
        assert_eq!(
            registry_view.dependencies(id).expect("dependencies"),
            &[dependency]
        );

        let location = registry_view
            .resolve_location(&config, id)
            .expect("location should resolve");
        assert_eq!(
            location.source_path,
            config.asset_root.join("source/hero.dummy")
        );
        assert_eq!(
            location.cooked_path,
            config.cooked_root().join("hero.dummyc")
        );
        assert_eq!(
            location.package_paths,
            vec![config.asset_root.join("packages/base/hero.dummyc")]
        );
        assert_eq!(
            location.package_files,
            vec![config.asset_root.join("bundles/base.skybundle")]
        );

        let watch_key = registry_view.watch_key(&config, id).expect("watch key");
        assert_eq!(watch_key.source_path, location.source_path);
        assert_eq!(watch_key.cooked_path, location.cooked_path);
        assert_eq!(watch_key.package_paths, location.package_paths);
        assert_eq!(watch_key.package_files, location.package_files);
        assert!(registry_view.metadata(dependency).is_none());
        assert!(registry_view.dependencies(dependency).is_none());
        assert!(registry_view
            .resolve_location(&config, dependency)
            .is_none());
        assert!(registry_view.watch_key(&config, dependency).is_none());
    }

    #[test]
    fn load_manifest_returns_empty_when_no_cooked_root_exists() {
        let dir = tempdir().expect("temporary asset root");
        let config = AssetConfig::new(dir.path(), "native");

        let manifest = load_manifest(&config).expect("missing manifest without cooked root");

        assert_eq!(manifest.version, ASSET_SYSTEM_VERSION);
        assert_eq!(manifest.target, "native");
        assert!(manifest.assets.is_empty());
    }

    #[test]
    fn load_manifest_rejects_version_mismatch() {
        let dir = tempdir().expect("temporary asset root");
        let config = AssetConfig::new(dir.path(), "native");
        std::fs::create_dir_all(config.cooked_root()).expect("cooked root");
        std::fs::write(
            config.manifest_path(),
            serde_json::json!({
                "version": ASSET_SYSTEM_VERSION + 1,
                "target": "native",
                "assets": []
            })
            .to_string(),
        )
        .expect("manifest");

        let error = load_manifest(&config).expect_err("version mismatch should fail");

        assert!(matches!(
            error,
            AssetError::VersionMismatch {
                expected: ASSET_SYSTEM_VERSION,
                actual,
                ..
            } if actual == ASSET_SYSTEM_VERSION + 1
        ));
    }
}
