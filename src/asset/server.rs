use std::any::{Any, TypeId};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, Weak};
use std::time::Instant;

use super::driver::AssetRecordDriver;
use super::events::{self, AssetEventLog};
use super::font::{FontAsset, FontAssetFactory};
use super::install::{self, AssetInstallBudget};
use super::load::{
    drain_completed_loads_or_fail, has_current_inflight_load, load_record_now_and_apply_or_fail,
    should_load_record_in_background, submit_record_load_or_fail, AssetLoadQueue,
};
use super::provider::{AssetProvider, LocalAssetProvider};
use super::query;
use super::registry::{
    resolve_typed_source_asset, AssetFactories, AssetRegistryLoader, AssetRuntimeFactory,
    LocalManifestRegistry, LocalManifestRegistryLoader,
};
use super::reload::AssetReloadService;
use super::request::AssetRequests;
use super::runtime;
use super::store::AssetStore;
use super::texture::{TextureAsset, TextureAssetFactory};
use super::types::{
    Asset, AssetConfig, AssetDiagnosticsSnapshot, AssetError, AssetEvent, AssetEventCursor,
    AssetFailurePhase, AssetFailureSnapshot, AssetHandleProvider, AssetId, AssetLease,
    AssetMetadata, AssetPath, AssetRegistryManifest, AssetReloadReport, AssetReloadStatus,
    AssetRequestSnapshot, AssetState, AssetStats, AssetStatus, AssetWatchPaths, Handle, WeakHandle,
};
use super::update::AssetUpdateContext;
use super::{blocking, dependency, lease, update};

#[derive(Clone)]
pub struct Assets {
    pub(crate) inner: Arc<Mutex<AssetsInner>>,
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
    pub(crate) fn with_manifest_and_provider(
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
    pub(crate) fn with_registry_loader_and_provider(
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

pub(crate) struct AssetsInner {
    pub(crate) config: AssetConfig,
    provider: Box<dyn AssetProvider>,
    registry_loader: Box<dyn AssetRegistryLoader>,
    manifest: LocalManifestRegistry,
    factories: AssetFactories,
    pub(crate) store: AssetStore,
    requests: AssetRequests,
    release_tx: Sender<AssetId>,
    release_rx: Receiver<AssetId>,
    handle_provider: Option<Weak<dyn AssetHandleProvider>>,
    pub(crate) load_queue: AssetLoadQueue,
    events: AssetEventLog,
    reload: AssetReloadService,
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
        let reload = AssetReloadService::new(&config);
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
            load_queue: AssetLoadQueue::with_shutdown_timeout(
                io_worker_threads,
                io_queue_capacity,
                io_shutdown_timeout,
            ),
            events: AssetEventLog::default(),
            reload,
        }
    }

    fn set_handle_provider(&mut self, handle_provider: Weak<dyn AssetHandleProvider>) {
        self.handle_provider = Some(handle_provider);
    }

    fn set_auto_reload_frozen(&mut self, frozen: bool) {
        self.reload.set_frozen(frozen);
    }

    fn last_reload_report(&self) -> AssetReloadReport {
        query::last_reload_report(&self.reload)
    }

    fn config(&self) -> AssetConfig {
        query::config(&self.config)
    }

    fn state_for_handle(&self, id: AssetId) -> AssetState {
        query::state_for_handle(&self.store, id)
    }

    fn error_for_handle(&self, id: AssetId) -> Option<AssetError> {
        query::error_for_handle(&self.store, id)
    }

    fn failure_phase(&self, id: AssetId) -> Option<AssetFailurePhase> {
        query::failure_phase(&self.store, id)
    }

    fn get_for_handle(
        &self,
        id: AssetId,
        expected_type: &'static str,
        expected_type_id: TypeId,
    ) -> Result<Arc<dyn Any + Send + Sync>, AssetError> {
        query::get_for_handle(&self.store, id, expected_type, expected_type_id)
    }

    fn event_cursor(&self) -> AssetEventCursor {
        query::event_cursor(&self.events)
    }

    fn events_since(&self, cursor: &mut AssetEventCursor) -> Vec<AssetEvent> {
        query::events_since(&self.events, cursor)
    }

    fn stats(&self) -> AssetStats {
        query::stats(
            &self.store,
            &self.requests,
            &self.load_queue,
            self.provider.as_ref(),
            &self.events,
        )
    }

    fn queued_request_snapshots(&self) -> Vec<AssetRequestSnapshot> {
        query::queued_request_snapshots(&self.requests)
    }

    fn active_request_snapshots(&self) -> Vec<AssetRequestSnapshot> {
        query::active_request_snapshots(&self.store, &self.requests, &self.load_queue)
    }

    fn failed_request_snapshots(&self) -> Vec<AssetRequestSnapshot> {
        query::failed_request_snapshots(&self.store, &self.requests)
    }

    fn canceled_request_snapshots(&self) -> Vec<AssetRequestSnapshot> {
        query::canceled_request_snapshots(&self.requests)
    }

    fn failed_asset_snapshots(&self) -> Vec<AssetFailureSnapshot> {
        query::failed_asset_snapshots(&self.store)
    }

    fn diagnostics_snapshot(&self) -> AssetDiagnosticsSnapshot {
        query::diagnostics_snapshot(
            &self.config,
            &self.store,
            &self.requests,
            &self.load_queue,
            self.provider.as_ref(),
            &self.events,
            &self.reload,
        )
    }

    fn reload_status(&self) -> AssetReloadStatus {
        query::reload_status(&self.config, &self.reload)
    }

    fn reload_manifest(&mut self) -> Result<(), AssetError> {
        self.reload.reload_manifest(
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
        query::typed_path::<T>(&self.manifest, &self.factories, id)
    }

    fn manifest_snapshot(&self) -> AssetRegistryManifest {
        query::manifest(&self.manifest)
    }

    fn lookup_source_asset(&self, path: &Path) -> Option<AssetId> {
        query::resolve_path(&self.config, &self.manifest, path)
    }

    fn source_path(&self, id: AssetId) -> Option<PathBuf> {
        query::source(&self.manifest, id)
    }

    fn metadata(&self, id: AssetId) -> Option<AssetMetadata> {
        query::asset_metadata(&self.manifest, id)
    }

    fn watch_paths(&self, id: AssetId) -> Option<AssetWatchPaths> {
        query::asset_watch_paths(&self.config, &self.manifest, id)
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
        self.reload.reload_changed_with_report(
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

    fn force_reload(&mut self, id: AssetId) -> Result<AssetReloadReport, AssetError> {
        self.reload.force_reload(
            &self.config,
            &mut self.store,
            &mut self.events,
            &mut self.requests,
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
        self.reload.drive_auto_reload(
            &self.config,
            &mut self.store,
            &mut self.events,
            &mut self.requests,
            self.provider.as_ref(),
            self.registry_loader.as_ref(),
            &mut self.manifest,
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
        update::apply_update(self)
    }

    fn apply_blocking_update(
        &mut self,
        target_id: AssetId,
        force_synchronous_load: bool,
    ) -> Result<(), AssetError> {
        update::apply_blocking_update(self, target_id, force_synchronous_load)
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

impl AssetUpdateContext for AssetsInner {
    fn drain_handle_releases(&mut self) {
        AssetsInner::drain_handle_releases(self);
    }

    fn maybe_auto_reload(&mut self) -> Result<(), AssetError> {
        AssetsInner::maybe_auto_reload(self)
    }

    fn activate_queued_requests(&mut self) -> Result<(), AssetError> {
        AssetsInner::activate_queued_requests(self)
    }

    fn refresh_active_requests_after_drive(&mut self) {
        AssetsInner::refresh_active_requests_after_drive(self);
    }

    fn install_budget_per_update(&self) -> Option<usize> {
        self.config.install_budget_per_update
    }

    fn install_time_budget(&self) -> Option<std::time::Duration> {
        self.config.install_time_budget
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
