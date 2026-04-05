use std::any::{Any, TypeId};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use super::registry::{AssetRuntimeFactory, ErasedAssetFactory, FactoryAdapter, ManifestIndex};
use super::texture::TextureAssetFactory;
use super::types::{
    Asset, AssetConfig, AssetError, AssetId, AssetInstallContext, AssetLoadContext,
    AssetRegistryManifest, AssetState, Handle, ASSET_SYSTEM_VERSION,
};

#[derive(Clone)]
pub struct AssetServer {
    inner: Arc<Mutex<AssetServerInner>>,
}

impl std::fmt::Debug for AssetServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AssetServer").finish_non_exhaustive()
    }
}

impl AssetServer {
    pub fn new(config: AssetConfig) -> Result<Self, AssetError> {
        let manifest = load_manifest(&config)?;
        let mut inner = AssetServerInner::new(config, manifest);
        inner.register_factory(TextureAssetFactory);
        Ok(Self {
            inner: Arc::new(Mutex::new(inner)),
        })
    }

    #[must_use]
    pub fn with_empty_manifest(config: AssetConfig) -> Self {
        let mut inner = AssetServerInner::new(config, AssetRegistryManifest::default());
        inner.register_factory(TextureAssetFactory);
        Self {
            inner: Arc::new(Mutex::new(inner)),
        }
    }

    pub fn register_factory<F>(&self, factory: F)
    where
        F: AssetRuntimeFactory,
    {
        let mut inner = self.inner.lock().expect("asset server mutex poisoned");
        inner.register_factory(factory);
    }

    pub fn reload_manifest(&self) -> Result<(), AssetError> {
        let mut inner = self.inner.lock().expect("asset server mutex poisoned");
        let manifest = load_manifest(&inner.config)?;
        inner.set_manifest(manifest);
        Ok(())
    }

    #[must_use]
    pub fn config(&self) -> AssetConfig {
        self.inner
            .lock()
            .expect("asset server mutex poisoned")
            .config
            .clone()
    }

    pub fn load<T: Asset>(&self, id: AssetId) -> Result<Handle<T>, AssetError> {
        let mut inner = self.inner.lock().expect("asset server mutex poisoned");
        inner.validate_typed_request::<T>(id)?;
        inner.queue_request(id, Some(TypeId::of::<T>()));
        Ok(Handle::new(id))
    }

    pub fn load_by_path<T: Asset>(&self, path: impl AsRef<Path>) -> Result<Handle<T>, AssetError> {
        let path_buf = path.as_ref().to_path_buf();
        let mut inner = self.inner.lock().expect("asset server mutex poisoned");
        let id = inner
            .lookup_source_asset(&path_buf)
            .ok_or_else(|| AssetError::AssetPathNotFound { path: path_buf })?;
        inner.validate_typed_request::<T>(id)?;
        inner.queue_request(id, Some(TypeId::of::<T>()));
        Ok(Handle::new(id))
    }

    pub fn load_blocking<T: Asset>(&self, id: AssetId) -> Result<Arc<T>, AssetError> {
        let handle = self.load::<T>(id)?;
        for _ in 0..64 {
            self.update()?;
            if self.is_installed(&handle) {
                return self.get(&handle);
            }
            if self.state(&handle) == AssetState::Failed {
                return match self.error(&handle) {
                    Some(error) => Err(error),
                    None => Err(AssetError::AssetNotInstalled {
                        id,
                        state: AssetState::Failed,
                    }),
                };
            }
        }
        Err(AssetError::InvalidState {
            id,
            state: self.state(&handle),
            message: "asset did not reach a terminal state after blocking updates".to_string(),
        })
    }

    pub fn update(&self) -> Result<(), AssetError> {
        let mut inner = self.inner.lock().expect("asset server mutex poisoned");
        inner.apply_update()
    }

    #[must_use]
    pub fn state<T: Asset>(&self, handle: &Handle<T>) -> AssetState {
        self.state_untyped(handle.id())
    }

    #[must_use]
    pub fn state_untyped(&self, id: AssetId) -> AssetState {
        self.inner
            .lock()
            .expect("asset server mutex poisoned")
            .records
            .get(&id)
            .map(|record| record.state)
            .unwrap_or(AssetState::Unloaded)
    }

    #[must_use]
    pub fn is_installed<T: Asset>(&self, handle: &Handle<T>) -> bool {
        self.state(handle) == AssetState::Installed
    }

    pub fn get<T: Asset>(&self, handle: &Handle<T>) -> Result<Arc<T>, AssetError> {
        let inner = self.inner.lock().expect("asset server mutex poisoned");
        let record = inner
            .records
            .get(&handle.id())
            .ok_or(AssetError::AssetNotInstalled {
                id: handle.id(),
                state: AssetState::Unloaded,
            })?;
        let installed = record
            .installed
            .clone()
            .ok_or(AssetError::AssetNotInstalled {
                id: handle.id(),
                state: record.state,
            })?;
        Arc::downcast::<T>(installed).map_err(|_| AssetError::AssetTypeMismatch {
            id: handle.id(),
            expected: T::TYPE,
            actual: record.asset_type.clone(),
        })
    }

    #[must_use]
    pub fn try_get<T: Asset>(&self, handle: &Handle<T>) -> Option<Arc<T>> {
        self.get(handle).ok()
    }

    #[must_use]
    pub fn error<T: Asset>(&self, handle: &Handle<T>) -> Option<AssetError> {
        self.inner
            .lock()
            .expect("asset server mutex poisoned")
            .records
            .get(&handle.id())
            .and_then(|record| record.error.clone())
    }

    pub fn unload<T: Asset>(&self, handle: &Handle<T>) {
        self.unload_untyped(handle.id());
    }

    pub fn unload_untyped(&self, id: AssetId) {
        let mut inner = self.inner.lock().expect("asset server mutex poisoned");
        inner.queue_unload(id);
    }

    #[must_use]
    pub fn manifest(&self) -> AssetRegistryManifest {
        self.inner
            .lock()
            .expect("asset server mutex poisoned")
            .manifest
            .manifest
            .clone()
    }

    #[must_use]
    pub fn resolve_path(&self, path: impl AsRef<Path>) -> Option<AssetId> {
        self.inner
            .lock()
            .expect("asset server mutex poisoned")
            .lookup_source_asset(path.as_ref())
    }
}

struct AssetServerInner {
    config: AssetConfig,
    manifest: ManifestIndex,
    factories: HashMap<String, Arc<dyn ErasedAssetFactory>>,
    records: HashMap<AssetId, AssetRecord>,
    requests: VecDeque<AssetRequest>,
    unloads: VecDeque<AssetId>,
}

impl AssetServerInner {
    fn new(config: AssetConfig, manifest: AssetRegistryManifest) -> Self {
        Self {
            config,
            manifest: ManifestIndex::new(manifest),
            factories: HashMap::default(),
            records: HashMap::default(),
            requests: VecDeque::new(),
            unloads: VecDeque::new(),
        }
    }

    fn set_manifest(&mut self, manifest: AssetRegistryManifest) {
        self.manifest = ManifestIndex::new(manifest);
    }

    fn register_factory<F>(&mut self, factory: F)
    where
        F: AssetRuntimeFactory,
    {
        self.factories.insert(
            factory.asset_type().to_string(),
            Arc::new(FactoryAdapter(factory)),
        );
    }

    fn validate_typed_request<T: Asset>(&self, id: AssetId) -> Result<(), AssetError> {
        let entry = self
            .manifest
            .entry(id)
            .ok_or(AssetError::AssetNotFound { id })?;
        let factory = self.factories.get(&entry.asset_type).ok_or_else(|| {
            AssetError::FactoryNotRegistered {
                asset_type: entry.asset_type.clone(),
            }
        })?;

        if factory.product_type_id() != TypeId::of::<T>() {
            return Err(AssetError::AssetTypeMismatch {
                id,
                expected: T::TYPE,
                actual: entry.asset_type.clone(),
            });
        }

        Ok(())
    }

    fn lookup_source_asset(&self, path: &Path) -> Option<AssetId> {
        let key = self.config.source_key(path);
        self.manifest.source_to_id.get(&key).copied()
    }

    fn queue_request(&mut self, id: AssetId, requested_type: Option<TypeId>) {
        let record = self.records.entry(id).or_insert_with(|| {
            let entry = self
                .manifest
                .entry(id)
                .expect("asset record must exist in manifest");
            AssetRecord::new(id, entry.asset_type.clone())
        });

        if record.requested_type.is_none() {
            record.requested_type = requested_type;
        }

        self.requests.push_back(AssetRequest { id, requested_type });
    }

    fn queue_unload(&mut self, id: AssetId) {
        self.unloads.push_back(id);
    }

    fn apply_update(&mut self) -> Result<(), AssetError> {
        while let Some(request) = self.requests.pop_front() {
            let entry = self
                .manifest
                .entry(request.id)
                .ok_or(AssetError::AssetNotFound { id: request.id })?
                .clone();
            let record = self
                .records
                .entry(request.id)
                .or_insert_with(|| AssetRecord::new(request.id, entry.asset_type.clone()));

            if record.state == AssetState::Unloaded || record.state == AssetState::Failed {
                record.state = AssetState::Loading;
                record.error = None;
                if record.requested_type.is_none() {
                    record.requested_type = request.requested_type;
                }
            }
        }

        while let Some(id) = self.unloads.pop_front() {
            if let Some(record) = self.records.get_mut(&id) {
                match record.state {
                    AssetState::Installed => record.state = AssetState::Uninstalling,
                    AssetState::Loaded
                    | AssetState::WaitingDependencies
                    | AssetState::Installing
                    | AssetState::Failed => record.state = AssetState::Unloading,
                    AssetState::Unloaded | AssetState::Loading => {
                        record.loaded = None;
                        record.installed = None;
                        record.error = None;
                        record.state = AssetState::Unloaded;
                    }
                    AssetState::Uninstalling | AssetState::Unloading => {}
                }
            }
        }

        let mut iterations = 0usize;
        loop {
            iterations += 1;
            if iterations > self.records.len().saturating_mul(4).max(8) {
                break;
            }

            let ids: Vec<_> = self.records.keys().copied().collect();
            let mut progressed = false;

            for id in ids {
                let state = match self.records.get(&id) {
                    Some(record) => record.state,
                    None => continue,
                };

                match state {
                    AssetState::Loading => {
                        self.load_record(id)?;
                        progressed = true;
                    }
                    AssetState::Loaded | AssetState::WaitingDependencies => {
                        if let Some(next_state) = self.evaluate_dependencies(id)? {
                            self.records
                                .get_mut(&id)
                                .expect("record should exist")
                                .state = next_state;
                            progressed = true;
                        }
                    }
                    AssetState::Installing => {
                        self.install_record(id)?;
                        progressed = true;
                    }
                    AssetState::Uninstalling => {
                        let record = self.records.get_mut(&id).expect("record should exist");
                        record.installed = None;
                        record.state = AssetState::Unloading;
                        progressed = true;
                    }
                    AssetState::Unloading => {
                        let record = self.records.get_mut(&id).expect("record should exist");
                        record.loaded = None;
                        record.installed = None;
                        record.dependencies.clear();
                        record.error = None;
                        record.state = AssetState::Unloaded;
                        progressed = true;
                    }
                    AssetState::Unloaded | AssetState::Installed | AssetState::Failed => {}
                }
            }

            if !progressed {
                break;
            }
        }

        Ok(())
    }

    fn load_record(&mut self, id: AssetId) -> Result<(), AssetError> {
        let entry = self
            .manifest
            .entry(id)
            .ok_or(AssetError::AssetNotFound { id })?
            .clone();
        let cooked_path = self.config.cooked_root().join(&entry.cooked_path);
        let cooked_root = self.config.cooked_root();
        let bytes =
            std::fs::read(&cooked_path).map_err(|error| map_read_error(id, &cooked_path, error))?;
        let factory = self
            .factories
            .get(&entry.asset_type)
            .cloned()
            .ok_or_else(|| AssetError::FactoryNotRegistered {
                asset_type: entry.asset_type.clone(),
            })?;

        let result = factory.load(AssetLoadContext {
            asset_id: id,
            entry: &entry,
            bytes: &bytes,
            asset_root: &self.config.asset_root,
            cooked_root: &cooked_root,
        });

        let record = self.records.get_mut(&id).expect("record should exist");
        match result {
            Ok(loaded) => {
                record.loaded = Some(loaded.loaded);
                record.dependencies = if loaded.dependencies.is_empty() {
                    entry.dependencies.clone()
                } else {
                    loaded.dependencies
                };
                record.error = None;
                record.state = AssetState::Loaded;
                Ok(())
            }
            Err(error) => {
                record.error = Some(error.clone());
                record.state = AssetState::Failed;
                Err(error)
            }
        }
    }

    fn evaluate_dependencies(&mut self, id: AssetId) -> Result<Option<AssetState>, AssetError> {
        let dependencies = self
            .records
            .get(&id)
            .map(|record| record.dependencies.clone())
            .unwrap_or_default();

        if dependencies.is_empty() {
            return Ok(Some(AssetState::Installing));
        }

        let mut waiting = false;
        for dependency in dependencies {
            let Some(entry) = self.manifest.entry(dependency) else {
                let error = AssetError::MissingDependency { id, dependency };
                let record = self.records.get_mut(&id).expect("record should exist");
                record.error = Some(error.clone());
                record.state = AssetState::Failed;
                return Err(error);
            };

            if !self.records.contains_key(&dependency) {
                self.records.insert(
                    dependency,
                    AssetRecord::new(dependency, entry.asset_type.clone()),
                );
                self.requests.push_back(AssetRequest {
                    id: dependency,
                    requested_type: None,
                });
                waiting = true;
                continue;
            }

            match self.records.get(&dependency).map(|record| record.state) {
                Some(AssetState::Installed) => {}
                Some(AssetState::Failed) => {
                    let error = AssetError::DependencyFailed { id, dependency };
                    let record = self.records.get_mut(&id).expect("record should exist");
                    record.error = Some(error.clone());
                    record.state = AssetState::Failed;
                    return Err(error);
                }
                Some(_) | None => waiting = true,
            }
        }

        Ok(Some(if waiting {
            AssetState::WaitingDependencies
        } else {
            AssetState::Installing
        }))
    }

    fn install_record(&mut self, id: AssetId) -> Result<(), AssetError> {
        let entry = self
            .manifest
            .entry(id)
            .ok_or(AssetError::AssetNotFound { id })?
            .clone();
        let factory = self
            .factories
            .get(&entry.asset_type)
            .cloned()
            .ok_or_else(|| AssetError::FactoryNotRegistered {
                asset_type: entry.asset_type.clone(),
            })?;
        let loaded = self
            .records
            .get(&id)
            .and_then(|record| record.loaded.clone())
            .ok_or_else(|| AssetError::InvalidState {
                id,
                state: AssetState::Installing,
                message: "missing loaded payload".to_string(),
            })?;

        let result = factory.install(
            &loaded,
            AssetInstallContext {
                asset_id: id,
                entry: &entry,
            },
        );

        let record = self.records.get_mut(&id).expect("record should exist");
        match result {
            Ok(installed) => {
                record.installed = Some(installed);
                record.error = None;
                record.state = AssetState::Installed;
                Ok(())
            }
            Err(error) => {
                record.error = Some(error.clone());
                record.state = AssetState::Failed;
                Err(error)
            }
        }
    }
}

#[derive(Clone, Debug)]
struct AssetRequest {
    id: AssetId,
    requested_type: Option<TypeId>,
}

#[derive(Clone)]
struct AssetRecord {
    asset_type: String,
    state: AssetState,
    requested_type: Option<TypeId>,
    dependencies: Vec<AssetId>,
    loaded: Option<Arc<dyn Any + Send + Sync>>,
    installed: Option<Arc<dyn Any + Send + Sync>>,
    error: Option<AssetError>,
}

impl AssetRecord {
    fn new(_id: AssetId, asset_type: String) -> Self {
        Self {
            asset_type,
            state: AssetState::Unloaded,
            requested_type: None,
            dependencies: Vec::new(),
            loaded: None,
            installed: None,
            error: None,
        }
    }
}

fn load_manifest(config: &AssetConfig) -> Result<AssetRegistryManifest, AssetError> {
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

fn map_read_error(id: AssetId, path: &PathBuf, error: std::io::Error) -> AssetError {
    if error.kind() == std::io::ErrorKind::NotFound {
        AssetError::MissingCookedArtifact {
            id,
            path: path.clone(),
        }
    } else {
        AssetError::Io {
            path: path.clone(),
            message: error.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset::{AssetManifestEntry, LoadedAsset};
    use tempfile::tempdir;

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct DummyAsset(String);

    impl Asset for DummyAsset {
        const TYPE: &'static str = "dummy";
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct LoadedDummy(String);

    struct DummyFactory;

    impl AssetRuntimeFactory for DummyFactory {
        type Asset = DummyAsset;
        type Loaded = LoadedDummy;

        fn load(&self, ctx: AssetLoadContext<'_>) -> Result<LoadedAsset<Self::Loaded>, AssetError> {
            let text =
                std::str::from_utf8(ctx.bytes).map_err(|error| AssetError::InvalidCookedAsset {
                    id: Some(ctx.asset_id),
                    message: error.to_string(),
                })?;
            Ok(LoadedAsset::new(LoadedDummy(text.to_string()))
                .with_dependencies(ctx.entry.dependencies.clone()))
        }

        fn install(
            &self,
            loaded: &Self::Loaded,
            _ctx: AssetInstallContext<'_>,
        ) -> Result<Self::Asset, AssetError> {
            Ok(DummyAsset(loaded.0.clone()))
        }
    }

    fn write_manifest(
        root: &Path,
        entry: AssetManifestEntry,
    ) -> Result<AssetConfig, Box<dyn std::error::Error>> {
        let config = AssetConfig::new(root, "native");
        std::fs::create_dir_all(config.cooked_root())?;
        let manifest = AssetRegistryManifest {
            version: ASSET_SYSTEM_VERSION,
            target: "native".to_string(),
            assets: vec![entry],
        };
        std::fs::write(
            config.manifest_path(),
            serde_json::to_vec_pretty(&manifest)?,
        )?;
        Ok(config)
    }

    #[test]
    fn assets_progress_to_installed_and_can_be_read() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        let asset_id = AssetId::new();
        let config = write_manifest(
            dir.path(),
            AssetManifestEntry {
                asset_id,
                asset_type: "dummy".to_string(),
                importer: "dummy".to_string(),
                cooker: "dummy".to_string(),
                version: 1,
                source_path: "clip.dummy".to_string(),
                cooked_path: "clip.dummyc".to_string(),
                dependencies: Vec::new(),
                import_settings: serde_json::Value::Null,
            },
        )?;
        std::fs::write(config.cooked_root().join("clip.dummyc"), b"ready")?;

        let server = AssetServer::new(config)?;
        server.register_factory(DummyFactory);
        let handle = server.load::<DummyAsset>(asset_id)?;

        server.update()?;
        assert_eq!(server.state(&handle), AssetState::Installed);
        assert_eq!(server.get(&handle)?.0, "ready");
        Ok(())
    }

    #[test]
    fn missing_dependency_marks_asset_failed() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        let asset_id = AssetId::new();
        let dependency = AssetId::new();
        let config = write_manifest(
            dir.path(),
            AssetManifestEntry {
                asset_id,
                asset_type: "dummy".to_string(),
                importer: "dummy".to_string(),
                cooker: "dummy".to_string(),
                version: 1,
                source_path: "clip.dummy".to_string(),
                cooked_path: "clip.dummyc".to_string(),
                dependencies: vec![dependency],
                import_settings: serde_json::Value::Null,
            },
        )?;
        std::fs::write(config.cooked_root().join("clip.dummyc"), b"ready")?;

        let server = AssetServer::new(config)?;
        server.register_factory(DummyFactory);
        let handle = server.load::<DummyAsset>(asset_id)?;
        let result = server.update();

        assert!(matches!(
            result,
            Err(AssetError::MissingDependency { id, dependency: dep }) if id == asset_id && dep == dependency
        ));
        assert_eq!(server.state(&handle), AssetState::Failed);
        Ok(())
    }

    #[test]
    fn typed_handles_reject_manifest_type_mismatch() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        let asset_id = AssetId::new();
        let config = write_manifest(
            dir.path(),
            AssetManifestEntry {
                asset_id,
                asset_type: "texture".to_string(),
                importer: "texture.image".to_string(),
                cooker: "texture.rgba8".to_string(),
                version: 1,
                source_path: "hero.png".to_string(),
                cooked_path: "hero.skytx".to_string(),
                dependencies: Vec::new(),
                import_settings: serde_json::json!({ "srgb": true }),
            },
        )?;
        let server = AssetServer::new(config)?;
        let result = server.load::<DummyAsset>(asset_id);
        assert!(matches!(result, Err(AssetError::AssetTypeMismatch { .. })));
        Ok(())
    }

    #[test]
    fn unload_returns_asset_to_unloaded_state() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        let asset_id = AssetId::new();
        let config = write_manifest(
            dir.path(),
            AssetManifestEntry {
                asset_id,
                asset_type: "dummy".to_string(),
                importer: "dummy".to_string(),
                cooker: "dummy".to_string(),
                version: 1,
                source_path: "clip.dummy".to_string(),
                cooked_path: "clip.dummyc".to_string(),
                dependencies: Vec::new(),
                import_settings: serde_json::Value::Null,
            },
        )?;
        std::fs::write(config.cooked_root().join("clip.dummyc"), b"ready")?;

        let server = AssetServer::new(config)?;
        server.register_factory(DummyFactory);
        let handle = server.load::<DummyAsset>(asset_id)?;
        server.update()?;

        server.unload(&handle);
        server.update()?;
        assert_eq!(server.state(&handle), AssetState::Unloaded);
        Ok(())
    }
}
