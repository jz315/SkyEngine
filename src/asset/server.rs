use std::any::{Any, TypeId};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, Weak};
use std::time::Instant;

use super::driver::{self, AssetDriveMode, AssetRecordDriver};
use super::events::{self, AssetEventLog};
use super::font::{FontAsset, FontAssetFactory};
#[cfg(test)]
use super::install::AssetInstallTask;
use super::install::{self, AssetInstallBudget};
#[cfg(test)]
use super::install::{
    AssetInstallContext, AssetInstallPoll, AssetInstallResult, AssetUninstallContext,
};
use super::load::{
    drain_completed_loads_or_fail, has_current_inflight_load, load_record_now_and_apply_or_fail,
    should_load_record_in_background, submit_record_load_or_fail, AssetLoadQueue,
};
#[cfg(test)]
use super::provider::ResolvedAssetSource;
use super::provider::{AssetProvider, LocalAssetProvider};
use super::registry::{
    resolve_typed_source_asset, typed_asset_path, AssetFactories, AssetRegistryLoader,
    AssetRuntimeFactory, LocalManifestRegistry, LocalManifestRegistryLoader,
};
use super::reload::{self, AssetReloadController};
use super::request::AssetRequests;
use super::runtime;
use super::store::AssetStore;
use super::texture::{TextureAsset, TextureAssetFactory};
#[cfg(test)]
use super::types::AssetLoadContext;
use super::types::{
    Asset, AssetConfig, AssetDiagnosticsSnapshot, AssetError, AssetEvent, AssetEventCursor,
    AssetFailurePhase, AssetFailureSnapshot, AssetHandleProvider, AssetId, AssetLease,
    AssetMetadata, AssetPath, AssetRegistryManifest, AssetReloadReport, AssetReloadStatus,
    AssetRequestSnapshot, AssetState, AssetStats, AssetStatus, AssetWatchPaths, Handle, WeakHandle,
};
use super::watcher::AssetFileWatcher;
use super::{blocking, dependency, diagnostics, lease};

#[derive(Clone)]
pub struct Assets {
    inner: Arc<Mutex<AssetsInner>>,
    release_tx: Sender<AssetId>,
}

impl std::fmt::Debug for Assets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Assets").finish_non_exhaustive()
    }
}

impl Assets {
    pub fn new(config: AssetConfig) -> Result<Self, AssetError> {
        let registry_loader = Box::new(LocalManifestRegistryLoader);
        let manifest = registry_loader.load(&config)?;
        let (release_tx, release_rx) = mpsc::channel();
        let mut inner = AssetsInner::new_with_registry_loader(
            config,
            manifest,
            release_tx.clone(),
            release_rx,
            registry_loader,
        );
        inner.register_factory(TextureAssetFactory);
        inner.register_factory(FontAssetFactory);
        Ok(Self::from_inner(inner, release_tx))
    }

    #[must_use]
    pub fn with_empty_manifest(config: AssetConfig) -> Self {
        let (release_tx, release_rx) = mpsc::channel();
        let mut inner = AssetsInner::new(
            config,
            AssetRegistryManifest::default(),
            release_tx.clone(),
            release_rx,
        );
        inner.register_factory(TextureAssetFactory);
        inner.register_factory(FontAssetFactory);
        Self::from_inner(inner, release_tx)
    }

    #[cfg(test)]
    fn with_manifest_and_provider(
        config: AssetConfig,
        manifest: AssetRegistryManifest,
        provider: impl AssetProvider + 'static,
    ) -> Self {
        let (release_tx, release_rx) = mpsc::channel();
        let mut inner = AssetsInner::new_with_provider(
            config,
            manifest,
            release_tx.clone(),
            release_rx,
            Box::new(provider),
        );
        inner.register_factory(TextureAssetFactory);
        inner.register_factory(FontAssetFactory);
        Self::from_inner(inner, release_tx)
    }

    #[cfg(test)]
    fn with_registry_loader_and_provider(
        config: AssetConfig,
        registry_loader: impl AssetRegistryLoader + 'static,
        provider: impl AssetProvider + 'static,
    ) -> Result<Self, AssetError> {
        let registry_loader = Box::new(registry_loader);
        let manifest = registry_loader.load(&config)?;
        let (release_tx, release_rx) = mpsc::channel();
        let mut inner = AssetsInner::new_with_provider_and_registry_loader(
            config,
            manifest,
            release_tx.clone(),
            release_rx,
            Box::new(provider),
            registry_loader,
        );
        inner.register_factory(TextureAssetFactory);
        inner.register_factory(FontAssetFactory);
        Ok(Self::from_inner(inner, release_tx))
    }

    fn from_inner(inner: AssetsInner, release_tx: Sender<AssetId>) -> Self {
        let inner = Arc::new(Mutex::new(inner));
        let provider: Arc<dyn AssetHandleProvider> = inner.clone();
        inner
            .lock()
            .expect("assets mutex poisoned")
            .set_handle_provider(Arc::downgrade(&provider));
        Self { inner, release_tx }
    }

    pub fn register_factory<F>(&self, factory: F)
    where
        F: AssetRuntimeFactory,
    {
        let mut inner = self.inner.lock().expect("assets mutex poisoned");
        inner.register_factory(factory);
    }

    pub fn reload_manifest(&self) -> Result<(), AssetError> {
        let mut inner = self.inner.lock().expect("assets mutex poisoned");
        inner.reload_manifest()
    }

    pub fn reload_changed(&self) -> Result<Vec<AssetId>, AssetError> {
        let mut inner = self.inner.lock().expect("assets mutex poisoned");
        inner.reload_changed()
    }

    pub fn reload_changed_with_report(&self) -> Result<AssetReloadReport, AssetError> {
        let mut inner = self.inner.lock().expect("assets mutex poisoned");
        inner.reload_changed_report()
    }

    pub fn force_reload(&self, id: AssetId) -> Result<AssetReloadReport, AssetError> {
        let mut inner = self.inner.lock().expect("assets mutex poisoned");
        inner.force_reload(id)
    }

    pub fn set_auto_reload_frozen(&self, frozen: bool) {
        let mut inner = self.inner.lock().expect("assets mutex poisoned");
        inner.set_auto_reload_frozen(frozen);
    }

    #[must_use]
    pub fn reload_status(&self) -> AssetReloadStatus {
        self.inner
            .lock()
            .expect("assets mutex poisoned")
            .reload_status()
    }

    #[must_use]
    pub fn last_reload_report(&self) -> AssetReloadReport {
        self.inner
            .lock()
            .expect("assets mutex poisoned")
            .last_reload_report()
    }

    #[must_use]
    pub fn config(&self) -> AssetConfig {
        self.inner.lock().expect("assets mutex poisoned").config()
    }

    pub fn load<T: Asset>(&self, path: impl AsRef<Path>) -> Result<Handle<T>, AssetError> {
        self.load_with_priority(path, self.config().io_default_priority)
    }

    pub fn load_with_priority<T: Asset>(
        &self,
        path: impl AsRef<Path>,
        priority: i32,
    ) -> Result<Handle<T>, AssetError> {
        let asset_path = AssetPath::<T>::new(path.as_ref());
        self.load_path_with_priority(&asset_path, priority)
    }

    pub fn load_path<T: Asset>(&self, path: &AssetPath<T>) -> Result<Handle<T>, AssetError> {
        self.load_path_with_priority(path, self.config().io_default_priority)
    }

    pub fn load_path_with_priority<T: Asset>(
        &self,
        path: &AssetPath<T>,
        priority: i32,
    ) -> Result<Handle<T>, AssetError> {
        let path_buf = path.as_path().to_path_buf();
        let mut inner = self.inner.lock().expect("assets mutex poisoned");
        let id = inner.acquire_typed_manifest_source_lease::<T>(&path_buf, priority)?;
        Ok(self.make_handle(id))
    }

    pub fn load_id<T: Asset>(&self, id: AssetId) -> Result<Handle<T>, AssetError> {
        self.load_id_with_priority(id, self.config().io_default_priority)
    }

    pub fn load_id_with_priority<T: Asset>(
        &self,
        id: AssetId,
        priority: i32,
    ) -> Result<Handle<T>, AssetError> {
        let mut inner = self.inner.lock().expect("assets mutex poisoned");
        inner.acquire_typed_manifest_direct_lease::<T>(id, priority)?;
        Ok(self.make_handle(id))
    }

    pub fn load_handle<T: Asset>(&self, handle: WeakHandle<T>) -> Result<Handle<T>, AssetError> {
        self.load_handle_with_priority(handle, self.config().io_default_priority)
    }

    pub fn load_handle_with_priority<T: Asset>(
        &self,
        handle: WeakHandle<T>,
        priority: i32,
    ) -> Result<Handle<T>, AssetError> {
        self.load_id_with_priority(handle.id(), priority)
    }

    pub fn load_texture(
        &self,
        path_or_key: impl AsRef<Path>,
    ) -> Result<Handle<TextureAsset>, AssetError> {
        self.load_texture_with_priority(path_or_key, self.config().io_default_priority)
    }

    pub fn load_texture_with_priority(
        &self,
        path_or_key: impl AsRef<Path>,
        priority: i32,
    ) -> Result<Handle<TextureAsset>, AssetError> {
        let mut inner = self.inner.lock().expect("assets mutex poisoned");
        let id = inner.load_texture(path_or_key.as_ref(), priority)?;
        Ok(self.make_handle(id))
    }

    pub fn load_font(
        &self,
        path_or_key: impl AsRef<Path>,
    ) -> Result<Handle<FontAsset>, AssetError> {
        self.load_font_with_priority(path_or_key, self.config().io_default_priority)
    }

    pub fn load_font_with_priority(
        &self,
        path_or_key: impl AsRef<Path>,
        priority: i32,
    ) -> Result<Handle<FontAsset>, AssetError> {
        let mut inner = self.inner.lock().expect("assets mutex poisoned");
        let id = inner.load_font(path_or_key.as_ref(), priority)?;
        Ok(self.make_handle(id))
    }

    pub fn load_blocking<T: Asset>(&self, id: AssetId) -> Result<Arc<T>, AssetError> {
        self.load_blocking_until::<T>(id, None)
    }

    pub fn load_blocking_with_timeout<T: Asset>(
        &self,
        id: AssetId,
        timeout: std::time::Duration,
    ) -> Result<Arc<T>, AssetError> {
        let deadline = blocking::deadline_from_timeout(id, timeout, || self.state_untyped(id))?;
        self.load_blocking_until::<T>(id, Some(deadline))
    }

    fn load_blocking_until<T: Asset>(
        &self,
        id: AssetId,
        deadline: Option<Instant>,
    ) -> Result<Arc<T>, AssetError> {
        let handle = self.load_id::<T>(id)?;

        blocking::drive_until_ready(
            id,
            deadline,
            |force_synchronous_load| {
                let mut inner = self.inner.lock().expect("assets mutex poisoned");
                inner.apply_blocking_update(id, force_synchronous_load)
            },
            || match self.state(&handle) {
                AssetState::Installed => blocking::BlockingLoadStatus::Ready(self.get(&handle)),
                AssetState::Failed => blocking::BlockingLoadStatus::Failed(self.error(&handle)),
                state => blocking::BlockingLoadStatus::Pending(state),
            },
        )
    }

    pub fn update(&self) -> Result<(), AssetError> {
        let mut inner = self.inner.lock().expect("assets mutex poisoned");
        inner.apply_update()
    }

    #[must_use]
    pub fn status<T: Asset>(&self, handle: &Handle<T>) -> AssetStatus {
        self.state(handle).into()
    }

    #[must_use]
    pub fn state<T: Asset>(&self, handle: &Handle<T>) -> AssetState {
        self.state_untyped(handle.id())
    }

    #[must_use]
    pub fn state_untyped(&self, id: AssetId) -> AssetState {
        self.inner
            .lock()
            .expect("assets mutex poisoned")
            .state_for_handle(id)
    }

    #[must_use]
    pub fn is_installed<T: Asset>(&self, handle: &Handle<T>) -> bool {
        self.state(handle) == AssetState::Installed
    }

    pub fn get<T: Asset>(&self, handle: &Handle<T>) -> Result<Arc<T>, AssetError> {
        self.get_id(handle.id())
    }

    pub fn get_id<T: Asset>(&self, id: AssetId) -> Result<Arc<T>, AssetError> {
        let installed = self
            .inner
            .lock()
            .expect("assets mutex poisoned")
            .get_for_handle(id, T::TYPE, TypeId::of::<T>())?;
        Arc::downcast::<T>(installed).map_err(|_| AssetError::AssetTypeMismatch {
            id,
            expected: T::TYPE,
            actual: "unknown".to_string(),
        })
    }

    #[must_use]
    pub fn try_get<T: Asset>(&self, handle: &Handle<T>) -> Option<Arc<T>> {
        self.get(handle).ok()
    }

    #[must_use]
    pub fn try_get_id<T: Asset>(&self, id: AssetId) -> Option<Arc<T>> {
        self.get_id::<T>(id).ok()
    }

    #[must_use]
    pub fn error<T: Asset>(&self, handle: &Handle<T>) -> Option<AssetError> {
        self.inner
            .lock()
            .expect("assets mutex poisoned")
            .error_for_handle(handle.id())
    }

    #[must_use]
    pub fn failure_phase<T: Asset>(&self, handle: &Handle<T>) -> Option<AssetFailurePhase> {
        self.failure_phase_untyped(handle.id())
    }

    #[must_use]
    pub fn failure_phase_untyped(&self, id: AssetId) -> Option<AssetFailurePhase> {
        self.inner
            .lock()
            .expect("assets mutex poisoned")
            .failure_phase(id)
    }

    #[must_use]
    pub fn stats(&self) -> AssetStats {
        self.inner.lock().expect("assets mutex poisoned").stats()
    }

    #[must_use]
    pub fn queued_request_snapshots(&self) -> Vec<AssetRequestSnapshot> {
        self.inner
            .lock()
            .expect("assets mutex poisoned")
            .queued_request_snapshots()
    }

    #[must_use]
    pub fn active_request_snapshots(&self) -> Vec<AssetRequestSnapshot> {
        self.inner
            .lock()
            .expect("assets mutex poisoned")
            .active_request_snapshots()
    }

    #[must_use]
    pub fn failed_request_snapshots(&self) -> Vec<AssetRequestSnapshot> {
        self.inner
            .lock()
            .expect("assets mutex poisoned")
            .failed_request_snapshots()
    }

    #[must_use]
    pub fn canceled_request_snapshots(&self) -> Vec<AssetRequestSnapshot> {
        self.inner
            .lock()
            .expect("assets mutex poisoned")
            .canceled_request_snapshots()
    }

    #[must_use]
    pub fn failed_asset_snapshots(&self) -> Vec<AssetFailureSnapshot> {
        self.inner
            .lock()
            .expect("assets mutex poisoned")
            .failed_asset_snapshots()
    }

    #[must_use]
    pub fn diagnostics_snapshot(&self) -> AssetDiagnosticsSnapshot {
        self.inner
            .lock()
            .expect("assets mutex poisoned")
            .diagnostics_snapshot()
    }

    #[must_use]
    pub fn manifest(&self) -> AssetRegistryManifest {
        self.inner
            .lock()
            .expect("assets mutex poisoned")
            .manifest_snapshot()
    }

    #[must_use]
    pub fn resolve_path(&self, path: impl AsRef<Path>) -> Option<AssetId> {
        self.inner
            .lock()
            .expect("assets mutex poisoned")
            .lookup_source_asset(path.as_ref())
    }

    #[must_use]
    pub fn source_path(&self, id: AssetId) -> Option<PathBuf> {
        self.inner
            .lock()
            .expect("assets mutex poisoned")
            .source_path(id)
    }

    #[must_use]
    pub fn metadata(&self, id: AssetId) -> Option<AssetMetadata> {
        self.inner
            .lock()
            .expect("assets mutex poisoned")
            .metadata(id)
    }

    #[must_use]
    pub fn watch_paths(&self, id: AssetId) -> Option<AssetWatchPaths> {
        self.inner
            .lock()
            .expect("assets mutex poisoned")
            .watch_paths(id)
    }

    pub fn resolve_asset_path<T: Asset>(
        &self,
        path: &AssetPath<T>,
    ) -> Result<WeakHandle<T>, AssetError> {
        let path_buf = path.as_path().to_path_buf();
        let inner = self.inner.lock().expect("assets mutex poisoned");
        let id = inner.resolve_typed_source_asset::<T>(&path_buf)?;
        Ok(WeakHandle::new(id))
    }

    pub fn asset_path<T: Asset>(&self, id: AssetId) -> Result<AssetPath<T>, AssetError> {
        let inner = self.inner.lock().expect("assets mutex poisoned");
        inner.typed_asset_path::<T>(id)
    }

    #[must_use]
    pub fn event_cursor(&self) -> AssetEventCursor {
        self.inner
            .lock()
            .expect("assets mutex poisoned")
            .event_cursor()
    }

    pub fn events_since(&self, cursor: &mut AssetEventCursor) -> Vec<AssetEvent> {
        self.inner
            .lock()
            .expect("assets mutex poisoned")
            .events_since(cursor)
    }

    /// Insert an already-built runtime asset that does not come from the cooked manifest.
    pub fn insert_runtime<T: Asset>(&self, asset: T) -> Handle<T> {
        let id = AssetId::new();
        let mut inner = self.inner.lock().expect("assets mutex poisoned");
        inner.insert_runtime(id, asset);
        inner
            .retain_existing_direct_lease(id, Some(TypeId::of::<T>()))
            .expect("runtime asset should exist before retaining its handle lease");
        self.make_handle(id)
    }

    /// Replace an existing runtime asset in-place while keeping its handle stable.
    ///
    /// This is intended for generated runtime content such as streaming video
    /// frames where downstream systems should keep referencing the same handle
    /// while the underlying payload changes.
    pub fn replace_runtime<T: Asset>(
        &self,
        handle: &Handle<T>,
        asset: T,
    ) -> Result<(), AssetError> {
        let mut inner = self.inner.lock().expect("assets mutex poisoned");
        inner.replace_runtime(handle.id(), asset)
    }

    fn make_handle<T: Asset>(&self, id: AssetId) -> Handle<T> {
        let provider: Arc<dyn AssetHandleProvider> = self.inner.clone();
        let lease = Arc::new(AssetLease::new(
            id,
            self.release_tx.clone(),
            Arc::downgrade(&provider),
        ));
        Handle::from_lease(id, lease)
    }
}

impl AssetHandleProvider for Mutex<AssetsInner> {
    fn state_for_handle(&self, id: AssetId) -> AssetState {
        self.lock()
            .expect("assets mutex poisoned")
            .state_for_handle(id)
    }

    fn error_for_handle(&self, id: AssetId) -> Option<AssetError> {
        self.lock()
            .expect("assets mutex poisoned")
            .error_for_handle(id)
    }

    fn get_for_handle(
        &self,
        id: AssetId,
        expected_type: &'static str,
        expected_type_id: TypeId,
    ) -> Result<Arc<dyn Any + Send + Sync>, AssetError> {
        let inner = self.lock().expect("assets mutex poisoned");
        inner.get_for_handle(id, expected_type, expected_type_id)
    }
}

struct AssetsInner {
    config: AssetConfig,
    provider: Box<dyn AssetProvider>,
    registry_loader: Box<dyn AssetRegistryLoader>,
    manifest: LocalManifestRegistry,
    factories: AssetFactories,
    store: AssetStore,
    requests: AssetRequests,
    release_tx: Sender<AssetId>,
    release_rx: Receiver<AssetId>,
    handle_provider: Option<Weak<dyn AssetHandleProvider>>,
    watcher: AssetFileWatcher,
    load_queue: AssetLoadQueue,
    events: AssetEventLog,
    reload: AssetReloadController,
}

impl AssetsInner {
    fn new(
        config: AssetConfig,
        manifest: AssetRegistryManifest,
        release_tx: Sender<AssetId>,
        release_rx: Receiver<AssetId>,
    ) -> Self {
        let provider = Box::new(LocalAssetProvider::new(&config));
        Self::new_with_provider(config, manifest, release_tx, release_rx, provider)
    }

    fn new_with_registry_loader(
        config: AssetConfig,
        manifest: AssetRegistryManifest,
        release_tx: Sender<AssetId>,
        release_rx: Receiver<AssetId>,
        registry_loader: Box<dyn AssetRegistryLoader>,
    ) -> Self {
        let provider = Box::new(LocalAssetProvider::new(&config));
        Self::new_with_provider_and_registry_loader(
            config,
            manifest,
            release_tx,
            release_rx,
            provider,
            registry_loader,
        )
    }

    fn new_with_provider(
        config: AssetConfig,
        manifest: AssetRegistryManifest,
        release_tx: Sender<AssetId>,
        release_rx: Receiver<AssetId>,
        provider: Box<dyn AssetProvider>,
    ) -> Self {
        Self::new_with_provider_and_registry_loader(
            config,
            manifest,
            release_tx,
            release_rx,
            provider,
            Box::new(LocalManifestRegistryLoader),
        )
    }

    fn new_with_provider_and_registry_loader(
        config: AssetConfig,
        manifest: AssetRegistryManifest,
        release_tx: Sender<AssetId>,
        release_rx: Receiver<AssetId>,
        provider: Box<dyn AssetProvider>,
        registry_loader: Box<dyn AssetRegistryLoader>,
    ) -> Self {
        let io_worker_threads = config.io_worker_threads;
        let io_queue_capacity = config.io_queue_capacity;
        let io_shutdown_timeout = config.io_shutdown_timeout;
        let watcher = AssetFileWatcher::new(&config);
        Self {
            config,
            provider,
            registry_loader,
            manifest: LocalManifestRegistry::new(manifest),
            factories: AssetFactories::default(),
            store: AssetStore::default(),
            requests: AssetRequests::default(),
            release_tx,
            release_rx,
            handle_provider: None,
            watcher,
            load_queue: AssetLoadQueue::with_shutdown_timeout(
                io_worker_threads,
                io_queue_capacity,
                io_shutdown_timeout,
            ),
            events: AssetEventLog::default(),
            reload: AssetReloadController::default(),
        }
    }

    fn set_handle_provider(&mut self, handle_provider: Weak<dyn AssetHandleProvider>) {
        self.handle_provider = Some(handle_provider);
    }

    fn set_auto_reload_frozen(&mut self, frozen: bool) {
        self.reload.set_frozen(frozen);
    }

    fn last_reload_report(&self) -> AssetReloadReport {
        self.reload.last_report()
    }

    fn config(&self) -> AssetConfig {
        self.config.clone()
    }

    fn state_for_handle(&self, id: AssetId) -> AssetState {
        self.store.state_for_handle(id)
    }

    fn error_for_handle(&self, id: AssetId) -> Option<AssetError> {
        self.store.error_for_handle(id)
    }

    fn failure_phase(&self, id: AssetId) -> Option<AssetFailurePhase> {
        self.store.failure_phase(id)
    }

    fn get_for_handle(
        &self,
        id: AssetId,
        expected_type: &'static str,
        expected_type_id: TypeId,
    ) -> Result<Arc<dyn Any + Send + Sync>, AssetError> {
        self.store
            .get_for_handle(id, expected_type, expected_type_id)
    }

    fn event_cursor(&self) -> AssetEventCursor {
        self.events.cursor()
    }

    fn events_since(&self, cursor: &mut AssetEventCursor) -> Vec<AssetEvent> {
        self.events.events_since(cursor)
    }

    fn stats(&self) -> AssetStats {
        diagnostics::stats(
            &self.store,
            &self.requests,
            &self.load_queue,
            self.provider.as_ref(),
            &self.events,
            Instant::now(),
        )
    }

    fn queued_request_snapshots(&self) -> Vec<AssetRequestSnapshot> {
        diagnostics::queued_request_snapshots(&self.requests, Instant::now())
    }

    fn active_request_snapshots(&self) -> Vec<AssetRequestSnapshot> {
        diagnostics::active_request_snapshots(
            &self.store,
            &self.requests,
            &self.load_queue,
            Instant::now(),
        )
    }

    fn failed_request_snapshots(&self) -> Vec<AssetRequestSnapshot> {
        diagnostics::failed_request_snapshots(&self.store, &self.requests)
    }

    fn canceled_request_snapshots(&self) -> Vec<AssetRequestSnapshot> {
        diagnostics::canceled_request_snapshots(&self.requests)
    }

    fn failed_asset_snapshots(&self) -> Vec<AssetFailureSnapshot> {
        diagnostics::failed_asset_snapshots(&self.store)
    }

    fn diagnostics_snapshot(&self) -> AssetDiagnosticsSnapshot {
        diagnostics::snapshot(
            &self.config,
            &self.store,
            &self.requests,
            &self.load_queue,
            self.provider.as_ref(),
            &self.events,
            &self.reload,
            Instant::now(),
        )
    }

    fn reload_status(&self) -> AssetReloadStatus {
        diagnostics::reload_status(&self.config, &self.reload, Instant::now())
    }

    fn reload_manifest(&mut self) -> Result<(), AssetError> {
        reload::reload_manifest_records(
            &self.config,
            &mut self.store,
            &mut self.events,
            &mut self.requests,
            self.provider.as_ref(),
            self.registry_loader.as_ref(),
            &mut self.manifest,
            self.config.io_default_priority,
            Instant::now(),
        )
    }

    fn register_factory<F>(&mut self, factory: F)
    where
        F: AssetRuntimeFactory,
    {
        self.factories.register(factory);
    }

    fn resolve_typed_source_asset<T: Asset>(&self, path: &Path) -> Result<AssetId, AssetError> {
        resolve_typed_source_asset::<T, _>(&self.config, &self.manifest, &self.factories, path)
    }

    fn typed_asset_path<T: Asset>(&self, id: AssetId) -> Result<AssetPath<T>, AssetError> {
        typed_asset_path::<T, _>(&self.manifest, &self.factories, id)
    }

    fn manifest_snapshot(&self) -> AssetRegistryManifest {
        super::registry::manifest_snapshot(&self.manifest)
    }

    fn lookup_source_asset(&self, path: &Path) -> Option<AssetId> {
        super::registry::lookup_source_asset(&self.config, &self.manifest, path)
    }

    fn source_path(&self, id: AssetId) -> Option<PathBuf> {
        super::registry::source_path(&self.manifest, id)
    }

    fn metadata(&self, id: AssetId) -> Option<AssetMetadata> {
        super::registry::metadata(&self.manifest, id)
    }

    fn watch_paths(&self, id: AssetId) -> Option<AssetWatchPaths> {
        super::registry::watch_paths(&self.config, &self.manifest, id)
    }

    fn load_texture(&mut self, path_or_key: &Path, priority: i32) -> Result<AssetId, AssetError> {
        lease::acquire_texture_source_lease(
            &self.config,
            &self.manifest,
            &self.factories,
            &mut self.store,
            &mut self.requests,
            path_or_key,
            priority,
            Instant::now(),
        )
    }

    fn load_font(&mut self, path_or_key: &Path, priority: i32) -> Result<AssetId, AssetError> {
        lease::acquire_font_source_lease(
            &self.config,
            &self.manifest,
            &self.factories,
            &mut self.store,
            &mut self.requests,
            path_or_key,
            priority,
            Instant::now(),
        )
    }

    fn acquire_typed_manifest_direct_lease<T: Asset>(
        &mut self,
        id: AssetId,
        priority: i32,
    ) -> Result<(), AssetError> {
        lease::acquire_typed_manifest_direct_lease::<T>(
            &mut self.store,
            &mut self.requests,
            &self.manifest,
            &self.factories,
            id,
            priority,
            Instant::now(),
        )
    }

    fn acquire_typed_manifest_source_lease<T: Asset>(
        &mut self,
        path: &Path,
        priority: i32,
    ) -> Result<AssetId, AssetError> {
        lease::acquire_typed_manifest_source_lease::<T>(
            &self.config,
            &self.manifest,
            &self.factories,
            &mut self.store,
            &mut self.requests,
            path,
            priority,
            Instant::now(),
        )
    }

    fn retain_existing_direct_lease(
        &mut self,
        id: AssetId,
        requested_type: Option<TypeId>,
    ) -> Result<(), AssetError> {
        lease::retain_existing_direct_lease(&mut self.store, id, requested_type)
    }

    fn drain_handle_releases(&mut self) {
        lease::apply_handle_releases(
            &self.release_rx,
            &mut self.store,
            &mut self.load_queue,
            &mut self.events,
        );
    }

    fn reload_changed(&mut self) -> Result<Vec<AssetId>, AssetError> {
        Ok(self.reload_changed_report()?.impacted)
    }

    fn reload_changed_report(&mut self) -> Result<AssetReloadReport, AssetError> {
        reload::reload_changed_with_report(
            &self.config,
            &mut self.store,
            &mut self.events,
            &mut self.requests,
            &mut self.reload,
            self.provider.as_ref(),
            self.registry_loader.as_ref(),
            &mut self.manifest,
            self.config.io_default_priority,
            Instant::now(),
        )
    }

    fn force_reload(&mut self, id: AssetId) -> Result<AssetReloadReport, AssetError> {
        reload::force_reload_root(
            &self.config,
            &mut self.store,
            &mut self.events,
            &mut self.requests,
            &mut self.reload,
            self.provider.as_ref(),
            self.registry_loader.as_ref(),
            &mut self.manifest,
            id,
            self.config.io_default_priority,
            Instant::now(),
        )
    }

    fn maybe_auto_reload(&mut self) -> Result<(), AssetError> {
        let now = Instant::now();
        let watcher_events = self.watcher.drain();
        reload::drive_auto_reload(
            &self.config,
            &mut self.store,
            &mut self.events,
            &mut self.requests,
            &mut self.reload,
            self.provider.as_ref(),
            self.registry_loader.as_ref(),
            &mut self.manifest,
            watcher_events,
            self.config.io_default_priority,
            now,
        )
    }

    fn spawn_load_record(&mut self, id: AssetId) -> Result<bool, AssetError> {
        submit_record_load_or_fail(
            &self.config,
            self.provider.as_ref(),
            &self.manifest,
            &self.factories,
            &mut self.store,
            &mut self.events,
            &mut self.requests,
            &mut self.load_queue,
            id,
            Instant::now(),
        )
    }

    fn drain_load_completions(&mut self) -> Result<usize, AssetError> {
        drain_completed_loads_or_fail(
            &mut self.store,
            &mut self.events,
            &mut self.requests,
            &mut self.load_queue,
            &self.manifest,
            self.config.io_default_priority,
            Instant::now(),
        )
    }

    fn activate_queued_requests(&mut self) -> Result<(), AssetError> {
        super::request::activate_queued_requests(
            &mut self.store,
            &mut self.requests,
            Instant::now(),
        )
    }

    fn refresh_active_requests_after_drive(&mut self) {
        super::request::refresh_active_requests_after_drive(
            &self.store,
            &mut self.requests,
            &mut self.load_queue,
            Instant::now(),
        );
    }

    fn should_load_in_background(&self, id: AssetId) -> bool {
        should_load_record_in_background(&self.config, &self.store, id)
    }

    fn has_current_inflight_load(&self, id: AssetId) -> bool {
        has_current_inflight_load(&self.store, &self.load_queue, id)
    }

    fn apply_update(&mut self) -> Result<(), AssetError> {
        self.drain_handle_releases();
        self.maybe_auto_reload()?;
        self.activate_queued_requests()?;
        let _ = self.drain_load_completions()?;
        let install_budget_per_update = self.config.install_budget_per_update;
        let install_time_budget = self.config.install_time_budget;
        driver::drive_records(
            self,
            AssetDriveMode::Normal {
                install_budget_per_update,
                install_time_budget,
                started_at: Instant::now(),
            },
        )?;

        self.refresh_active_requests_after_drive();
        Ok(())
    }

    fn apply_blocking_update(
        &mut self,
        target_id: AssetId,
        force_synchronous_load: bool,
    ) -> Result<(), AssetError> {
        self.drain_handle_releases();
        self.activate_queued_requests()?;
        let _ = self.drain_load_completions()?;

        driver::drive_records(
            self,
            AssetDriveMode::Blocking {
                target_id,
                force_synchronous_load,
                started_at: Instant::now(),
            },
        )?;

        self.refresh_active_requests_after_drive();
        Ok(())
    }

    fn insert_runtime<T: Asset>(&mut self, id: AssetId, asset: T) {
        runtime::insert_runtime_asset(&mut self.store, &mut self.events, id, asset);
    }

    fn replace_runtime<T: Asset>(&mut self, id: AssetId, asset: T) -> Result<(), AssetError> {
        runtime::replace_runtime_asset(
            &mut self.store,
            &mut self.events,
            &self.manifest,
            id,
            asset,
            self.config.io_default_priority,
        )
    }

    fn load_record(&mut self, id: AssetId) -> Result<(), AssetError> {
        load_record_now_and_apply_or_fail(
            &self.config,
            self.provider.as_ref(),
            &self.manifest,
            &self.factories,
            &mut self.store,
            &mut self.events,
            &mut self.requests,
            &mut self.load_queue,
            id,
            Instant::now(),
        )
    }

    fn evaluate_dependencies(&mut self, id: AssetId) -> Result<Option<AssetState>, AssetError> {
        dependency::resolve_dependencies_or_fail(
            &mut self.store,
            &mut self.events,
            &mut self.requests,
            &self.manifest,
            id,
            self.config.io_default_priority,
            Instant::now(),
        )
        .map(Some)
    }

    fn install_record(
        &mut self,
        id: AssetId,
        budget: AssetInstallBudget,
    ) -> Result<bool, AssetError> {
        install::install_record_or_fail(
            &self.config,
            &self.manifest,
            &self.factories,
            &mut self.store,
            &mut self.events,
            &mut self.requests,
            &self.release_tx,
            self.handle_provider.clone(),
            id,
            budget,
            self.config.io_default_priority,
            Instant::now(),
        )
    }

    fn uninstall_record(&mut self, id: AssetId) -> Result<bool, AssetError> {
        install::uninstall_record_or_fail(
            &self.config,
            &self.manifest,
            &self.factories,
            &mut self.store,
            &mut self.events,
            &mut self.requests,
            id,
            self.config.io_default_priority,
            Instant::now(),
        )
    }
}

impl AssetRecordDriver for AssetsInner {
    fn store(&self) -> &AssetStore {
        &self.store
    }

    fn store_mut(&mut self) -> &mut AssetStore {
        &mut self.store
    }

    fn drain_load_completions(&mut self) -> Result<usize, AssetError> {
        AssetsInner::drain_load_completions(self)
    }

    fn spawn_load_record(&mut self, id: AssetId) -> Result<bool, AssetError> {
        AssetsInner::spawn_load_record(self, id)
    }

    fn load_record(&mut self, id: AssetId) -> Result<(), AssetError> {
        AssetsInner::load_record(self, id)
    }

    fn evaluate_dependencies(&mut self, id: AssetId) -> Result<Option<AssetState>, AssetError> {
        AssetsInner::evaluate_dependencies(self, id)
    }

    fn install_record(
        &mut self,
        id: AssetId,
        budget: AssetInstallBudget,
    ) -> Result<bool, AssetError> {
        AssetsInner::install_record(self, id, budget)
    }

    fn uninstall_record(&mut self, id: AssetId) -> Result<bool, AssetError> {
        AssetsInner::uninstall_record(self, id)
    }

    fn should_load_in_background(&self, id: AssetId) -> bool {
        AssetsInner::should_load_in_background(self, id)
    }

    fn has_current_inflight_load(&self, id: AssetId) -> bool {
        AssetsInner::has_current_inflight_load(self, id)
    }

    fn blocking_relevant_ids(&self, target_id: AssetId) -> Vec<AssetId> {
        dependency::blocking_relevant_ids(&self.store, &self.manifest, target_id)
    }

    fn record_unloaded(&mut self, id: AssetId) {
        events::push_unloaded_event(&mut self.events, &self.store, id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset::provider::{write_test_bundle, MemoryAssetProvider};
    use crate::asset::registry::load_manifest;
    use crate::asset::{
        AssetEventKind, AssetManifestEntry, AssetReloadSkipReason, AssetRequestStatus, LoadedAsset,
        ASSET_SYSTEM_VERSION,
    };
    use std::collections::{HashMap, HashSet};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
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

        fn begin_install(
            &self,
            loaded: &Self::Loaded,
            _ctx: AssetInstallContext<'_>,
        ) -> Result<AssetInstallResult<Self::Asset>, AssetError> {
            Ok(AssetInstallResult::Ready(DummyAsset(loaded.0.clone())))
        }
    }

    struct StaticRegistryLoader {
        manifest: AssetRegistryManifest,
        loads: Arc<AtomicUsize>,
    }

    impl AssetRegistryLoader for StaticRegistryLoader {
        fn load(&self, _config: &AssetConfig) -> Result<AssetRegistryManifest, AssetError> {
            self.loads.fetch_add(1, Ordering::SeqCst);
            Ok(self.manifest.clone())
        }
    }

    #[derive(Clone)]
    struct SlowFactory {
        delay: Duration,
    }

    impl AssetRuntimeFactory for SlowFactory {
        type Asset = DummyAsset;
        type Loaded = LoadedDummy;

        fn load(&self, ctx: AssetLoadContext<'_>) -> Result<LoadedAsset<Self::Loaded>, AssetError> {
            std::thread::sleep(self.delay);
            let text =
                std::str::from_utf8(ctx.bytes).map_err(|error| AssetError::InvalidCookedAsset {
                    id: Some(ctx.asset_id),
                    message: error.to_string(),
                })?;
            Ok(LoadedAsset::new(LoadedDummy(text.to_string()))
                .with_dependencies(ctx.entry.dependencies.clone()))
        }

        fn begin_install(
            &self,
            loaded: &Self::Loaded,
            _ctx: AssetInstallContext<'_>,
        ) -> Result<AssetInstallResult<Self::Asset>, AssetError> {
            Ok(AssetInstallResult::Ready(DummyAsset(loaded.0.clone())))
        }
    }

    struct SlowTextureFactory {
        delay: Duration,
    }

    impl AssetRuntimeFactory for SlowTextureFactory {
        type Asset = TextureAsset;
        type Loaded = TextureAsset;

        fn load(&self, ctx: AssetLoadContext<'_>) -> Result<LoadedAsset<Self::Loaded>, AssetError> {
            std::thread::sleep(self.delay);
            Ok(LoadedAsset::new(TextureAsset::white_pixel())
                .with_dependencies(ctx.entry.dependencies.clone()))
        }

        fn begin_install(
            &self,
            loaded: &Self::Loaded,
            _ctx: AssetInstallContext<'_>,
        ) -> Result<AssetInstallResult<Self::Asset>, AssetError> {
            Ok(AssetInstallResult::Ready(loaded.clone()))
        }
    }

    struct FailingInstallFactory;

    impl AssetRuntimeFactory for FailingInstallFactory {
        type Asset = DummyAsset;
        type Loaded = LoadedDummy;

        fn load(&self, ctx: AssetLoadContext<'_>) -> Result<LoadedAsset<Self::Loaded>, AssetError> {
            let text =
                std::str::from_utf8(ctx.bytes).map_err(|error| AssetError::InvalidCookedAsset {
                    id: Some(ctx.asset_id),
                    message: error.to_string(),
                })?;
            Ok(LoadedAsset::new(LoadedDummy(text.to_string())))
        }

        fn begin_install(
            &self,
            _loaded: &Self::Loaded,
            _ctx: AssetInstallContext<'_>,
        ) -> Result<AssetInstallResult<Self::Asset>, AssetError> {
            Err(AssetError::Unsupported {
                message: "install failed intentionally".to_string(),
            })
        }
    }

    struct FailingUninstallFactory;

    impl AssetRuntimeFactory for FailingUninstallFactory {
        type Asset = DummyAsset;
        type Loaded = LoadedDummy;

        fn load(&self, ctx: AssetLoadContext<'_>) -> Result<LoadedAsset<Self::Loaded>, AssetError> {
            let text =
                std::str::from_utf8(ctx.bytes).map_err(|error| AssetError::InvalidCookedAsset {
                    id: Some(ctx.asset_id),
                    message: error.to_string(),
                })?;
            Ok(LoadedAsset::new(LoadedDummy(text.to_string())))
        }

        fn begin_install(
            &self,
            loaded: &Self::Loaded,
            _ctx: AssetInstallContext<'_>,
        ) -> Result<AssetInstallResult<Self::Asset>, AssetError> {
            Ok(AssetInstallResult::Ready(DummyAsset(loaded.0.clone())))
        }

        fn uninstall(
            &self,
            _installed: &Self::Asset,
            _ctx: AssetUninstallContext<'_>,
        ) -> Result<(), AssetError> {
            Err(AssetError::Unsupported {
                message: "uninstall failed intentionally".to_string(),
            })
        }
    }

    #[derive(Clone)]
    struct DeferredInstallFactory {
        pending_polls: usize,
    }

    impl AssetRuntimeFactory for DeferredInstallFactory {
        type Asset = DummyAsset;
        type Loaded = LoadedDummy;

        fn load(&self, ctx: AssetLoadContext<'_>) -> Result<LoadedAsset<Self::Loaded>, AssetError> {
            let text =
                std::str::from_utf8(ctx.bytes).map_err(|error| AssetError::InvalidCookedAsset {
                    id: Some(ctx.asset_id),
                    message: error.to_string(),
                })?;
            Ok(LoadedAsset::new(LoadedDummy(text.to_string())))
        }

        fn begin_install(
            &self,
            loaded: &Self::Loaded,
            _ctx: AssetInstallContext<'_>,
        ) -> Result<AssetInstallResult<Self::Asset>, AssetError> {
            Ok(AssetInstallResult::Pending(Box::new(DeferredInstallTask {
                value: loaded.0.clone(),
                pending_polls: self.pending_polls,
            })))
        }
    }

    struct DeferredInstallTask {
        value: String,
        pending_polls: usize,
    }

    impl AssetInstallTask for DeferredInstallTask {
        type Output = DummyAsset;

        fn poll_install(
            &mut self,
            _ctx: AssetInstallContext<'_>,
            _budget: AssetInstallBudget,
        ) -> Result<AssetInstallPoll<Self::Output>, AssetError> {
            if self.pending_polls > 0 {
                self.pending_polls -= 1;
                return Ok(AssetInstallPoll::Pending);
            }

            Ok(AssetInstallPoll::Ready(DummyAsset(self.value.clone())))
        }
    }

    #[derive(Clone, Default)]
    struct InvalidationCountingProvider {
        invalidate_all_calls: Arc<AtomicUsize>,
    }

    impl InvalidationCountingProvider {
        fn invalidate_all_calls(&self) -> usize {
            self.invalidate_all_calls.load(Ordering::SeqCst)
        }
    }

    impl AssetProvider for InvalidationCountingProvider {
        fn resolve(
            &self,
            id: AssetId,
            _entry: AssetManifestEntry,
            _raw_source_path: Option<PathBuf>,
        ) -> Result<ResolvedAssetSource, AssetError> {
            Err(AssetError::AssetNotFound { id })
        }

        fn invalidate_all(&self) {
            self.invalidate_all_calls.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[derive(Clone)]
    struct CountingFactory {
        loads: Arc<Mutex<HashMap<AssetId, usize>>>,
        installs: Arc<Mutex<HashMap<AssetId, usize>>>,
        uninstalls: Arc<Mutex<HashMap<AssetId, usize>>>,
    }

    impl CountingFactory {
        fn new() -> Self {
            Self {
                loads: Arc::new(Mutex::new(HashMap::default())),
                installs: Arc::new(Mutex::new(HashMap::default())),
                uninstalls: Arc::new(Mutex::new(HashMap::default())),
            }
        }

        fn load_count(&self, id: AssetId) -> usize {
            *self
                .loads
                .lock()
                .expect("counting loads mutex poisoned")
                .get(&id)
                .unwrap_or(&0)
        }

        fn install_count(&self, id: AssetId) -> usize {
            *self
                .installs
                .lock()
                .expect("counting installs mutex poisoned")
                .get(&id)
                .unwrap_or(&0)
        }

        fn uninstall_count(&self, id: AssetId) -> usize {
            *self
                .uninstalls
                .lock()
                .expect("counting uninstalls mutex poisoned")
                .get(&id)
                .unwrap_or(&0)
        }
    }

    impl AssetRuntimeFactory for CountingFactory {
        type Asset = DummyAsset;
        type Loaded = LoadedDummy;

        fn load(&self, ctx: AssetLoadContext<'_>) -> Result<LoadedAsset<Self::Loaded>, AssetError> {
            let mut loads = self.loads.lock().expect("counting loads mutex poisoned");
            *loads.entry(ctx.asset_id).or_insert(0) += 1;
            drop(loads);

            let text =
                std::str::from_utf8(ctx.bytes).map_err(|error| AssetError::InvalidCookedAsset {
                    id: Some(ctx.asset_id),
                    message: error.to_string(),
                })?;
            Ok(LoadedAsset::new(LoadedDummy(text.to_string()))
                .with_dependencies(ctx.entry.dependencies.clone()))
        }

        fn begin_install(
            &self,
            loaded: &Self::Loaded,
            ctx: AssetInstallContext<'_>,
        ) -> Result<AssetInstallResult<Self::Asset>, AssetError> {
            let mut installs = self
                .installs
                .lock()
                .expect("counting installs mutex poisoned");
            *installs.entry(ctx.asset_id).or_insert(0) += 1;
            Ok(AssetInstallResult::Ready(DummyAsset(loaded.0.clone())))
        }

        fn uninstall(
            &self,
            _installed: &Self::Asset,
            ctx: AssetUninstallContext<'_>,
        ) -> Result<(), AssetError> {
            let mut uninstalls = self
                .uninstalls
                .lock()
                .expect("counting uninstalls mutex poisoned");
            *uninstalls.entry(ctx.asset_id).or_insert(0) += 1;
            Ok(())
        }
    }

    fn write_manifest_entries(
        root: &Path,
        entries: Vec<AssetManifestEntry>,
    ) -> Result<AssetConfig, Box<dyn std::error::Error>> {
        let config = AssetConfig::new(root, "native");
        std::fs::create_dir_all(config.cooked_root())?;
        let manifest = AssetRegistryManifest {
            version: ASSET_SYSTEM_VERSION,
            target: "native".to_string(),
            provenance: Vec::new(),
            assets: entries,
        };
        std::fs::write(
            config.manifest_path(),
            serde_json::to_vec_pretty(&manifest)?,
        )?;
        Ok(config)
    }

    fn write_manifest(
        root: &Path,
        entry: AssetManifestEntry,
    ) -> Result<AssetConfig, Box<dyn std::error::Error>> {
        write_manifest_entries(root, vec![entry])
    }

    #[test]
    fn typed_asset_path_serializes_resolves_and_loads() -> Result<(), Box<dyn std::error::Error>> {
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
        )?
        .with_package_root("packages/base")
        .with_package_file("bundles/base.skybundle");
        let expected_source_path = config.asset_root.join("clip.dummy");
        let expected_cooked_path = config.cooked_root().join("clip.dummyc");
        let expected_package_path = config.asset_root.join("packages/base/clip.dummyc");
        let expected_package_file = config.asset_root.join("bundles/base.skybundle");
        std::fs::write(config.cooked_root().join("clip.dummyc"), b"ready")?;

        let server = Assets::new(config)?;
        server.register_factory(DummyFactory);
        let path = AssetPath::<DummyAsset>::new("clip.dummy");
        let serialized = serde_json::to_string(&path)?;
        assert_eq!(serialized, "\"clip.dummy\"");
        let decoded: AssetPath<DummyAsset> = serde_json::from_str(&serialized)?;
        assert_eq!(decoded.as_path(), Path::new("clip.dummy"));

        assert_eq!(
            server.source_path(asset_id),
            Some(PathBuf::from("clip.dummy"))
        );
        assert_eq!(server.asset_path::<DummyAsset>(asset_id)?, path);
        let metadata = server.metadata(asset_id).expect("metadata should resolve");
        assert_eq!(metadata.asset_id, asset_id);
        assert_eq!(metadata.asset_type, "dummy");
        assert_eq!(metadata.source_path, "clip.dummy");
        assert_eq!(metadata.cooked_path, "clip.dummyc");
        assert!(metadata.dependencies.is_empty());
        let watch_paths = server.watch_paths(asset_id).expect("watch paths");
        assert_eq!(watch_paths.source_path, expected_source_path);
        assert_eq!(watch_paths.cooked_path, expected_cooked_path);
        assert_eq!(watch_paths.package_paths, vec![expected_package_path]);
        assert_eq!(watch_paths.package_files, vec![expected_package_file]);

        let weak = server.resolve_asset_path(&decoded)?;
        assert_eq!(weak.id(), asset_id);
        let handle = server.load_path(&path)?;
        assert_eq!(handle.id(), asset_id);
        server.update()?;
        assert_eq!(server.get(&handle)?.0, "ready");

        let wrong_type_path = AssetPath::<FontAsset>::new("clip.dummy");
        let err = server.resolve_asset_path(&wrong_type_path).unwrap_err();
        assert!(matches!(
            err,
            AssetError::AssetTypeMismatch {
                expected,
                actual,
                ..
            } if expected == FontAsset::TYPE && actual == "dummy"
        ));
        let err = server.asset_path::<FontAsset>(asset_id).unwrap_err();
        assert!(matches!(
            err,
            AssetError::AssetTypeMismatch {
                expected,
                actual,
                ..
            } if expected == FontAsset::TYPE && actual == "dummy"
        ));
        Ok(())
    }

    #[test]
    fn runtime_assets_are_installed_immediately_and_survive_manifest_reload(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        let config = write_manifest_entries(dir.path(), Vec::new())?;
        let server = Assets::new(config)?;
        let mut events = server.event_cursor();
        let handle = server.insert_runtime(DummyAsset("runtime".to_string()));
        let installed_events = server.events_since(&mut events);
        assert_eq!(installed_events.len(), 1);
        assert_eq!(installed_events[0].id, handle.id());
        assert_eq!(installed_events[0].kind, AssetEventKind::Installed);

        assert_eq!(server.state(&handle), AssetState::Installed);
        assert_eq!(server.get(&handle)?.0, "runtime");

        server.reload_manifest()?;
        assert!(server.events_since(&mut events).is_empty());
        assert_eq!(server.state(&handle), AssetState::Installed);
        assert_eq!(server.get(&handle)?.0, "runtime");

        let handle_id = handle.id();
        drop(handle);
        server.update()?;
        let unloaded_events = server.events_since(&mut events);
        assert!(unloaded_events
            .iter()
            .any(|event| { event.id == handle_id && event.kind == AssetEventKind::Unloaded }));
        assert_eq!(server.state_untyped(handle_id), AssetState::Unloaded);
        Ok(())
    }

    #[test]
    fn asset_io_config_controls_worker_pool() {
        let dir = tempdir().expect("temporary asset root");
        let config = AssetConfig::new(dir.path(), "native")
            .with_io_worker_threads(1)
            .with_io_queue_capacity(3)
            .with_io_default_priority(7);
        let server = Assets::with_empty_manifest(config);
        let inner = server.inner.lock().expect("assets mutex poisoned");

        assert_eq!(inner.load_queue.worker_count(), 1);
        assert_eq!(inner.load_queue.queue_capacity(), 3);
        assert_eq!(inner.config.io_default_priority, 7);
    }

    #[test]
    fn queued_request_is_canceled_when_last_lease_is_dropped_before_update(
    ) -> Result<(), Box<dyn std::error::Error>> {
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
                cooked_path: "missing.dummyc".to_string(),
                dependencies: Vec::new(),
                import_settings: serde_json::Value::Null,
            },
        )?;

        let server = Assets::new(config)?;
        server.register_factory(DummyFactory);
        let handle = server.load_id::<DummyAsset>(asset_id)?;
        drop(handle);

        server.update()?;

        assert_eq!(server.state_untyped(asset_id), AssetState::Unloaded);
        let stats = server.stats();
        assert_eq!(stats.submitted_requests, 1);
        assert_eq!(stats.activated_requests, 0);
        assert_eq!(stats.canceled_requests, 1);
        let canceled = server.canceled_request_snapshots();
        assert_eq!(canceled.len(), 1);
        assert_eq!(canceled[0].asset_id, asset_id);
        assert_eq!(canceled[0].status, AssetRequestStatus::Canceled);
        Ok(())
    }

    #[test]
    fn asset_events_include_backend_residency_context() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        let dependency_id = AssetId::new();
        let asset_id = AssetId::new();
        let config = write_manifest_entries(
            dir.path(),
            vec![
                AssetManifestEntry {
                    asset_id: asset_id,
                    asset_type: "dummy".to_string(),
                    importer: "dummy".to_string(),
                    cooker: "dummy".to_string(),
                    version: 1,
                    source_path: "clip.dummy".to_string(),
                    cooked_path: "clip.dummyc".to_string(),
                    dependencies: vec![dependency_id],
                    import_settings: serde_json::Value::Null,
                },
                AssetManifestEntry {
                    asset_id: dependency_id,
                    asset_type: "dummy".to_string(),
                    importer: "dummy".to_string(),
                    cooker: "dummy".to_string(),
                    version: 1,
                    source_path: "dependency.dummy".to_string(),
                    cooked_path: "dependency.dummyc".to_string(),
                    dependencies: Vec::new(),
                    import_settings: serde_json::Value::Null,
                },
            ],
        )?;
        let cooked_path = config.cooked_root().join("clip.dummyc");
        std::fs::write(&cooked_path, b"ready")?;
        std::fs::write(
            config.cooked_root().join("dependency.dummyc"),
            b"dependency",
        )?;

        let server = Assets::new(config)?;
        server.register_factory(DummyFactory);
        let mut events = server.event_cursor();
        let handle = server.load_id::<DummyAsset>(asset_id)?;

        server.update()?;

        let installed_events = server.events_since(&mut events);
        let loaded = installed_events
            .iter()
            .find(|event| event.id == asset_id && event.kind == AssetEventKind::Loaded)
            .expect("parent asset should emit loaded event");
        assert_eq!(loaded.state, AssetState::Loaded);
        assert_eq!(loaded.generation, 1);
        assert_eq!(loaded.asset_type, "dummy");
        assert_eq!(loaded.dependencies, vec![dependency_id]);
        assert!(loaded.manifest_fingerprint.is_some());
        assert!(loaded.content_hash.is_some());
        assert!(!loaded.reload_pending);
        let installed = installed_events
            .iter()
            .find(|event| event.id == asset_id && event.kind == AssetEventKind::Installed)
            .expect("parent asset should emit installed event");
        assert_eq!(installed.state, AssetState::Installed);
        assert_eq!(installed.generation, 1);
        assert_eq!(installed.asset_type, "dummy");
        assert_eq!(installed.dependencies, vec![dependency_id]);
        assert!(installed.manifest_fingerprint.is_some());
        assert!(installed.content_hash.is_some());
        assert!(!installed.reload_pending);

        std::fs::write(&cooked_path, b"fresh")?;
        let report = server.reload_changed_with_report()?;
        assert_eq!(report.changed_roots, vec![asset_id]);
        let reload_events = server.events_since(&mut events);
        let reload = reload_events
            .iter()
            .find(|event| event.id == asset_id && event.kind == AssetEventKind::ReloadQueued)
            .expect("parent asset should emit reload queued event");
        assert_eq!(reload.generation, 2);
        assert_eq!(reload.asset_type, "dummy");
        assert!(reload.content_hash.is_none());
        assert!(reload.reload_pending);

        server.update()?;
        assert_eq!(server.get(&handle)?.0, "fresh");
        let reloaded_events = server.events_since(&mut events);
        let reloaded = reloaded_events
            .iter()
            .find(|event| event.id == asset_id && event.kind == AssetEventKind::Reloaded)
            .expect("parent asset should emit reloaded event");
        assert_eq!(reloaded.state, AssetState::Installed);
        assert_eq!(reloaded.generation, 2);
        assert_eq!(reloaded.asset_type, "dummy");
        assert_eq!(reloaded.dependencies, vec![dependency_id]);
        assert!(reloaded.manifest_fingerprint.is_some());
        assert!(reloaded.content_hash.is_some());
        assert!(!reloaded.reload_pending);
        Ok(())
    }

    #[test]
    fn memory_provider_loads_cooked_asset_without_filesystem_bytes(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let asset_id = AssetId::new();
        let entry = AssetManifestEntry {
            asset_id,
            asset_type: "dummy".to_string(),
            importer: "dummy".to_string(),
            cooker: "dummy".to_string(),
            version: 1,
            source_path: "memory.dummy".to_string(),
            cooked_path: "memory.dummyc".to_string(),
            dependencies: Vec::new(),
            import_settings: serde_json::Value::Null,
        };
        let manifest = AssetRegistryManifest {
            version: ASSET_SYSTEM_VERSION,
            target: "native".to_string(),
            provenance: Vec::new(),
            assets: vec![entry],
        };
        let provider = MemoryAssetProvider::new().with_asset(asset_id, b"from-memory".to_vec());
        let server = Assets::with_manifest_and_provider(
            AssetConfig::new("memory-root", "native"),
            manifest,
            provider,
        );
        server.register_factory(DummyFactory);

        let handle = server.load_id::<DummyAsset>(asset_id)?;
        server.update()?;

        assert_eq!(server.get(&handle)?.0, "from-memory");
        Ok(())
    }

    #[test]
    fn registry_loader_supplies_manifest_without_local_manifest_file(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        let config = AssetConfig::new(dir.path(), "native");
        std::fs::create_dir_all(config.cooked_root())?;
        assert!(matches!(
            load_manifest(&config),
            Err(AssetError::ManifestMissing { .. })
        ));

        let asset_id = AssetId::new();
        let manifest = AssetRegistryManifest {
            version: ASSET_SYSTEM_VERSION,
            target: "native".to_string(),
            provenance: Vec::new(),
            assets: vec![AssetManifestEntry {
                asset_id,
                asset_type: "dummy".to_string(),
                importer: "dummy".to_string(),
                cooker: "dummy".to_string(),
                version: 1,
                source_path: "virtual.dummy".to_string(),
                cooked_path: "virtual.dummyc".to_string(),
                dependencies: Vec::new(),
                import_settings: serde_json::Value::Null,
            }],
        };
        let loads = Arc::new(AtomicUsize::new(0));
        let provider = MemoryAssetProvider::new().with_asset(asset_id, b"from-registry".to_vec());
        let server = Assets::with_registry_loader_and_provider(
            config,
            StaticRegistryLoader {
                manifest,
                loads: loads.clone(),
            },
            provider,
        )?;
        server.register_factory(DummyFactory);

        let handle = server.load_id::<DummyAsset>(asset_id)?;
        server.update()?;
        assert_eq!(server.get(&handle)?.0, "from-registry");
        assert_eq!(loads.load(Ordering::SeqCst), 1);

        server.reload_manifest()?;
        assert_eq!(loads.load(Ordering::SeqCst), 2);
        Ok(())
    }

    #[test]
    fn memory_provider_read_failure_records_failed_request_without_filesystem(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let asset_id = AssetId::new();
        let entry = AssetManifestEntry {
            asset_id,
            asset_type: "dummy".to_string(),
            importer: "dummy".to_string(),
            cooker: "dummy".to_string(),
            version: 1,
            source_path: "memory.dummy".to_string(),
            cooked_path: "memory.dummyc".to_string(),
            dependencies: Vec::new(),
            import_settings: serde_json::Value::Null,
        };
        let manifest = AssetRegistryManifest {
            version: ASSET_SYSTEM_VERSION,
            target: "native".to_string(),
            provenance: Vec::new(),
            assets: vec![entry],
        };
        let read_error = AssetError::Io {
            path: PathBuf::from(format!("memory://{asset_id}")),
            message: "simulated read failure".to_string(),
        };
        let provider = MemoryAssetProvider::new().with_read_error(asset_id, read_error.clone());
        let server = Assets::with_manifest_and_provider(
            AssetConfig::new("memory-root", "native"),
            manifest,
            provider,
        );
        server.register_factory(DummyFactory);

        let handle = server.load_id::<DummyAsset>(asset_id)?;
        let error = server
            .update()
            .expect_err("memory read failure should surface through normal update");

        assert_eq!(error, read_error);
        assert_eq!(server.state(&handle), AssetState::Failed);
        assert_eq!(server.failure_phase(&handle), Some(AssetFailurePhase::Read));
        let failed = server.failed_request_snapshots();
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].asset_id, asset_id);
        assert_eq!(failed[0].status, AssetRequestStatus::Failed);
        assert_eq!(failed[0].generation, 1);
        assert!(failed[0]
            .last_error
            .as_deref()
            .is_some_and(|message| message.contains("simulated read failure")));
        Ok(())
    }

    #[test]
    fn memory_provider_delayed_source_stays_inflight_without_filesystem(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let asset_id = AssetId::new();
        let entry = AssetManifestEntry {
            asset_id,
            asset_type: "dummy".to_string(),
            importer: "dummy".to_string(),
            cooker: "dummy".to_string(),
            version: 1,
            source_path: "memory.dummy".to_string(),
            cooked_path: "memory.dummyc".to_string(),
            dependencies: Vec::new(),
            import_settings: serde_json::Value::Null,
        };
        let manifest = AssetRegistryManifest {
            version: ASSET_SYSTEM_VERSION,
            target: "native".to_string(),
            provenance: Vec::new(),
            assets: vec![entry],
        };
        let provider = MemoryAssetProvider::new().with_delayed_asset(
            asset_id,
            b"delayed-memory".to_vec(),
            Duration::from_millis(60),
        );
        let server = Assets::with_manifest_and_provider(
            AssetConfig::new("memory-root", "native").with_background_loading(true),
            manifest,
            provider,
        );
        server.register_factory(DummyFactory);

        let handle = server.load_id::<DummyAsset>(asset_id)?;
        server.update()?;

        assert_eq!(server.state(&handle), AssetState::Loading);
        assert_eq!(server.stats().inflight_loads, 1);
        let active = server.active_request_snapshots();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].asset_id, asset_id);
        assert_eq!(active[0].status, AssetRequestStatus::Loading);

        std::thread::sleep(Duration::from_millis(90));
        server.update()?;

        assert_eq!(server.state(&handle), AssetState::Installed);
        assert_eq!(server.get(&handle)?.0, "delayed-memory");
        assert_eq!(server.stats().inflight_loads, 0);
        Ok(())
    }

    #[test]
    fn memory_provider_loads_dependency_chain_without_filesystem(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let parent_id = AssetId::new();
        let dependency_id = AssetId::new();
        let manifest = AssetRegistryManifest {
            version: ASSET_SYSTEM_VERSION,
            target: "native".to_string(),
            provenance: Vec::new(),
            assets: vec![
                AssetManifestEntry {
                    asset_id: parent_id,
                    asset_type: "dummy".to_string(),
                    importer: "dummy".to_string(),
                    cooker: "dummy".to_string(),
                    version: 1,
                    source_path: "parent.dummy".to_string(),
                    cooked_path: "parent.dummyc".to_string(),
                    dependencies: vec![dependency_id],
                    import_settings: serde_json::Value::Null,
                },
                AssetManifestEntry {
                    asset_id: dependency_id,
                    asset_type: "dummy".to_string(),
                    importer: "dummy".to_string(),
                    cooker: "dummy".to_string(),
                    version: 1,
                    source_path: "dependency.dummy".to_string(),
                    cooked_path: "dependency.dummyc".to_string(),
                    dependencies: Vec::new(),
                    import_settings: serde_json::Value::Null,
                },
            ],
        };
        let provider = MemoryAssetProvider::new()
            .with_asset(parent_id, b"parent".to_vec())
            .with_asset(dependency_id, b"dependency".to_vec());
        let server = Assets::with_manifest_and_provider(
            AssetConfig::new("memory-root", "native"),
            manifest,
            provider,
        );
        server.register_factory(DummyFactory);

        let parent = server.load_id::<DummyAsset>(parent_id)?;
        server.update()?;

        assert_eq!(server.state(&parent), AssetState::Installed);
        assert_eq!(server.get(&parent)?.0, "parent");
        assert_eq!(server.state_untyped(dependency_id), AssetState::Installed);

        drop(parent);
        server.update()?;

        assert_eq!(server.state_untyped(parent_id), AssetState::Unloaded);
        assert_eq!(server.state_untyped(dependency_id), AssetState::Unloaded);
        Ok(())
    }

    #[test]
    fn package_root_cooked_artifact_loads_through_normal_assets_facade(
    ) -> Result<(), Box<dyn std::error::Error>> {
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
        )?
        .with_package_root("packages/base");
        let package_root = config.asset_root.join("packages/base");
        std::fs::create_dir_all(&package_root)?;
        std::fs::write(package_root.join("clip.dummyc"), b"from-package")?;

        let server = Assets::new(config)?;
        server.register_factory(DummyFactory);
        let handle = server.load_id::<DummyAsset>(asset_id)?;
        server.update()?;

        assert_eq!(server.get(&handle)?.0, "from-package");
        Ok(())
    }

    #[test]
    fn package_file_cooked_artifact_loads_through_normal_assets_facade(
    ) -> Result<(), Box<dyn std::error::Error>> {
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
        )?
        .with_package_file("base.skybundle");
        write_test_bundle(
            &config.asset_root.join("base.skybundle"),
            &[("clip.dummyc", b"from-bundle")],
        )?;

        let server = Assets::new(config)?;
        server.register_factory(DummyFactory);
        let handle = server.load_id::<DummyAsset>(asset_id)?;
        server.update()?;

        assert_eq!(server.get(&handle)?.0, "from-bundle");
        let stats = server.stats();
        assert_eq!(stats.provider.package_files, 1);
        assert_eq!(stats.provider.cached_bundle_indexes, 1);
        assert_eq!(stats.provider.cached_bundle_index_entries, 1);
        Ok(())
    }

    #[test]
    fn force_reload_reads_rewritten_package_file_bundle() -> Result<(), Box<dyn std::error::Error>>
    {
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
        )?
        .with_package_file("base.skybundle");
        let bundle = config.asset_root.join("base.skybundle");
        write_test_bundle(&bundle, &[("clip.dummyc", b"from-bundle")])?;

        let server = Assets::new(config)?;
        server.register_factory(DummyFactory);
        let handle = server.load_id::<DummyAsset>(asset_id)?;
        server.update()?;
        assert_eq!(server.get(&handle)?.0, "from-bundle");

        write_test_bundle(&bundle, &[("clip.dummyc", b"from-updated-bundle")])?;
        let report = server.force_reload(asset_id)?;
        assert_eq!(report.changed_roots, vec![asset_id]);
        server.update()?;

        assert_eq!(server.get(&handle)?.0, "from-updated-bundle");
        Ok(())
    }

    #[test]
    fn replace_runtime_keeps_handle_and_updates_payload() -> Result<(), Box<dyn std::error::Error>>
    {
        let dir = tempdir()?;
        let config = write_manifest_entries(dir.path(), Vec::new())?;
        let server = Assets::new(config)?;
        let mut events = server.event_cursor();
        let handle = server.insert_runtime(DummyAsset("frame-a".to_string()));
        let _ = server.events_since(&mut events);

        server.replace_runtime(&handle, DummyAsset("frame-b".to_string()))?;

        assert_eq!(server.state(&handle), AssetState::Installed);
        assert_eq!(server.get(&handle)?.0, "frame-b");
        let events = server.events_since(&mut events);
        assert!(events
            .iter()
            .any(|event| event.id == handle.id() && event.kind == AssetEventKind::Installed));
        Ok(())
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

        let server = Assets::new(config)?;
        server.register_factory(DummyFactory);
        let handle = server.load_id::<DummyAsset>(asset_id)?;

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

        let server = Assets::new(config)?;
        server.register_factory(DummyFactory);
        let handle = server.load_id::<DummyAsset>(asset_id)?;
        let result = server.update();

        assert!(matches!(
            result,
            Err(AssetError::MissingDependency { id, dependency: dep }) if id == asset_id && dep == dependency
        ));
        assert_eq!(server.state(&handle), AssetState::Failed);
        assert_eq!(
            server.failure_phase(&handle),
            Some(AssetFailurePhase::Dependency)
        );
        Ok(())
    }

    #[test]
    fn install_failure_records_install_phase() -> Result<(), Box<dyn std::error::Error>> {
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

        let server = Assets::new(config)?;
        server.register_factory(FailingInstallFactory);
        let handle = server.load_id::<DummyAsset>(asset_id)?;
        let result = server.update();

        assert!(matches!(result, Err(AssetError::Unsupported { .. })));
        assert_eq!(server.state(&handle), AssetState::Failed);
        assert_eq!(
            server.failure_phase(&handle),
            Some(AssetFailurePhase::Install)
        );
        let failures = server.failed_asset_snapshots();
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].asset_id, asset_id);
        assert_eq!(failures[0].asset_type, "dummy");
        assert_eq!(failures[0].state, AssetState::Failed);
        assert_eq!(failures[0].phase, Some(AssetFailurePhase::Install));
        assert!(matches!(failures[0].error, AssetError::Unsupported { .. }));
        Ok(())
    }

    #[test]
    fn install_precondition_failure_records_install_phase_and_failed_request(
    ) -> Result<(), Box<dyn std::error::Error>> {
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

        let server = Assets::new(config)?;
        server.register_factory(DummyFactory);
        let mut events = server.event_cursor();
        let handle = server.load_id::<DummyAsset>(asset_id)?;
        {
            let mut inner = server.inner.lock().expect("assets mutex poisoned");
            inner
                .store
                .force_missing_loaded_payload_for_install_test(asset_id)?;
        }

        let error = server
            .update()
            .expect_err("missing loaded payload should fail install");
        assert!(matches!(
            error,
            AssetError::InvalidState {
                id,
                state: AssetState::Installing,
                ..
            } if id == asset_id
        ));
        assert_eq!(server.state(&handle), AssetState::Failed);
        assert_eq!(
            server.failure_phase(&handle),
            Some(AssetFailurePhase::Install)
        );

        let failed_events = server.events_since(&mut events);
        let failed = failed_events
            .iter()
            .find(|event| event.id == asset_id && event.kind == AssetEventKind::Failed)
            .expect("install precondition failure should emit a failed event");
        assert_eq!(failed.failure_phase, Some(AssetFailurePhase::Install));

        let failed_requests = server.failed_request_snapshots();
        assert_eq!(failed_requests.len(), 1);
        assert_eq!(failed_requests[0].asset_id, asset_id);
        assert_eq!(failed_requests[0].status, AssetRequestStatus::Failed);
        assert!(failed_requests[0]
            .last_error
            .as_deref()
            .is_some_and(|error| error.contains("missing loaded payload")));
        Ok(())
    }

    #[test]
    fn background_provider_resolve_failure_records_lookup_phase_and_failed_request(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let asset_id = AssetId::new();
        let manifest = AssetRegistryManifest {
            version: ASSET_SYSTEM_VERSION,
            target: "native".to_string(),
            provenance: Vec::new(),
            assets: vec![AssetManifestEntry {
                asset_id,
                asset_type: "dummy".to_string(),
                importer: "dummy".to_string(),
                cooker: "dummy".to_string(),
                version: 1,
                source_path: "clip.dummy".to_string(),
                cooked_path: "clip.dummyc".to_string(),
                dependencies: Vec::new(),
                import_settings: serde_json::Value::Null,
            }],
        };
        let provider = MemoryAssetProvider::new()
            .with_resolve_error(asset_id, AssetError::AssetNotFound { id: asset_id });
        let server = Assets::with_manifest_and_provider(
            AssetConfig::new("memory-root", "native").with_background_loading(true),
            manifest,
            provider,
        );
        server.register_factory(DummyFactory);
        let mut events = server.event_cursor();
        let handle = server.load_id::<DummyAsset>(asset_id)?;

        let error = server
            .update()
            .expect_err("provider resolve failure should fail during background submit");
        assert_eq!(error, AssetError::AssetNotFound { id: asset_id });
        assert_eq!(server.state(&handle), AssetState::Failed);
        assert_eq!(
            server.failure_phase(&handle),
            Some(AssetFailurePhase::Lookup)
        );

        let failed_events = server.events_since(&mut events);
        let failed = failed_events
            .iter()
            .find(|event| event.id == asset_id && event.kind == AssetEventKind::Failed)
            .expect("background submit failure should emit a failed event");
        assert_eq!(failed.failure_phase, Some(AssetFailurePhase::Lookup));
        assert_eq!(failed.state, AssetState::Failed);

        let failed_requests = server.failed_request_snapshots();
        assert_eq!(failed_requests.len(), 1);
        assert_eq!(failed_requests[0].asset_id, asset_id);
        assert_eq!(failed_requests[0].status, AssetRequestStatus::Failed);
        assert!(failed_requests[0]
            .last_error
            .as_deref()
            .is_some_and(|error| error.contains("not found in manifest")));
        Ok(())
    }

    #[test]
    fn load_blocking_provider_resolve_failure_records_lookup_phase_and_failed_request(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let asset_id = AssetId::new();
        let manifest = AssetRegistryManifest {
            version: ASSET_SYSTEM_VERSION,
            target: "native".to_string(),
            provenance: Vec::new(),
            assets: vec![AssetManifestEntry {
                asset_id,
                asset_type: "dummy".to_string(),
                importer: "dummy".to_string(),
                cooker: "dummy".to_string(),
                version: 1,
                source_path: "clip.dummy".to_string(),
                cooked_path: "clip.dummyc".to_string(),
                dependencies: Vec::new(),
                import_settings: serde_json::Value::Null,
            }],
        };
        let provider = MemoryAssetProvider::new()
            .with_resolve_error(asset_id, AssetError::AssetNotFound { id: asset_id });
        let server = Assets::with_manifest_and_provider(
            AssetConfig::new("memory-root", "native").with_background_loading(true),
            manifest,
            provider,
        );
        server.register_factory(DummyFactory);
        let mut events = server.event_cursor();

        let error = server
            .load_blocking::<DummyAsset>(asset_id)
            .expect_err("blocking provider resolve failure should fail");
        assert_eq!(error, AssetError::AssetNotFound { id: asset_id });
        assert_eq!(server.state_untyped(asset_id), AssetState::Failed);
        assert_eq!(
            server.failure_phase_untyped(asset_id),
            Some(AssetFailurePhase::Lookup)
        );

        let failed_events = server.events_since(&mut events);
        let failed = failed_events
            .iter()
            .find(|event| event.id == asset_id && event.kind == AssetEventKind::Failed)
            .expect("blocking provider resolve failure should emit failed event");
        assert_eq!(failed.failure_phase, Some(AssetFailurePhase::Lookup));
        assert_eq!(failed.state, AssetState::Failed);

        let failed_requests = server.failed_request_snapshots();
        assert_eq!(failed_requests.len(), 1);
        assert_eq!(failed_requests[0].asset_id, asset_id);
        assert_eq!(failed_requests[0].status, AssetRequestStatus::Failed);
        assert!(failed_requests[0]
            .last_error
            .as_deref()
            .is_some_and(|error| error.contains("not found in manifest")));
        Ok(())
    }

    #[test]
    fn load_blocking_provider_read_failure_records_read_phase_and_failed_request(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let asset_id = AssetId::new();
        let manifest = AssetRegistryManifest {
            version: ASSET_SYSTEM_VERSION,
            target: "native".to_string(),
            provenance: Vec::new(),
            assets: vec![AssetManifestEntry {
                asset_id,
                asset_type: "dummy".to_string(),
                importer: "dummy".to_string(),
                cooker: "dummy".to_string(),
                version: 1,
                source_path: "clip.dummy".to_string(),
                cooked_path: "clip.dummyc".to_string(),
                dependencies: Vec::new(),
                import_settings: serde_json::Value::Null,
            }],
        };
        let read_error = AssetError::Io {
            path: PathBuf::from(format!("memory://{asset_id}")),
            message: "blocking read failed".to_string(),
        };
        let provider = MemoryAssetProvider::new().with_read_error(asset_id, read_error.clone());
        let server = Assets::with_manifest_and_provider(
            AssetConfig::new("memory-root", "native").with_background_loading(true),
            manifest,
            provider,
        );
        server.register_factory(DummyFactory);
        let mut events = server.event_cursor();

        let error = server
            .load_blocking::<DummyAsset>(asset_id)
            .expect_err("blocking provider read failure should fail");
        assert_eq!(error, read_error);
        assert_eq!(server.state_untyped(asset_id), AssetState::Failed);
        assert_eq!(
            server.failure_phase_untyped(asset_id),
            Some(AssetFailurePhase::Read)
        );
        assert_eq!(server.stats().load_timings.failed_source_loads, 1);

        let failed_events = server.events_since(&mut events);
        let failed = failed_events
            .iter()
            .find(|event| event.id == asset_id && event.kind == AssetEventKind::Failed)
            .expect("blocking provider read failure should emit failed event");
        assert_eq!(failed.failure_phase, Some(AssetFailurePhase::Read));
        assert_eq!(failed.state, AssetState::Failed);

        let failed_requests = server.failed_request_snapshots();
        assert_eq!(failed_requests.len(), 1);
        assert_eq!(failed_requests[0].asset_id, asset_id);
        assert_eq!(failed_requests[0].status, AssetRequestStatus::Failed);
        assert!(failed_requests[0]
            .last_error
            .as_deref()
            .is_some_and(|error| error.contains("blocking read failed")));
        Ok(())
    }

    #[test]
    fn deferred_install_task_advances_across_updates() -> Result<(), Box<dyn std::error::Error>> {
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

        let server = Assets::new(config)?;
        server.register_factory(DeferredInstallFactory { pending_polls: 1 });
        let handle = server.load_id::<DummyAsset>(asset_id)?;

        server.update()?;
        assert_eq!(server.state(&handle), AssetState::Installing);
        assert!(server.try_get(&handle).is_none());

        server.update()?;
        assert_eq!(server.state(&handle), AssetState::Installing);
        assert!(server.try_get(&handle).is_none());

        server.update()?;
        assert_eq!(server.state(&handle), AssetState::Installed);
        assert_eq!(server.get(&handle)?.0, "ready");
        Ok(())
    }

    #[test]
    fn install_time_budget_can_defer_install_task_polling() -> Result<(), Box<dyn std::error::Error>>
    {
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
        )?
        .with_install_time_budget(Duration::ZERO);
        std::fs::write(config.cooked_root().join("clip.dummyc"), b"ready")?;

        let server = Assets::new(config)?;
        server.register_factory(DeferredInstallFactory { pending_polls: 0 });
        let handle = server.load_id::<DummyAsset>(asset_id)?;

        server.update()?;
        assert_eq!(server.state(&handle), AssetState::Installing);
        assert!(server.try_get(&handle).is_none());

        let asset = server.load_blocking::<DummyAsset>(asset_id)?;
        assert_eq!(asset.0, "ready");
        assert_eq!(server.state(&handle), AssetState::Installed);
        Ok(())
    }

    #[test]
    fn load_blocking_waits_for_deferred_install_task() -> Result<(), Box<dyn std::error::Error>> {
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

        let server = Assets::new(config)?;
        server.register_factory(DeferredInstallFactory { pending_polls: 2 });

        let asset = server.load_blocking::<DummyAsset>(asset_id)?;

        assert_eq!(asset.0, "ready");
        assert_eq!(server.state_untyped(asset_id), AssetState::Installed);
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
        let server = Assets::new(config)?;
        let result = server.load_id::<DummyAsset>(asset_id);
        assert!(matches!(result, Err(AssetError::AssetTypeMismatch { .. })));
        Ok(())
    }

    #[test]
    fn cooked_runtime_load_reports_schema_mismatch_before_decode(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        let asset_id = AssetId::new();
        let config = write_manifest(
            dir.path(),
            AssetManifestEntry {
                asset_id,
                asset_type: FontAsset::TYPE.to_string(),
                importer: "font.raw".to_string(),
                cooker: "font.legacy".to_string(),
                version: 99,
                source_path: "ui.ttf".to_string(),
                cooked_path: "ui.skyasset".to_string(),
                dependencies: Vec::new(),
                import_settings: serde_json::Value::Null,
            },
        )?;
        std::fs::write(
            config.cooked_root().join("ui.skyasset"),
            crate::asset::font::encode_font_cooked(&FontAsset::new(vec![1, 2, 3, 4])),
        )?;

        let server = Assets::new(config)?;
        let error = server
            .load_blocking::<FontAsset>(asset_id)
            .expect_err("schema mismatch should fail before font decode");

        match error {
            AssetError::CookedSchemaMismatch {
                id,
                expected_cooker,
                expected_version,
                actual_cooker,
                actual_version,
            } => {
                assert_eq!(id, asset_id);
                assert_eq!(expected_cooker, "font.raw_bytes");
                assert_eq!(expected_version, 1);
                assert_eq!(actual_cooker, "font.legacy");
                assert_eq!(actual_version, 99);
            }
            other => panic!("unexpected error: {other:?}"),
        }
        assert_eq!(
            server.failure_phase_untyped(asset_id),
            Some(AssetFailurePhase::Decode)
        );
        Ok(())
    }

    #[test]
    fn dropping_last_handle_returns_asset_to_unloaded_state(
    ) -> Result<(), Box<dyn std::error::Error>> {
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

        let server = Assets::new(config)?;
        server.register_factory(DummyFactory);
        let handle = server.load_id::<DummyAsset>(asset_id)?;
        server.update()?;

        let handle_id = handle.id();
        drop(handle);
        server.update()?;
        assert_eq!(server.state_untyped(handle_id), AssetState::Unloaded);
        Ok(())
    }

    #[test]
    fn queued_release_from_old_generation_preserves_reacquired_weak_handle(
    ) -> Result<(), Box<dyn std::error::Error>> {
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
        let cooked_path = config.cooked_root().join("clip.dummyc");
        std::fs::write(&cooked_path, b"old")?;

        let server = Assets::new(config)?;
        server.register_factory(DummyFactory);
        let handle = server.load_id::<DummyAsset>(asset_id)?;
        server.update()?;
        assert_eq!(server.get(&handle)?.0, "old");

        let weak = handle.downgrade();
        drop(handle);
        std::fs::write(&cooked_path, b"new")?;
        let report = server.force_reload(asset_id)?;
        assert_eq!(report.changed_roots, vec![asset_id]);

        let reacquired = server.load_handle(weak)?;
        server.update()?;
        assert_eq!(server.state(&reacquired), AssetState::Installed);
        assert_eq!(server.get(&reacquired)?.0, "new");

        drop(reacquired);
        server.update()?;
        assert_eq!(server.state_untyped(asset_id), AssetState::Unloaded);
        Ok(())
    }

    #[test]
    fn dropping_last_handle_calls_factory_uninstall_before_unload(
    ) -> Result<(), Box<dyn std::error::Error>> {
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

        let server = Assets::new(config)?;
        let factory = CountingFactory::new();
        server.register_factory(factory.clone());
        let mut events = server.event_cursor();
        let handle = server.load_id::<DummyAsset>(asset_id)?;
        server.update()?;
        let _ = server.events_since(&mut events);

        drop(handle);
        server.update()?;

        assert_eq!(factory.uninstall_count(asset_id), 1);
        assert_eq!(server.state_untyped(asset_id), AssetState::Unloaded);
        let emitted = server.events_since(&mut events);
        assert!(emitted
            .iter()
            .any(|event| event.id == asset_id && event.kind == AssetEventKind::Unloaded));
        Ok(())
    }

    #[test]
    fn uninstall_failure_records_phase_and_failed_event() -> Result<(), Box<dyn std::error::Error>>
    {
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

        let server = Assets::new(config)?;
        server.register_factory(FailingUninstallFactory);
        let mut events = server.event_cursor();
        let handle = server.load_id::<DummyAsset>(asset_id)?;
        server.update()?;
        let _ = server.events_since(&mut events);

        drop(handle);
        let error = server
            .update()
            .expect_err("uninstall hook failure should surface from update");
        assert!(matches!(error, AssetError::Unsupported { .. }));
        assert_eq!(
            server.failure_phase_untyped(asset_id),
            Some(AssetFailurePhase::Uninstall)
        );

        let failed_events = server.events_since(&mut events);
        let failed = failed_events
            .iter()
            .find(|event| event.id == asset_id && event.kind == AssetEventKind::Failed)
            .expect("failed uninstall should emit a failed event");
        assert_eq!(failed.failure_phase, Some(AssetFailurePhase::Uninstall));
        assert_eq!(failed.state, AssetState::Failed);
        Ok(())
    }

    #[test]
    fn multiple_loads_hold_independent_leases() -> Result<(), Box<dyn std::error::Error>> {
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

        let server = Assets::new(config)?;
        server.register_factory(DummyFactory);
        let first = server.load_id::<DummyAsset>(asset_id)?;
        let second = server.load_id::<DummyAsset>(asset_id)?;
        server.update()?;

        drop(first);
        server.update()?;
        assert_eq!(server.state(&second), AssetState::Installed);
        assert_eq!(server.get(&second)?.0, "ready");

        let second_id = second.id();
        drop(second);
        server.update()?;
        assert_eq!(server.state_untyped(second_id), AssetState::Unloaded);
        Ok(())
    }

    #[test]
    fn same_frame_drop_and_reacquire_keeps_latest_lease_alive(
    ) -> Result<(), Box<dyn std::error::Error>> {
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

        let server = Assets::new(config)?;
        server.register_factory(DummyFactory);
        let first = server.load_id::<DummyAsset>(asset_id)?;
        drop(first);
        let second = server.load_id::<DummyAsset>(asset_id)?;

        server.update()?;

        assert_eq!(server.state(&second), AssetState::Installed);
        assert_eq!(server.get(&second)?.0, "ready");
        assert_eq!(server.stats().strong_references, 1);

        let second_id = second.id();
        drop(second);
        server.update()?;
        assert_eq!(server.state_untyped(second_id), AssetState::Unloaded);
        Ok(())
    }

    #[test]
    fn dependencies_load_transitively_and_release_with_parent(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        let parent_id = AssetId::new();
        let dependency_id = AssetId::new();
        let config = write_manifest_entries(
            dir.path(),
            vec![
                AssetManifestEntry {
                    asset_id: parent_id,
                    asset_type: "dummy".to_string(),
                    importer: "dummy".to_string(),
                    cooker: "dummy".to_string(),
                    version: 1,
                    source_path: "parent.dummy".to_string(),
                    cooked_path: "parent.dummyc".to_string(),
                    dependencies: vec![dependency_id],
                    import_settings: serde_json::Value::Null,
                },
                AssetManifestEntry {
                    asset_id: dependency_id,
                    asset_type: "dummy".to_string(),
                    importer: "dummy".to_string(),
                    cooker: "dummy".to_string(),
                    version: 1,
                    source_path: "dependency.dummy".to_string(),
                    cooked_path: "dependency.dummyc".to_string(),
                    dependencies: Vec::new(),
                    import_settings: serde_json::Value::Null,
                },
            ],
        )?;
        std::fs::write(config.cooked_root().join("parent.dummyc"), b"parent")?;
        std::fs::write(
            config.cooked_root().join("dependency.dummyc"),
            b"dependency",
        )?;

        let server = Assets::new(config)?;
        server.register_factory(DummyFactory);
        let parent = server.load_id::<DummyAsset>(parent_id)?;
        server.update()?;

        assert_eq!(server.state(&parent), AssetState::Installed);
        assert_eq!(server.state_untyped(dependency_id), AssetState::Installed);

        drop(parent);
        server.update()?;
        assert_eq!(server.state_untyped(parent_id), AssetState::Unloaded);
        assert_eq!(server.state_untyped(dependency_id), AssetState::Unloaded);
        Ok(())
    }

    #[test]
    fn direct_dependency_request_survives_parent_drop() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        let parent_id = AssetId::new();
        let dependency_id = AssetId::new();
        let config = write_manifest_entries(
            dir.path(),
            vec![
                AssetManifestEntry {
                    asset_id: parent_id,
                    asset_type: "dummy".to_string(),
                    importer: "dummy".to_string(),
                    cooker: "dummy".to_string(),
                    version: 1,
                    source_path: "parent.dummy".to_string(),
                    cooked_path: "parent.dummyc".to_string(),
                    dependencies: vec![dependency_id],
                    import_settings: serde_json::Value::Null,
                },
                AssetManifestEntry {
                    asset_id: dependency_id,
                    asset_type: "dummy".to_string(),
                    importer: "dummy".to_string(),
                    cooker: "dummy".to_string(),
                    version: 1,
                    source_path: "dependency.dummy".to_string(),
                    cooked_path: "dependency.dummyc".to_string(),
                    dependencies: Vec::new(),
                    import_settings: serde_json::Value::Null,
                },
            ],
        )?;
        std::fs::write(config.cooked_root().join("parent.dummyc"), b"parent")?;
        std::fs::write(
            config.cooked_root().join("dependency.dummyc"),
            b"dependency",
        )?;

        let server = Assets::new(config)?;
        server.register_factory(DummyFactory);
        let parent = server.load_id::<DummyAsset>(parent_id)?;
        let dependency = server.load_id::<DummyAsset>(dependency_id)?;
        server.update()?;

        drop(parent);
        server.update()?;
        assert_eq!(server.state_untyped(parent_id), AssetState::Unloaded);
        assert_eq!(server.state(&dependency), AssetState::Installed);

        let dependency_id = dependency.id();
        drop(dependency);
        server.update()?;
        assert_eq!(server.state_untyped(dependency_id), AssetState::Unloaded);
        Ok(())
    }

    #[test]
    fn load_uses_manifest_source_lookup() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        let asset_root = dir.path().join("assets");
        std::fs::create_dir_all(asset_root.join("nested"))?;

        let asset_id = AssetId::new();
        let config = write_manifest_entries(
            &asset_root,
            vec![AssetManifestEntry {
                asset_id,
                asset_type: "dummy".to_string(),
                importer: "dummy".to_string(),
                cooker: "dummy".to_string(),
                version: 1,
                source_path: "nested/clip.dummy".to_string(),
                cooked_path: "clip.dummyc".to_string(),
                dependencies: Vec::new(),
                import_settings: serde_json::Value::Null,
            }],
        )?;
        std::fs::write(config.cooked_root().join("clip.dummyc"), b"ready")?;

        let server = Assets::new(config)?;
        server.register_factory(DummyFactory);
        let handle = server.load::<DummyAsset>(asset_root.join("nested").join("clip.dummy"))?;
        server.update()?;

        assert_eq!(handle.id(), asset_id);
        assert_eq!(server.get(&handle)?.0, "ready");
        Ok(())
    }

    #[test]
    fn load_font_installs_raw_font_bytes() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        std::fs::write(dir.path().join("title.ttf"), b"fake-font")?;

        let server = Assets::with_empty_manifest(AssetConfig::new(dir.path(), "native"));
        let handle = server.load_font("title.ttf")?;
        for _ in 0..16 {
            server.update()?;
            if server.is_installed(&handle) {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }

        assert_eq!(server.get(&handle)?.bytes(), b"fake-font");
        Ok(())
    }

    #[test]
    fn load_font_by_asset_id_reads_cooked_font() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        let asset_id = AssetId::new();
        let config = write_manifest_entries(
            dir.path(),
            vec![AssetManifestEntry {
                asset_id,
                asset_type: FontAsset::TYPE.to_string(),
                importer: "font.raw".to_string(),
                cooker: "font.raw_bytes".to_string(),
                version: 1,
                source_path: "ui/title.ttf".to_string(),
                cooked_path: "title.skyfont".to_string(),
                dependencies: Vec::new(),
                import_settings: serde_json::Value::Null,
            }],
        )?;
        let cooked = super::super::font::encode_font_cooked(&FontAsset::new(Arc::<[u8]>::from(
            b"cooked-font".to_vec(),
        )));
        std::fs::write(config.cooked_root().join("title.skyfont"), cooked)?;

        let server = Assets::new(config)?;
        let handle = server.load_id::<FontAsset>(asset_id)?;
        server.update()?;

        assert_eq!(server.get(&handle)?.bytes(), b"cooked-font");
        Ok(())
    }

    #[test]
    fn load_texture_deduplicates_raw_paths() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        image::save_buffer(
            dir.path().join("white.png"),
            &[255, 255, 255, 255],
            1,
            1,
            image::ColorType::Rgba8,
        )?;

        let server = Assets::with_empty_manifest(AssetConfig::new(dir.path(), "native"));
        let first = server.load_texture("white.png")?;
        let second = server.load_texture(dir.path().join("white.png"))?;
        assert_eq!(first, second);

        wait_for_terminal_texture(&server, &first)?;
        assert_eq!(server.state(&first), AssetState::Installed);
        assert_eq!(server.get(&first)?.size(), [1, 1]);
        Ok(())
    }

    #[test]
    fn per_request_priority_is_used_for_manifest_and_raw_loads(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        image::save_buffer(
            dir.path().join("white.png"),
            &[255, 255, 255, 255],
            1,
            1,
            image::ColorType::Rgba8,
        )?;

        let manifest_id = AssetId::new();
        let config = write_manifest(
            dir.path(),
            AssetManifestEntry {
                asset_id: manifest_id,
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

        let server = Assets::new(config)?;
        server.register_factory(DummyFactory);
        let manifest = server.load_id_with_priority::<DummyAsset>(manifest_id, 21)?;
        let raw = server.load_texture_with_priority("white.png", -3)?;

        let snapshots = server.queued_request_snapshots();
        let manifest_snapshot = snapshots
            .iter()
            .find(|snapshot| snapshot.asset_id == manifest.id())
            .expect("manifest request snapshot");
        assert_eq!(manifest_snapshot.priority, 21);
        let raw_snapshot = snapshots
            .iter()
            .find(|snapshot| snapshot.asset_id == raw.id())
            .expect("raw texture request snapshot");
        assert_eq!(raw_snapshot.priority, -3);
        Ok(())
    }

    #[test]
    fn load_texture_installs_raw_texture_metadata() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        let pixels = [
            0, 0, 0, 0, 255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255, 255,
            255,
        ];
        image::save_buffer(
            dir.path().join("pose.png"),
            &pixels,
            3,
            2,
            image::ColorType::Rgba8,
        )?;

        let server = Assets::with_empty_manifest(AssetConfig::new(dir.path(), "native"));
        let handle = server.load_texture("pose.png")?;
        wait_for_terminal_texture(&server, &handle)?;

        let texture = server.get(&handle)?;
        assert_eq!(texture.size(), [3, 2]);
        assert_eq!(texture.visible_rect(), [1, 0, 2, 2]);
        Ok(())
    }

    #[test]
    fn load_texture_missing_raw_path_fails_without_panic() -> Result<(), Box<dyn std::error::Error>>
    {
        let dir = tempdir()?;
        let server = Assets::with_empty_manifest(AssetConfig::new(dir.path(), "native"));
        let handle = server.load_texture("missing.png")?;

        wait_for_terminal_texture(&server, &handle)?;
        assert_eq!(server.state(&handle), AssetState::Failed);
        assert!(matches!(server.error(&handle), Some(AssetError::Io { .. })));
        assert_eq!(server.failure_phase(&handle), Some(AssetFailurePhase::Read));
        let failed_requests = server.failed_request_snapshots();
        assert_eq!(failed_requests.len(), 1);
        assert_eq!(failed_requests[0].asset_id, handle.id());
        assert_eq!(
            failed_requests[0].status,
            crate::asset::AssetRequestStatus::Failed
        );
        assert!(failed_requests[0]
            .last_error
            .as_deref()
            .is_some_and(|error| error.contains("I/O error")));
        Ok(())
    }

    #[test]
    fn load_texture_returns_before_raw_decode_completes() -> Result<(), Box<dyn std::error::Error>>
    {
        let dir = tempdir()?;
        image::save_buffer(
            dir.path().join("white.png"),
            &[255, 255, 255, 255],
            1,
            1,
            image::ColorType::Rgba8,
        )?;

        let server = Assets::with_empty_manifest(AssetConfig::new(dir.path(), "native"));
        let handle = server.load_texture("white.png")?;

        assert_ne!(server.state(&handle), AssetState::Installed);
        wait_for_terminal_texture(&server, &handle)?;
        assert_eq!(server.state(&handle), AssetState::Installed);
        Ok(())
    }

    #[test]
    fn load_texture_retries_failed_raw_path_when_requested_again(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        let path = dir.path().join("late.png");
        let server = Assets::with_empty_manifest(AssetConfig::new(dir.path(), "native"));
        let handle = server.load_texture("late.png")?;

        wait_for_terminal_texture(&server, &handle)?;
        assert_eq!(server.state(&handle), AssetState::Failed);

        image::save_buffer(&path, &[255, 255, 255, 255], 1, 1, image::ColorType::Rgba8)?;
        let retry = server.load_texture("late.png")?;
        assert_eq!(retry, handle);
        wait_for_terminal_texture(&server, &retry)?;

        assert_eq!(server.state(&retry), AssetState::Installed);
        assert_eq!(server.failure_phase(&retry), None);
        assert_eq!(server.get(&retry)?.size(), [1, 1]);
        Ok(())
    }

    #[test]
    fn load_texture_reuses_raw_handle_after_unload() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        image::save_buffer(
            dir.path().join("white.png"),
            &[255, 255, 255, 255],
            1,
            1,
            image::ColorType::Rgba8,
        )?;

        let server = Assets::with_empty_manifest(AssetConfig::new(dir.path(), "native"));
        let first = server.load_texture("white.png")?;
        wait_for_terminal_texture(&server, &first)?;
        let first_id = first.id();
        drop(first);
        server.update()?;
        assert_eq!(server.state_untyped(first_id), AssetState::Unloaded);

        let second = server.load_texture("white.png")?;
        assert_eq!(second.id(), first_id);
        wait_for_terminal_texture(&server, &second)?;
        assert_eq!(server.state(&second), AssetState::Installed);
        Ok(())
    }

    #[test]
    fn reload_manifest_refreshes_source_lookup() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        let asset_root = dir.path().join("assets");
        std::fs::create_dir_all(&asset_root)?;

        let config = AssetConfig::new(&asset_root, "native");
        std::fs::create_dir_all(config.cooked_root())?;
        std::fs::write(
            config.manifest_path(),
            serde_json::to_vec_pretty(&AssetRegistryManifest::default())?,
        )?;

        let server = Assets::new(config.clone())?;
        assert_eq!(server.resolve_path(asset_root.join("late.dummy")), None);

        let asset_id = AssetId::new();
        std::fs::write(
            config.manifest_path(),
            serde_json::to_vec_pretty(&AssetRegistryManifest {
                version: ASSET_SYSTEM_VERSION,
                target: "native".to_string(),
                provenance: Vec::new(),
                assets: vec![AssetManifestEntry {
                    asset_id,
                    asset_type: "dummy".to_string(),
                    importer: "dummy".to_string(),
                    cooker: "dummy".to_string(),
                    version: 1,
                    source_path: "late.dummy".to_string(),
                    cooked_path: "late.dummyc".to_string(),
                    dependencies: Vec::new(),
                    import_settings: serde_json::Value::Null,
                }],
            })?,
        )?;

        server.reload_manifest()?;
        assert_eq!(
            server.resolve_path(asset_root.join("late.dummy")),
            Some(asset_id)
        );
        Ok(())
    }

    #[test]
    fn reload_manifest_invalidates_provider_package_cache() -> Result<(), Box<dyn std::error::Error>>
    {
        let dir = tempdir()?;
        let config = write_manifest_entries(dir.path(), Vec::new())?;
        let provider = InvalidationCountingProvider::default();
        let server = Assets::with_manifest_and_provider(
            config.clone(),
            AssetRegistryManifest::default(),
            provider.clone(),
        );
        assert_eq!(provider.invalidate_all_calls(), 0);

        std::fs::write(
            config.manifest_path(),
            serde_json::to_vec_pretty(&AssetRegistryManifest {
                version: ASSET_SYSTEM_VERSION,
                target: "native".to_string(),
                provenance: Vec::new(),
                assets: Vec::new(),
            })?,
        )?;

        server.reload_manifest()?;

        assert_eq!(provider.invalidate_all_calls(), 1);
        Ok(())
    }

    #[test]
    fn reload_scan_and_force_reload_invalidate_provider_once_per_manifest_refresh(
    ) -> Result<(), Box<dyn std::error::Error>> {
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
        let provider = InvalidationCountingProvider::default();
        let server = Assets::with_manifest_and_provider(
            config.clone(),
            load_manifest(&config)?,
            provider.clone(),
        );
        server.register_factory(DummyFactory);
        let _handle = server.load_id::<DummyAsset>(asset_id)?;

        let report = server.reload_changed_with_report()?;
        assert!(report.changed_roots.is_empty());
        assert_eq!(provider.invalidate_all_calls(), 1);

        let report = server.force_reload(asset_id)?;
        assert_eq!(report.changed_roots, vec![asset_id]);
        assert_eq!(provider.invalidate_all_calls(), 2);
        Ok(())
    }

    fn wait_for_terminal_texture(
        server: &Assets,
        handle: &Handle<TextureAsset>,
    ) -> Result<(), AssetError> {
        for _ in 0..64 {
            let _ = server.update();
            match server.state(handle) {
                AssetState::Installed | AssetState::Failed => return Ok(()),
                _ => std::thread::sleep(Duration::from_millis(5)),
            }
        }
        Err(AssetError::InvalidState {
            id: handle.id(),
            state: server.state(handle),
            message: "raw texture did not reach a terminal state".to_string(),
        })
    }

    fn wait_for_terminal_dummy(
        server: &Assets,
        handle: &Handle<DummyAsset>,
    ) -> Result<(), AssetError> {
        for _ in 0..64 {
            let _ = server.update();
            match server.state(handle) {
                AssetState::Installed | AssetState::Failed => return Ok(()),
                _ => std::thread::sleep(Duration::from_millis(5)),
            }
        }
        Err(AssetError::InvalidState {
            id: handle.id(),
            state: server.state(handle),
            message: "dummy asset did not reach a terminal state".to_string(),
        })
    }

    #[test]
    fn dependency_cycle_marks_asset_failed() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        let first_id = AssetId::new();
        let second_id = AssetId::new();
        let config = write_manifest_entries(
            dir.path(),
            vec![
                AssetManifestEntry {
                    asset_id: first_id,
                    asset_type: "dummy".to_string(),
                    importer: "dummy".to_string(),
                    cooker: "dummy".to_string(),
                    version: 1,
                    source_path: "first.dummy".to_string(),
                    cooked_path: "first.dummyc".to_string(),
                    dependencies: vec![second_id],
                    import_settings: serde_json::Value::Null,
                },
                AssetManifestEntry {
                    asset_id: second_id,
                    asset_type: "dummy".to_string(),
                    importer: "dummy".to_string(),
                    cooker: "dummy".to_string(),
                    version: 1,
                    source_path: "second.dummy".to_string(),
                    cooked_path: "second.dummyc".to_string(),
                    dependencies: vec![first_id],
                    import_settings: serde_json::Value::Null,
                },
            ],
        )?;
        std::fs::write(config.cooked_root().join("first.dummyc"), b"first")?;
        std::fs::write(config.cooked_root().join("second.dummyc"), b"second")?;

        let server = Assets::new(config)?;
        server.register_factory(DummyFactory);
        let handle = server.load_id::<DummyAsset>(first_id)?;
        let result = server.update();

        assert!(matches!(result, Err(AssetError::DependencyCycle { .. })));
        assert_eq!(server.state(&handle), AssetState::Failed);
        Ok(())
    }

    #[test]
    fn background_loading_completes_on_later_updates() -> Result<(), Box<dyn std::error::Error>> {
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
        )?
        .with_background_loading(true);
        std::fs::write(config.cooked_root().join("clip.dummyc"), b"ready")?;

        let server = Assets::new(config)?;
        server.register_factory(SlowFactory {
            delay: Duration::from_millis(60),
        });
        let handle = server.load_id::<DummyAsset>(asset_id)?;

        server.update()?;
        assert_eq!(server.state(&handle), AssetState::Loading);

        std::thread::sleep(Duration::from_millis(90));
        server.update()?;
        assert_eq!(server.state(&handle), AssetState::Installed);
        assert_eq!(server.get(&handle)?.0, "ready");
        Ok(())
    }

    #[test]
    fn background_completion_after_release_is_discarded() -> Result<(), Box<dyn std::error::Error>>
    {
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
        )?
        .with_background_loading(true);
        std::fs::write(config.cooked_root().join("clip.dummyc"), b"ready")?;

        let server = Assets::new(config)?;
        server.register_factory(SlowFactory {
            delay: Duration::from_millis(60),
        });
        let handle = server.load_id::<DummyAsset>(asset_id)?;

        server.update()?;
        assert_eq!(server.state(&handle), AssetState::Loading);
        assert_eq!(server.stats().inflight_loads, 1);

        drop(handle);
        server.update()?;
        assert_eq!(server.state_untyped(asset_id), AssetState::Unloaded);

        std::thread::sleep(Duration::from_millis(90));
        server.update()?;

        let stats = server.stats();
        assert_eq!(server.state_untyped(asset_id), AssetState::Unloaded);
        assert_eq!(stats.inflight_loads, 0);
        assert_eq!(stats.states.installed, 0);
        assert_eq!(stats.states.unloaded, 1);
        Ok(())
    }

    #[test]
    fn burst_background_loads_stay_bounded_by_worker_pool_and_queue(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        let mut entries = Vec::new();
        for index in 0..12 {
            let asset_id = AssetId::new();
            entries.push(AssetManifestEntry {
                asset_id,
                asset_type: "dummy".to_string(),
                importer: "dummy".to_string(),
                cooker: "dummy".to_string(),
                version: 1,
                source_path: format!("burst-{index}.dummy"),
                cooked_path: format!("burst-{index}.dummyc"),
                dependencies: Vec::new(),
                import_settings: serde_json::Value::Null,
            });
        }
        let ids = entries
            .iter()
            .map(|entry| entry.asset_id)
            .collect::<Vec<_>>();
        let config = write_manifest_entries(dir.path(), entries.clone())?
            .with_background_loading(true)
            .with_io_worker_threads(2)
            .with_io_queue_capacity(3);
        for (index, entry) in entries.iter().enumerate() {
            std::fs::write(
                config.cooked_root().join(&entry.cooked_path),
                format!("ready-{index}"),
            )?;
        }

        let server = Assets::new(config)?;
        server.register_factory(SlowFactory {
            delay: Duration::from_millis(250),
        });
        let handles = ids
            .iter()
            .map(|id| server.load_id::<DummyAsset>(*id))
            .collect::<Result<Vec<_>, _>>()?;

        server.update()?;

        let bounded = server.stats();

        assert_eq!(bounded.load_worker_threads, 2);
        assert_eq!(bounded.load_queue_capacity, 3);
        assert!(
            bounded.inflight_loads <= bounded.load_worker_threads + bounded.load_queue_capacity
        );
        assert!(
            bounded.inflight_loads < handles.len(),
            "all burst loads entered in-flight state without backpressure: {bounded:?}"
        );
        assert_eq!(bounded.active_requests, handles.len());
        assert_eq!(
            bounded.source_load_phases.queued
                + bounded.source_load_phases.reading
                + bounded.source_load_phases.decoding,
            bounded.inflight_loads
        );
        assert!(bounded.deferred_load_submissions >= handles.len() - bounded.inflight_loads);
        assert_eq!(bounded.states.loading, handles.len());
        Ok(())
    }

    #[test]
    fn asset_stats_report_queue_inflight_and_state_counts() -> Result<(), Box<dyn std::error::Error>>
    {
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
        )?
        .with_background_loading(true);
        std::fs::write(config.cooked_root().join("clip.dummyc"), b"ready")?;

        let server = Assets::new(config)?;
        server.register_factory(SlowFactory {
            delay: Duration::from_millis(60),
        });
        let handle = server.load_id::<DummyAsset>(asset_id)?;

        let diagnostics = server.diagnostics_snapshot();
        let queued = diagnostics.stats.clone();
        assert_eq!(queued.records, 1);
        assert_eq!(queued.queued_requests, 1);
        assert_eq!(queued.active_requests, 0);
        assert_eq!(
            queued.load_worker_threads,
            server.config().io_worker_threads
        );
        assert_eq!(
            queued.load_queue_capacity,
            server.config().io_queue_capacity
        );
        assert_eq!(queued.submitted_requests, 1);
        assert_eq!(queued.activated_requests, 0);
        assert!(queued.oldest_queued_request_age.is_some());
        assert_eq!(queued.strong_references, 1);
        assert_eq!(queued.states.unloaded, 1);
        assert_eq!(diagnostics.queued_requests.len(), 1);
        assert!(diagnostics.active_requests.is_empty());
        assert!(diagnostics.failures.is_empty());
        assert!(diagnostics.last_reload_report.is_empty());
        let snapshots = server.queued_request_snapshots();
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].request_id, 0);
        assert_eq!(snapshots[0].asset_id, asset_id);
        assert_eq!(snapshots[0].priority, server.config().io_default_priority);
        assert_eq!(
            snapshots[0].status,
            crate::asset::AssetRequestStatus::Queued
        );

        server.update()?;
        let loading = server.stats();
        assert_eq!(loading.queued_requests, 0);
        assert_eq!(loading.active_requests, 1);
        assert_eq!(loading.submitted_requests, 1);
        assert_eq!(loading.activated_requests, 1);
        assert_eq!(loading.oldest_queued_request_age, None);
        assert_eq!(loading.inflight_loads, 1);
        assert_eq!(loading.states.loading, 1);
        assert!(server.queued_request_snapshots().is_empty());
        let active = server.active_request_snapshots();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].request_id, 0);
        assert_eq!(active[0].asset_id, asset_id);
        assert_eq!(active[0].generation, 1);
        assert!(
            matches!(
                active[0].status,
                crate::asset::AssetRequestStatus::Loading
                    | crate::asset::AssetRequestStatus::Decoding
            ),
            "active request may advance from source read to decode before diagnostics are sampled"
        );
        assert!(active[0].active_age.is_some());

        wait_for_terminal_dummy(&server, &handle)?;
        let installed = server.stats();
        assert_eq!(installed.active_requests, 0);
        assert_eq!(installed.inflight_loads, 0);
        assert_eq!(installed.states.installed, 1);
        assert_eq!(installed.load_timings.completed_source_loads, 1);
        assert_eq!(installed.load_timings.failed_source_loads, 0);
        assert!(installed.load_timings.average_total_time.is_some());
        assert!(installed.retained_events > 0);
        Ok(())
    }

    #[test]
    fn load_blocking_completes_slow_asset_without_fixed_poll_limit(
    ) -> Result<(), Box<dyn std::error::Error>> {
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
        )?
        .with_background_loading(true);
        std::fs::write(config.cooked_root().join("clip.dummyc"), b"ready")?;

        let server = Assets::new(config)?;
        server.register_factory(SlowFactory {
            delay: Duration::from_millis(60),
        });

        let started = std::time::Instant::now();
        let asset = server.load_blocking::<DummyAsset>(asset_id)?;

        assert!(started.elapsed() >= Duration::from_millis(50));
        assert_eq!(asset.0, "ready");
        Ok(())
    }

    #[test]
    fn load_blocking_timeout_reports_current_state() -> Result<(), Box<dyn std::error::Error>> {
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
        )?
        .with_background_loading(true);
        std::fs::write(config.cooked_root().join("clip.dummyc"), b"ready")?;

        let server = Assets::new(config)?;
        server.register_factory(SlowFactory {
            delay: Duration::from_millis(80),
        });

        let result =
            server.load_blocking_with_timeout::<DummyAsset>(asset_id, Duration::from_millis(5));

        assert!(matches!(
            result,
            Err(AssetError::InvalidState {
                id,
                state: AssetState::Loading,
                ..
            }) if id == asset_id
        ));
        Ok(())
    }

    #[test]
    fn load_blocking_installs_dependency_chain() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        let parent_id = AssetId::new();
        let dependency_id = AssetId::new();
        let config = write_manifest_entries(
            dir.path(),
            vec![
                AssetManifestEntry {
                    asset_id: parent_id,
                    asset_type: "dummy".to_string(),
                    importer: "dummy".to_string(),
                    cooker: "dummy".to_string(),
                    version: 1,
                    source_path: "parent.dummy".to_string(),
                    cooked_path: "parent.dummyc".to_string(),
                    dependencies: vec![dependency_id],
                    import_settings: serde_json::Value::Null,
                },
                AssetManifestEntry {
                    asset_id: dependency_id,
                    asset_type: "dummy".to_string(),
                    importer: "dummy".to_string(),
                    cooker: "dummy".to_string(),
                    version: 1,
                    source_path: "dependency.dummy".to_string(),
                    cooked_path: "dependency.dummyc".to_string(),
                    dependencies: Vec::new(),
                    import_settings: serde_json::Value::Null,
                },
            ],
        )?;
        std::fs::write(config.cooked_root().join("parent.dummyc"), b"parent")?;
        std::fs::write(
            config.cooked_root().join("dependency.dummyc"),
            b"dependency",
        )?;

        let server = Assets::new(config)?;
        server.register_factory(DummyFactory);

        let asset = server.load_blocking::<DummyAsset>(parent_id)?;

        assert_eq!(asset.0, "parent");
        assert_eq!(server.state_untyped(parent_id), AssetState::Installed);
        assert_eq!(server.state_untyped(dependency_id), AssetState::Installed);
        Ok(())
    }

    #[test]
    fn cooked_texture_load_uses_background_worker_even_without_global_background_loading(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        let asset_id = AssetId::new();
        let config = write_manifest(
            dir.path(),
            AssetManifestEntry {
                asset_id,
                asset_type: TextureAsset::TYPE.to_string(),
                importer: "texture".to_string(),
                cooker: "texture".to_string(),
                version: 1,
                source_path: "white.png".to_string(),
                cooked_path: "white.skytex".to_string(),
                dependencies: Vec::new(),
                import_settings: serde_json::Value::Null,
            },
        )?;
        std::fs::write(
            config.cooked_root().join("white.skytex"),
            crate::asset::texture::encode_texture_cooked(&TextureAsset::white_pixel()),
        )?;

        let server = Assets::new(config)?;
        server.register_factory(SlowTextureFactory {
            delay: Duration::from_millis(60),
        });
        let handle = server.load_id::<TextureAsset>(asset_id)?;
        server.update()?;
        assert_eq!(server.state(&handle), AssetState::Loading);
        assert_eq!(server.stats().inflight_loads, 1);

        wait_for_terminal_texture(&server, &handle)?;
        assert_eq!(server.state(&handle), AssetState::Installed);
        assert_eq!(server.get(&handle)?.size(), [1, 1]);
        Ok(())
    }

    #[test]
    fn install_budget_limits_number_of_installs_per_update(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        let first_id = AssetId::new();
        let second_id = AssetId::new();
        let config = write_manifest_entries(
            dir.path(),
            vec![
                AssetManifestEntry {
                    asset_id: first_id,
                    asset_type: "dummy".to_string(),
                    importer: "dummy".to_string(),
                    cooker: "dummy".to_string(),
                    version: 1,
                    source_path: "first.dummy".to_string(),
                    cooked_path: "first.dummyc".to_string(),
                    dependencies: Vec::new(),
                    import_settings: serde_json::Value::Null,
                },
                AssetManifestEntry {
                    asset_id: second_id,
                    asset_type: "dummy".to_string(),
                    importer: "dummy".to_string(),
                    cooker: "dummy".to_string(),
                    version: 1,
                    source_path: "second.dummy".to_string(),
                    cooked_path: "second.dummyc".to_string(),
                    dependencies: Vec::new(),
                    import_settings: serde_json::Value::Null,
                },
            ],
        )?
        .with_install_budget_per_update(1);
        std::fs::write(config.cooked_root().join("first.dummyc"), b"first")?;
        std::fs::write(config.cooked_root().join("second.dummyc"), b"second")?;

        let server = Assets::new(config)?;
        server.register_factory(DummyFactory);
        let first = server.load_id::<DummyAsset>(first_id)?;
        let second = server.load_id::<DummyAsset>(second_id)?;

        server.update()?;
        let states = [server.state(&first), server.state(&second)];
        let installed = states
            .iter()
            .filter(|state| **state == AssetState::Installed)
            .count();
        let installing = states
            .iter()
            .filter(|state| **state == AssetState::Installing)
            .count();
        assert_eq!(
            installed, 1,
            "states after first budgeted update: {states:?}"
        );
        assert_eq!(
            installing, 1,
            "states after first budgeted update: {states:?}"
        );

        server.update()?;
        assert_eq!(server.state(&first), AssetState::Installed);
        assert_eq!(server.state(&second), AssetState::Installed);
        Ok(())
    }

    #[test]
    fn reload_changed_returns_empty_when_assets_are_unchanged(
    ) -> Result<(), Box<dyn std::error::Error>> {
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

        let server = Assets::new(config)?;
        server.register_factory(DummyFactory);
        let handle = server.load_id::<DummyAsset>(asset_id)?;
        server.update()?;

        let changed = server.reload_changed()?;
        assert!(changed.is_empty());
        let report = server.last_reload_report();
        assert!(report.changed_roots.is_empty());
        assert!(report.impacted.is_empty());
        assert_eq!(report.skipped.len(), 1);
        assert_eq!(report.skipped[0].asset_id, asset_id);
        assert_eq!(report.skipped[0].reason, AssetReloadSkipReason::Unchanged);
        assert_eq!(server.get(&handle)?.0, "ready");
        Ok(())
    }

    #[test]
    fn reload_changed_reloads_modified_asset() -> Result<(), Box<dyn std::error::Error>> {
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
        let cooked_path = config.cooked_root().join("clip.dummyc");
        std::fs::write(&cooked_path, b"ready")?;

        let server = Assets::new(config)?;
        server.register_factory(DummyFactory);
        let handle = server.load_id::<DummyAsset>(asset_id)?;
        server.update()?;
        assert_eq!(server.get(&handle)?.0, "ready");

        std::fs::write(&cooked_path, b"fresh")?;
        let changed = server.reload_changed()?;
        assert_eq!(changed, vec![asset_id]);
        server.update()?;

        assert_eq!(server.get(&handle)?.0, "fresh");
        Ok(())
    }

    #[test]
    fn failed_reload_keeps_last_good_installed_asset() -> Result<(), Box<dyn std::error::Error>> {
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
        let cooked_path = config.cooked_root().join("clip.dummyc");
        std::fs::write(&cooked_path, b"ready")?;

        let server = Assets::new(config)?;
        server.register_factory(DummyFactory);
        let mut events = server.event_cursor();
        let handle = server.load_id::<DummyAsset>(asset_id)?;
        server.update()?;
        assert_eq!(server.get(&handle)?.0, "ready");
        let _ = server.events_since(&mut events);

        std::fs::write(&cooked_path, [0xff, 0xfe, 0xfd])?;
        let changed = server.reload_changed()?;
        assert_eq!(changed, vec![asset_id]);
        let error = server
            .update()
            .expect_err("invalid reload payload should report a read failure");
        assert!(matches!(error, AssetError::InvalidCookedAsset { .. }));

        assert_eq!(server.state(&handle), AssetState::Installed);
        assert_eq!(server.get(&handle)?.0, "ready");
        assert!(matches!(
            server.error(&handle),
            Some(AssetError::InvalidCookedAsset { .. })
        ));
        assert_eq!(
            server.failure_phase(&handle),
            Some(AssetFailurePhase::Decode)
        );
        let failed_events = server.events_since(&mut events);
        let failed = failed_events
            .iter()
            .find(|event| event.id == asset_id && event.kind == AssetEventKind::Failed)
            .expect("failed reload should emit a diagnostic event");
        assert_eq!(failed.state, AssetState::Installed);
        assert_eq!(failed.failure_phase, Some(AssetFailurePhase::Decode));
        let failures = server.failed_asset_snapshots();
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].asset_id, asset_id);
        assert_eq!(failures[0].state, AssetState::Installed);
        assert_eq!(failures[0].phase, Some(AssetFailurePhase::Decode));

        std::fs::write(&cooked_path, b"fresh")?;
        let changed = server.reload_changed()?;
        assert_eq!(changed, vec![asset_id]);
        server.update()?;
        assert_eq!(server.get(&handle)?.0, "fresh");
        assert!(server.error(&handle).is_none());
        assert!(server.failed_asset_snapshots().is_empty());
        Ok(())
    }

    #[test]
    fn auto_reload_reloads_modified_asset_on_update() -> Result<(), Box<dyn std::error::Error>> {
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
        )?
        .with_auto_reload(true)
        .with_auto_reload_interval(Duration::ZERO);
        let cooked_path = config.cooked_root().join("clip.dummyc");
        std::fs::write(&cooked_path, b"ready")?;

        let server = Assets::new(config)?;
        server.register_factory(DummyFactory);
        let handle = server.load_id::<DummyAsset>(asset_id)?;
        server.update()?;
        assert_eq!(server.get(&handle)?.0, "ready");
        assert!(server.last_reload_report().is_empty());

        std::fs::write(&cooked_path, b"fresh")?;
        server.update()?;

        assert_eq!(server.get(&handle)?.0, "fresh");
        let report = server.last_reload_report();
        assert_eq!(report.changed_roots, vec![asset_id]);
        assert_eq!(report.impacted, vec![asset_id]);
        Ok(())
    }

    #[test]
    fn auto_reload_debounces_modified_asset_on_update() -> Result<(), Box<dyn std::error::Error>> {
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
        )?
        .with_auto_reload(true)
        .with_auto_reload_interval(Duration::ZERO)
        .with_auto_reload_debounce(Duration::from_millis(40));
        let cooked_path = config.cooked_root().join("clip.dummyc");
        std::fs::write(&cooked_path, b"ready")?;

        let server = Assets::new(config)?;
        server.register_factory(DummyFactory);
        let handle = server.load_id::<DummyAsset>(asset_id)?;
        server.update()?;
        assert_eq!(server.get(&handle)?.0, "ready");

        std::fs::write(&cooked_path, b"fresh")?;
        server.update()?;
        assert_eq!(server.get(&handle)?.0, "ready");
        let pending = server.reload_status();
        assert_eq!(pending.pending_roots, vec![asset_id]);
        assert!(pending.last_report.is_empty());

        std::thread::sleep(Duration::from_millis(70));
        server.update()?;
        assert_eq!(server.get(&handle)?.0, "fresh");
        let reloaded = server.reload_status();
        assert!(reloaded.pending_roots.is_empty());
        assert_eq!(reloaded.last_report.changed_roots, vec![asset_id]);
        assert_eq!(reloaded.last_report.impacted, vec![asset_id]);
        Ok(())
    }

    #[test]
    fn auto_reload_freeze_delays_scan_until_unfrozen() -> Result<(), Box<dyn std::error::Error>> {
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
        )?
        .with_auto_reload(true)
        .with_auto_reload_interval(Duration::ZERO);
        let cooked_path = config.cooked_root().join("clip.dummyc");
        std::fs::write(&cooked_path, b"ready")?;

        let server = Assets::new(config)?;
        server.register_factory(DummyFactory);
        let handle = server.load_id::<DummyAsset>(asset_id)?;
        server.update()?;
        assert_eq!(server.get(&handle)?.0, "ready");

        server.set_auto_reload_frozen(true);
        std::fs::write(&cooked_path, b"fresh")?;
        server.update()?;
        assert_eq!(server.get(&handle)?.0, "ready");
        let frozen = server.reload_status();
        assert!(frozen.auto_reload_frozen);
        assert!(frozen.pending_roots.is_empty());

        server.set_auto_reload_frozen(false);
        server.update()?;
        assert_eq!(server.get(&handle)?.0, "fresh");
        let reloaded = server.reload_status();
        assert!(!reloaded.auto_reload_frozen);
        assert_eq!(reloaded.last_report.changed_roots, vec![asset_id]);
        Ok(())
    }

    #[cfg(feature = "asset-watch")]
    #[test]
    fn file_watcher_triggers_auto_reload_before_poll_interval(
    ) -> Result<(), Box<dyn std::error::Error>> {
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
        )?
        .with_auto_reload(true)
        .with_auto_reload_interval(Duration::from_secs(60))
        .with_file_watcher(true);
        let cooked_path = config.cooked_root().join("clip.dummyc");
        std::fs::write(&cooked_path, b"ready")?;

        let server = Assets::new(config)?;
        server.register_factory(DummyFactory);
        let handle = server.load_id::<DummyAsset>(asset_id)?;
        server.update()?;
        assert_eq!(server.get(&handle)?.0, "ready");

        std::fs::write(&cooked_path, b"fresh")?;
        for _ in 0..100 {
            server.update()?;
            if server.get(&handle)?.0 == "fresh" {
                let report = server.last_reload_report();
                assert_eq!(report.changed_roots, vec![asset_id]);
                assert_eq!(report.impacted, vec![asset_id]);
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(25));
        }

        panic!("asset watcher did not trigger reload before poll interval");
    }

    #[cfg(feature = "asset-watch")]
    #[test]
    fn file_watcher_triggers_auto_reload_for_external_package_root(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        let package_dir = tempdir()?;
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
        )?
        .with_package_root(package_dir.path())
        .with_auto_reload(true)
        .with_auto_reload_interval(Duration::from_secs(60))
        .with_file_watcher(true);
        let package_cooked_path = package_dir.path().join("clip.dummyc");
        std::fs::write(&package_cooked_path, b"ready")?;

        let server = Assets::new(config)?;
        server.register_factory(DummyFactory);
        let handle = server.load_id::<DummyAsset>(asset_id)?;
        server.update()?;
        assert_eq!(server.get(&handle)?.0, "ready");

        std::fs::write(&package_cooked_path, b"fresh")?;
        for _ in 0..100 {
            server.update()?;
            if server.get(&handle)?.0 == "fresh" {
                let report = server.last_reload_report();
                assert_eq!(report.changed_roots, vec![asset_id]);
                assert_eq!(report.impacted, vec![asset_id]);
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(25));
        }

        panic!("external package root watcher did not trigger reload before poll interval");
    }

    #[cfg(feature = "asset-watch")]
    #[test]
    fn file_watcher_triggers_auto_reload_for_external_package_file(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        let package_dir = tempdir()?;
        let asset_id = AssetId::new();
        let package_file = package_dir.path().join("base.skybundle");
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
        )?
        .with_package_file(&package_file)
        .with_auto_reload(true)
        .with_auto_reload_interval(Duration::from_secs(60))
        .with_file_watcher(true);
        write_test_bundle(&package_file, &[("clip.dummyc", b"ready")])?;

        let server = Assets::new(config)?;
        server.register_factory(DummyFactory);
        let handle = server.load_id::<DummyAsset>(asset_id)?;
        server.update()?;
        assert_eq!(server.get(&handle)?.0, "ready");

        write_test_bundle(&package_file, &[("clip.dummyc", b"fresh")])?;
        for _ in 0..100 {
            server.update()?;
            if server.get(&handle)?.0 == "fresh" {
                let report = server.last_reload_report();
                assert_eq!(report.changed_roots, vec![asset_id]);
                assert_eq!(report.impacted, vec![asset_id]);
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(25));
        }

        panic!("external package file watcher did not trigger reload before poll interval");
    }

    #[test]
    fn force_reload_reloads_asset_even_when_hash_is_unchanged(
    ) -> Result<(), Box<dyn std::error::Error>> {
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

        let server = Assets::new(config)?;
        let factory = CountingFactory::new();
        server.register_factory(factory.clone());
        let handle = server.load_id::<DummyAsset>(asset_id)?;
        server.update()?;
        assert_eq!(server.get(&handle)?.0, "ready");
        assert_eq!(factory.load_count(asset_id), 1);

        let report = server.force_reload(asset_id)?;
        assert_eq!(report.changed_roots, vec![asset_id]);
        assert_eq!(report.impacted, vec![asset_id]);
        server.update()?;
        assert_eq!(server.get(&handle)?.0, "ready");
        assert_eq!(factory.load_count(asset_id), 2);
        assert_eq!(factory.install_count(asset_id), 2);
        Ok(())
    }

    #[test]
    fn reload_changed_reloads_dependents_of_changed_dependency(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        let parent_id = AssetId::new();
        let dependency_id = AssetId::new();
        let config = write_manifest_entries(
            dir.path(),
            vec![
                AssetManifestEntry {
                    asset_id: parent_id,
                    asset_type: "dummy".to_string(),
                    importer: "dummy".to_string(),
                    cooker: "dummy".to_string(),
                    version: 1,
                    source_path: "parent.dummy".to_string(),
                    cooked_path: "parent.dummyc".to_string(),
                    dependencies: vec![dependency_id],
                    import_settings: serde_json::Value::Null,
                },
                AssetManifestEntry {
                    asset_id: dependency_id,
                    asset_type: "dummy".to_string(),
                    importer: "dummy".to_string(),
                    cooker: "dummy".to_string(),
                    version: 1,
                    source_path: "dependency.dummy".to_string(),
                    cooked_path: "dependency.dummyc".to_string(),
                    dependencies: Vec::new(),
                    import_settings: serde_json::Value::Null,
                },
            ],
        )?;
        std::fs::write(config.cooked_root().join("parent.dummyc"), b"parent")?;
        let dependency_path = config.cooked_root().join("dependency.dummyc");
        std::fs::write(&dependency_path, b"dependency-v1")?;

        let server = Assets::new(config)?;
        let factory = CountingFactory::new();
        server.register_factory(factory.clone());
        let parent = server.load_id::<DummyAsset>(parent_id)?;
        server.update()?;
        assert_eq!(server.get(&parent)?.0, "parent");
        assert_eq!(factory.load_count(parent_id), 1);
        assert_eq!(factory.load_count(dependency_id), 1);
        assert_eq!(factory.install_count(parent_id), 1);
        assert_eq!(factory.install_count(dependency_id), 1);

        std::fs::write(&dependency_path, b"dependency-v2")?;
        let report = server.reload_changed_with_report()?;
        assert_eq!(report.changed_roots, vec![dependency_id]);
        let changed_ids: HashSet<_> = report.impacted.into_iter().collect();
        assert_eq!(changed_ids, HashSet::from([parent_id, dependency_id]));

        server.update()?;
        assert_eq!(factory.load_count(parent_id), 2);
        assert_eq!(factory.load_count(dependency_id), 2);
        assert_eq!(factory.install_count(parent_id), 2);
        assert_eq!(factory.install_count(dependency_id), 2);
        Ok(())
    }

    #[test]
    fn large_dependency_graph_loads_reloads_and_releases_closure(
    ) -> Result<(), Box<dyn std::error::Error>> {
        const ASSET_COUNT: usize = 63;

        let dir = tempdir()?;
        let ids: Vec<_> = (0..ASSET_COUNT).map(|_| AssetId::new()).collect();
        let entries = ids
            .iter()
            .enumerate()
            .map(|(index, &asset_id)| {
                let dependencies = [index * 2 + 1, index * 2 + 2]
                    .into_iter()
                    .filter_map(|child| ids.get(child).copied())
                    .collect();
                AssetManifestEntry {
                    asset_id,
                    asset_type: "dummy".to_string(),
                    importer: "dummy".to_string(),
                    cooker: "dummy".to_string(),
                    version: 1,
                    source_path: format!("asset-{index}.dummy"),
                    cooked_path: format!("asset-{index}.dummyc"),
                    dependencies,
                    import_settings: serde_json::Value::Null,
                }
            })
            .collect();
        let config = write_manifest_entries(dir.path(), entries)?;
        let cooked_root = config.cooked_root().to_path_buf();
        for index in 0..ASSET_COUNT {
            std::fs::write(
                cooked_root.join(format!("asset-{index}.dummyc")),
                format!("asset-{index}-v1"),
            )?;
        }

        let server = Assets::new(config)?;
        let factory = CountingFactory::new();
        server.register_factory(factory.clone());
        let root = server.load_id::<DummyAsset>(ids[0])?;
        server.update()?;

        assert_eq!(server.get(&root)?.0, "asset-0-v1");
        assert_eq!(server.stats().states.installed, ASSET_COUNT);
        for &id in &ids {
            assert_eq!(server.state_untyped(id), AssetState::Installed);
            assert_eq!(factory.load_count(id), 1);
            assert_eq!(factory.install_count(id), 1);
        }

        let leaf_indexes: Vec<_> = (0..ASSET_COUNT)
            .filter(|index| index * 2 + 1 >= ASSET_COUNT)
            .collect();
        for &index in &leaf_indexes {
            std::fs::write(
                cooked_root.join(format!("asset-{index}.dummyc")),
                format!("asset-{index}-v2"),
            )?;
        }

        let report = server.reload_changed_with_report()?;
        let changed_roots: HashSet<_> = report.changed_roots.into_iter().collect();
        let expected_roots: HashSet<_> = leaf_indexes.iter().map(|&index| ids[index]).collect();
        assert_eq!(changed_roots, expected_roots);
        let impacted: HashSet<_> = report.impacted.into_iter().collect();
        let expected_impacted: HashSet<_> = ids.iter().copied().collect();
        assert_eq!(impacted, expected_impacted);

        for _ in 0..ASSET_COUNT {
            server.update()?;
            if ids.iter().all(|&id| factory.install_count(id) == 2) {
                break;
            }
        }

        assert_eq!(server.get(&root)?.0, "asset-0-v1");
        assert_eq!(server.stats().states.installed, ASSET_COUNT);
        for &id in &ids {
            assert_eq!(server.state_untyped(id), AssetState::Installed);
            assert_eq!(factory.load_count(id), 2);
            assert_eq!(factory.install_count(id), 2);
        }

        drop(root);
        server.update()?;
        assert_eq!(server.stats().states.unloaded, ASSET_COUNT);
        Ok(())
    }
}
