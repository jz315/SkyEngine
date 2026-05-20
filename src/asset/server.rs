use std::any::{Any, TypeId};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};

use super::font::{FontAsset, FontAssetFactory};
use super::registry::{AssetRuntimeFactory, ErasedAssetFactory, FactoryAdapter, ManifestIndex};
use super::texture::{
    decode_texture_source_bytes, TextureAsset, TextureAssetFactory, TextureColorSpace,
};
use super::types::{
    Asset, AssetConfig, AssetError, AssetEvent, AssetEventCursor, AssetEventKind,
    AssetHandleProvider, AssetId, AssetInstallContext, AssetLease, AssetLoadContext,
    AssetManifestEntry, AssetRegistryManifest, AssetState, AssetStatus, Handle, WeakHandle,
    ASSET_SYSTEM_VERSION,
};

const ASSET_EVENT_LOG_CAP: usize = 1024;

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
        let manifest = load_manifest(&config)?;
        let (release_tx, release_rx) = mpsc::channel();
        let mut inner = AssetsInner::new(config, manifest, release_rx);
        inner.register_factory(TextureAssetFactory);
        inner.register_factory(FontAssetFactory);
        Ok(Self {
            inner: Arc::new(Mutex::new(inner)),
            release_tx,
        })
    }

    #[must_use]
    pub fn with_empty_manifest(config: AssetConfig) -> Self {
        let (release_tx, release_rx) = mpsc::channel();
        let mut inner = AssetsInner::new(config, AssetRegistryManifest::default(), release_rx);
        inner.register_factory(TextureAssetFactory);
        inner.register_factory(FontAssetFactory);
        Self {
            inner: Arc::new(Mutex::new(inner)),
            release_tx,
        }
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
        let manifest = load_manifest(&inner.config)?;
        inner.set_manifest(manifest);
        Ok(())
    }

    pub fn reload_changed(&self) -> Result<Vec<AssetId>, AssetError> {
        let mut inner = self.inner.lock().expect("assets mutex poisoned");
        inner.reload_changed()
    }

    #[must_use]
    pub fn config(&self) -> AssetConfig {
        self.inner
            .lock()
            .expect("assets mutex poisoned")
            .config
            .clone()
    }

    pub fn load<T: Asset>(&self, path: impl AsRef<Path>) -> Result<Handle<T>, AssetError> {
        let path_buf = path.as_ref().to_path_buf();
        let mut inner = self.inner.lock().expect("assets mutex poisoned");
        let id = inner
            .lookup_source_asset(&path_buf)
            .ok_or_else(|| AssetError::AssetPathNotFound { path: path_buf })?;
        inner.validate_typed_request::<T>(id)?;
        inner.acquire_direct_lease(id, Some(TypeId::of::<T>()));
        Ok(self.make_handle(id))
    }

    pub fn load_id<T: Asset>(&self, id: AssetId) -> Result<Handle<T>, AssetError> {
        let mut inner = self.inner.lock().expect("assets mutex poisoned");
        inner.validate_typed_request::<T>(id)?;
        inner.acquire_direct_lease(id, Some(TypeId::of::<T>()));
        Ok(self.make_handle(id))
    }

    pub fn load_handle<T: Asset>(&self, handle: WeakHandle<T>) -> Result<Handle<T>, AssetError> {
        self.load_id(handle.id())
    }

    pub fn load_texture(
        &self,
        path_or_key: impl AsRef<Path>,
    ) -> Result<Handle<TextureAsset>, AssetError> {
        let mut inner = self.inner.lock().expect("assets mutex poisoned");
        let id = inner.load_texture(path_or_key.as_ref())?;
        Ok(self.make_handle(id))
    }

    pub fn load_font(
        &self,
        path_or_key: impl AsRef<Path>,
    ) -> Result<Handle<FontAsset>, AssetError> {
        let mut inner = self.inner.lock().expect("assets mutex poisoned");
        let id = inner.load_font(path_or_key.as_ref())?;
        Ok(self.make_handle(id))
    }

    pub fn load_blocking<T: Asset>(&self, id: AssetId) -> Result<Arc<T>, AssetError> {
        let handle = self.load_id::<T>(id)?;
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
        self.get_id(handle.id())
    }

    pub fn get_id<T: Asset>(&self, id: AssetId) -> Result<Arc<T>, AssetError> {
        let installed = self.inner.get_for_handle(id, T::TYPE, TypeId::of::<T>())?;
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
            .records
            .get(&handle.id())
            .and_then(|record| record.error.clone())
    }

    #[must_use]
    pub fn manifest(&self) -> AssetRegistryManifest {
        self.inner
            .lock()
            .expect("assets mutex poisoned")
            .manifest
            .manifest
            .clone()
    }

    #[must_use]
    pub fn resolve_path(&self, path: impl AsRef<Path>) -> Option<AssetId> {
        self.inner
            .lock()
            .expect("assets mutex poisoned")
            .lookup_source_asset(path.as_ref())
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
        inner.retain_direct_lease(id, Some(TypeId::of::<T>()));
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
            .records
            .get(&id)
            .map(|record| record.state)
            .unwrap_or(AssetState::Unloaded)
    }

    fn error_for_handle(&self, id: AssetId) -> Option<AssetError> {
        self.lock()
            .expect("assets mutex poisoned")
            .records
            .get(&id)
            .and_then(|record| record.error.clone())
    }

    fn get_for_handle(
        &self,
        id: AssetId,
        expected_type: &'static str,
        expected_type_id: TypeId,
    ) -> Result<Arc<dyn Any + Send + Sync>, AssetError> {
        let inner = self.lock().expect("assets mutex poisoned");
        let record = inner
            .records
            .get(&id)
            .ok_or(AssetError::AssetNotInstalled {
                id,
                state: AssetState::Unloaded,
            })?;
        if record
            .requested_type
            .is_some_and(|type_id| type_id != expected_type_id)
        {
            return Err(AssetError::AssetTypeMismatch {
                id,
                expected: expected_type,
                actual: record.asset_type.clone(),
            });
        }
        record
            .installed
            .clone()
            .ok_or(AssetError::AssetNotInstalled {
                id,
                state: record.state,
            })
    }
}

struct AssetsInner {
    config: AssetConfig,
    manifest: ManifestIndex,
    factories: HashMap<String, Arc<dyn ErasedAssetFactory>>,
    records: HashMap<AssetId, AssetRecord>,
    raw_textures: HashMap<String, AssetId>,
    raw_fonts: HashMap<String, AssetId>,
    requests: VecDeque<AssetRequest>,
    release_rx: Receiver<AssetId>,
    load_tx: Sender<CompletedLoad>,
    load_rx: Receiver<CompletedLoad>,
    inflight_loads: HashSet<(AssetId, u64)>,
    events: VecDeque<AssetEvent>,
    next_event_sequence: u64,
}

impl AssetsInner {
    fn new(
        config: AssetConfig,
        manifest: AssetRegistryManifest,
        release_rx: Receiver<AssetId>,
    ) -> Self {
        let (load_tx, load_rx) = mpsc::channel();
        Self {
            config,
            manifest: ManifestIndex::new(manifest),
            factories: HashMap::default(),
            records: HashMap::default(),
            raw_textures: HashMap::default(),
            raw_fonts: HashMap::default(),
            requests: VecDeque::new(),
            release_rx,
            load_tx,
            load_rx,
            inflight_loads: HashSet::default(),
            events: VecDeque::new(),
            next_event_sequence: 0,
        }
    }

    fn push_event(&mut self, id: AssetId, kind: AssetEventKind, state: AssetState) {
        let event = AssetEvent {
            sequence: self.next_event_sequence,
            id,
            kind,
            state,
        };
        self.next_event_sequence = self.next_event_sequence.wrapping_add(1);
        self.events.push_back(event);
        while self.events.len() > ASSET_EVENT_LOG_CAP {
            self.events.pop_front();
        }
    }

    fn event_cursor(&self) -> AssetEventCursor {
        AssetEventCursor::new(self.next_event_sequence)
    }

    fn events_since(&self, cursor: &mut AssetEventCursor) -> Vec<AssetEvent> {
        let next_sequence = cursor.next_sequence();
        let events = self
            .events
            .iter()
            .copied()
            .filter(|event| event.sequence >= next_sequence)
            .collect();
        cursor.set_next_sequence(self.next_event_sequence);
        events
    }

    fn set_manifest(&mut self, manifest: AssetRegistryManifest) {
        self.manifest = ManifestIndex::new(manifest);

        let ids: Vec<_> = self.records.keys().copied().collect();
        for id in ids {
            if self
                .records
                .get(&id)
                .is_some_and(|record| record.runtime || record.raw_source_path.is_some())
            {
                continue;
            }

            if let Some(entry) = self.manifest.entry(id) {
                self.records
                    .get_mut(&id)
                    .expect("record should exist")
                    .asset_type = entry.asset_type.clone();
                continue;
            }

            self.release_held_dependencies(id);
            if let Some(record) = self.records.get_mut(&id) {
                record.loaded = None;
                record.installed = None;
                record.error = Some(AssetError::AssetNotFound { id });
                record.state = AssetState::Failed;
            }
            self.push_event(id, AssetEventKind::Failed, AssetState::Failed);
            self.schedule_release_if_unused(id);
        }
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

    fn load_texture(&mut self, path_or_key: &Path) -> Result<AssetId, AssetError> {
        if let Some(id) = self.lookup_source_asset(path_or_key) {
            self.validate_typed_request::<TextureAsset>(id)?;
            self.acquire_direct_lease(id, Some(TypeId::of::<TextureAsset>()));
            return Ok(id);
        }

        let request = RawTextureRequest::new(&self.config, path_or_key);
        let id = match self.raw_textures.get(&request.key).copied() {
            Some(id) => id,
            None => {
                let id = AssetId::new();
                self.raw_textures.insert(request.key, id);
                self.records
                    .insert(id, AssetRecord::new_raw_texture(request.path.clone()));
                id
            }
        };

        let factory = self.factories.get(TextureAsset::TYPE).ok_or_else(|| {
            AssetError::FactoryNotRegistered {
                asset_type: TextureAsset::TYPE.to_string(),
            }
        })?;
        if factory.product_type_id() != TypeId::of::<TextureAsset>() {
            return Err(AssetError::Internal {
                message: "registered texture factory product type does not match TextureAsset"
                    .to_string(),
            });
        }

        if let Some(record) = self.records.get_mut(&id) {
            record.strong_ref_count += 1;
            if record.requested_type.is_none() {
                record.requested_type = Some(TypeId::of::<TextureAsset>());
            }
        }
        self.requests.push_back(AssetRequest {
            id,
            requested_type: Some(TypeId::of::<TextureAsset>()),
        });
        Ok(id)
    }

    fn load_font(&mut self, path_or_key: &Path) -> Result<AssetId, AssetError> {
        if let Some(id) = self.lookup_source_asset(path_or_key) {
            self.validate_typed_request::<FontAsset>(id)?;
            self.acquire_direct_lease(id, Some(TypeId::of::<FontAsset>()));
            return Ok(id);
        }

        let request = RawTextureRequest::new(&self.config, path_or_key);
        let id = match self.raw_fonts.get(&request.key).copied() {
            Some(id) => id,
            None => {
                let id = AssetId::new();
                self.raw_fonts.insert(request.key, id);
                self.records
                    .insert(id, AssetRecord::new_raw_font(request.path.clone()));
                id
            }
        };

        let factory = self.factories.get(FontAsset::TYPE).ok_or_else(|| {
            AssetError::FactoryNotRegistered {
                asset_type: FontAsset::TYPE.to_string(),
            }
        })?;
        if factory.product_type_id() != TypeId::of::<FontAsset>() {
            return Err(AssetError::Internal {
                message: "registered font factory product type does not match FontAsset"
                    .to_string(),
            });
        }

        if let Some(record) = self.records.get_mut(&id) {
            record.strong_ref_count += 1;
            if record.requested_type.is_none() {
                record.requested_type = Some(TypeId::of::<FontAsset>());
            }
        }
        self.requests.push_back(AssetRequest {
            id,
            requested_type: Some(TypeId::of::<FontAsset>()),
        });
        Ok(id)
    }

    fn should_track_for_reload(&self, id: AssetId) -> bool {
        self.records.get(&id).is_some_and(|record| {
            !record.runtime
                && record.raw_source_path.is_none()
                && (record.strong_ref_count > 0
                    || record.dependency_ref_count > 0
                    || record.installed.is_some()
                    || record.loaded.is_some()
                    || record.reload_pending)
        })
    }

    fn acquire_direct_lease(&mut self, id: AssetId, requested_type: Option<TypeId>) {
        self.retain_direct_lease(id, requested_type);
        self.requests.push_back(AssetRequest { id, requested_type });
    }

    fn retain_direct_lease(&mut self, id: AssetId, requested_type: Option<TypeId>) {
        let record = self.records.entry(id).or_insert_with(|| {
            let entry = self
                .manifest
                .entry(id)
                .expect("asset record must exist in manifest");
            AssetRecord::new(id, entry.asset_type.clone())
        });
        record.strong_ref_count += 1;

        if record.requested_type.is_none() {
            record.requested_type = requested_type;
        }
    }

    fn drain_handle_releases(&mut self) {
        while let Ok(id) = self.release_rx.try_recv() {
            if let Some(record) = self.records.get_mut(&id) {
                record.strong_ref_count = record.strong_ref_count.saturating_sub(1);
            }
            self.schedule_release_if_unused(id);
        }
    }

    fn activate_record_for_load(record: &mut AssetRecord, requested_type: Option<TypeId>) {
        if record.requested_type.is_none() {
            record.requested_type = requested_type;
        }

        match record.state {
            AssetState::Unloaded | AssetState::Failed => {
                record.load_generation = record.load_generation.wrapping_add(1);
                record.state = AssetState::Loading;
                record.error = None;
            }
            AssetState::Uninstalling => {
                record.state = AssetState::Installed;
                record.error = None;
            }
            AssetState::Unloading => {
                record.load_generation = record.load_generation.wrapping_add(1);
                record.error = None;
                record.state = if record.loaded.is_some() {
                    AssetState::Loaded
                } else {
                    AssetState::Loading
                };
            }
            AssetState::Loading
            | AssetState::Loaded
            | AssetState::WaitingDependencies
            | AssetState::Installing
            | AssetState::Installed => {}
        }
    }

    fn normalize_dependencies(dependencies: Vec<AssetId>) -> Vec<AssetId> {
        let mut unique = Vec::with_capacity(dependencies.len());
        for dependency in dependencies {
            if !unique.contains(&dependency) {
                unique.push(dependency);
            }
        }
        unique
    }

    fn replace_held_dependencies(&mut self, id: AssetId, new_dependencies: Vec<AssetId>) {
        let new_dependencies = Self::normalize_dependencies(new_dependencies);
        let old_dependencies = match self.records.get_mut(&id) {
            Some(record) => {
                std::mem::replace(&mut record.held_dependencies, new_dependencies.clone())
            }
            None => return,
        };

        for dependency in &old_dependencies {
            if new_dependencies.contains(dependency) {
                continue;
            }
            self.release_dependency_reference(*dependency);
        }

        for dependency in &new_dependencies {
            if old_dependencies.contains(dependency) {
                continue;
            }

            let Some(entry) = self.manifest.entry(*dependency) else {
                continue;
            };

            let record = self
                .records
                .entry(*dependency)
                .or_insert_with(|| AssetRecord::new(*dependency, entry.asset_type.clone()));
            record.dependency_ref_count += 1;
            Self::activate_record_for_load(record, None);
        }
    }

    fn release_held_dependencies(&mut self, id: AssetId) {
        self.replace_held_dependencies(id, Vec::new());
    }

    fn release_dependency_reference(&mut self, id: AssetId) {
        if let Some(record) = self.records.get_mut(&id) {
            record.dependency_ref_count = record.dependency_ref_count.saturating_sub(1);
        }
        self.schedule_release_if_unused(id);
    }

    fn is_referenced(&self, id: AssetId) -> bool {
        self.records
            .get(&id)
            .is_some_and(|record| record.strong_ref_count > 0 || record.dependency_ref_count > 0)
    }

    fn dependency_links(&self, id: AssetId) -> Vec<AssetId> {
        if let Some(record) = self.records.get(&id) {
            if !record.dependencies.is_empty() {
                return record.dependencies.clone();
            }
        }

        self.manifest
            .entry(id)
            .map(|entry| entry.dependencies.clone())
            .unwrap_or_default()
    }

    fn find_dependency_cycle_from(
        &self,
        id: AssetId,
        stack: &mut Vec<AssetId>,
        visited: &mut HashSet<AssetId>,
    ) -> Option<Vec<AssetId>> {
        if let Some(index) = stack.iter().position(|existing| *existing == id) {
            let mut cycle = stack[index..].to_vec();
            cycle.push(id);
            return Some(cycle);
        }

        if !visited.insert(id) {
            return None;
        }

        stack.push(id);
        for dependency in self.dependency_links(id) {
            if let Some(cycle) = self.find_dependency_cycle_from(dependency, stack, visited) {
                return Some(cycle);
            }
        }
        stack.pop();
        None
    }

    fn find_dependency_cycle(&self, id: AssetId) -> Option<Vec<AssetId>> {
        let mut stack = Vec::new();
        let mut visited = HashSet::new();
        self.find_dependency_cycle_from(id, &mut stack, &mut visited)
    }

    fn schedule_release_if_unused(&mut self, id: AssetId) {
        if self
            .records
            .get(&id)
            .is_some_and(|record| record.reload_pending)
        {
            return;
        }

        if self.is_referenced(id) {
            return;
        }

        self.release_held_dependencies(id);

        let Some(record) = self.records.get_mut(&id) else {
            return;
        };

        let mut pushed_unloaded = false;
        match record.state {
            AssetState::Installed => record.state = AssetState::Uninstalling,
            AssetState::Loaded
            | AssetState::WaitingDependencies
            | AssetState::Installing
            | AssetState::Failed => record.state = AssetState::Unloading,
            AssetState::Unloaded | AssetState::Loading => {
                if matches!(record.state, AssetState::Loading) {
                    record.load_generation = record.load_generation.wrapping_add(1);
                }
                record.loaded = None;
                record.installed = None;
                record.error = None;
                record.loaded_entry_fingerprint = None;
                record.loaded_cooked_hash = None;
                record.reload_pending = false;
                record.state = AssetState::Unloaded;
                pushed_unloaded = true;
            }
            AssetState::Uninstalling | AssetState::Unloading => {}
        }
        if pushed_unloaded {
            self.push_event(id, AssetEventKind::Unloaded, AssetState::Unloaded);
        }
    }

    fn record_depends_on(&self, id: AssetId, dependency: AssetId) -> bool {
        self.records.get(&id).is_some_and(|record| {
            if record.held_dependencies.contains(&dependency) {
                return true;
            }
            record.dependencies.contains(&dependency)
        })
    }

    fn dependent_reload_closure(&self, roots: &[AssetId]) -> Vec<AssetId> {
        let mut impacted = Vec::new();
        let mut seen = HashSet::new();
        let mut queue: VecDeque<_> = roots.iter().copied().collect();

        while let Some(id) = queue.pop_front() {
            if !seen.insert(id) {
                continue;
            }
            impacted.push(id);

            let dependents: Vec<_> = self
                .records
                .keys()
                .copied()
                .filter(|candidate| self.record_depends_on(*candidate, id))
                .collect();
            for dependent in dependents {
                queue.push_back(dependent);
            }
        }

        impacted
    }

    fn queue_record_reload(&mut self, id: AssetId) {
        if let Some(record) = self.records.get_mut(&id) {
            record.reload_pending = true;
        }
    }

    fn prepare_record_reload(&mut self, id: AssetId) {
        self.release_held_dependencies(id);

        let Some(record) = self.records.get_mut(&id) else {
            return;
        };

        record.load_generation = record.load_generation.wrapping_add(1);
        record.loaded = None;
        record.installed = None;
        record.dependencies.clear();
        record.error = None;
        record.loaded_entry_fingerprint = None;
        record.loaded_cooked_hash = None;

        if self.manifest.entry(id).is_some() {
            record.state = AssetState::Loading;
            self.push_event(id, AssetEventKind::ReloadQueued, AssetState::Loading);
        } else {
            record.error = Some(AssetError::AssetNotFound { id });
            record.state = AssetState::Failed;
            record.reload_pending = false;
            self.push_event(id, AssetEventKind::Failed, AssetState::Failed);
        }
    }

    fn reload_changed(&mut self) -> Result<Vec<AssetId>, AssetError> {
        let manifest = load_manifest(&self.config)?;
        self.set_manifest(manifest);

        let ids: Vec<_> = self.records.keys().copied().collect();
        let mut changed_roots = Vec::new();

        for id in ids {
            if !self.should_track_for_reload(id) {
                continue;
            }

            let Some(entry) = self.manifest.entry(id) else {
                continue;
            };
            let Some(record) = self.records.get(&id) else {
                continue;
            };

            let current_entry_fingerprint = manifest_entry_fingerprint(entry)?;
            let current_cooked_hash =
                file_hash(&self.config.cooked_root().join(&entry.cooked_path))?;
            if record.loaded_entry_fingerprint.as_ref() != Some(&current_entry_fingerprint)
                || record.loaded_cooked_hash.as_ref() != Some(&current_cooked_hash)
            {
                changed_roots.push(id);
            }
        }

        let impacted = self.dependent_reload_closure(&changed_roots);
        for id in &impacted {
            self.queue_record_reload(*id);
        }
        for id in &impacted {
            self.prepare_record_reload(*id);
        }

        Ok(impacted)
    }

    fn entry_for_record(&self, id: AssetId) -> Result<AssetManifestEntry, AssetError> {
        if let Some(entry) = self.manifest.entry(id) {
            return Ok(entry.clone());
        }

        let path = self
            .records
            .get(&id)
            .and_then(|record| record.raw_source_path.as_ref())
            .ok_or(AssetError::AssetNotFound { id })?;
        let record = self
            .records
            .get(&id)
            .ok_or(AssetError::AssetNotFound { id })?;
        if record.asset_type == FontAsset::TYPE {
            Ok(raw_font_manifest_entry(id, self.config.source_key(path)))
        } else {
            Ok(raw_texture_manifest_entry(
                id,
                self.config.source_key(path),
                serde_json::json!({ "srgb": true }),
            ))
        }
    }

    fn spawn_load_record(&mut self, id: AssetId) -> Result<bool, AssetError> {
        let entry = self.entry_for_record(id)?;
        let generation = self
            .records
            .get(&id)
            .map(|record| record.load_generation)
            .ok_or(AssetError::AssetNotFound { id })?;

        if !self.inflight_loads.insert((id, generation)) {
            return Ok(false);
        }

        let raw_source_path = self
            .records
            .get(&id)
            .and_then(|record| record.raw_source_path.clone());
        let cooked_path = self.config.cooked_root().join(&entry.cooked_path);
        let asset_root = self.config.asset_root.clone();
        let cooked_root = self.config.cooked_root();
        let factory = self
            .factories
            .get(&entry.asset_type)
            .cloned()
            .ok_or_else(|| AssetError::FactoryNotRegistered {
                asset_type: entry.asset_type.clone(),
            })?;
        let tx = self.load_tx.clone();

        std::thread::spawn(move || {
            let result = (|| {
                if let Some(raw_source_path) = raw_source_path {
                    let bytes = std::fs::read(&raw_source_path)
                        .map_err(|error| map_raw_source_read_error(&raw_source_path, error))?;
                    let loaded = load_raw_source_asset(&entry, &raw_source_path, &bytes)?;
                    Ok((loaded, hash_bytes(&bytes)))
                } else {
                    let bytes = std::fs::read(&cooked_path)
                        .map_err(|error| map_read_error(id, &cooked_path, error))?;
                    let cooked_hash = hash_bytes(&bytes);
                    let loaded = factory.load(AssetLoadContext {
                        asset_id: id,
                        entry: &entry,
                        bytes: &bytes,
                        asset_root: &asset_root,
                        cooked_root: &cooked_root,
                    })?;
                    Ok((loaded, cooked_hash))
                }
            })();

            let completion = match result {
                Ok((loaded, cooked_hash)) => CompletedLoad {
                    id,
                    generation,
                    entry,
                    cooked_hash: Some(cooked_hash),
                    result: Ok(loaded),
                },
                Err(error) => CompletedLoad {
                    id,
                    generation,
                    entry,
                    cooked_hash: None,
                    result: Err(error),
                },
            };

            let _ = tx.send(completion);
        });

        Ok(true)
    }

    fn finish_load_completion(&mut self, completion: CompletedLoad) -> Result<bool, AssetError> {
        self.inflight_loads
            .remove(&(completion.id, completion.generation));

        let Some(record) = self.records.get(&completion.id) else {
            return Ok(false);
        };
        if record.load_generation != completion.generation || record.state != AssetState::Loading {
            return Ok(false);
        }

        match completion.result {
            Ok(loaded) => {
                let dependencies =
                    Self::normalize_dependencies(if loaded.dependencies.is_empty() {
                        completion.entry.dependencies.clone()
                    } else {
                        loaded.dependencies
                    });
                let entry_fingerprint = manifest_entry_fingerprint(&completion.entry)?;
                self.replace_held_dependencies(completion.id, dependencies.clone());

                let record = self
                    .records
                    .get_mut(&completion.id)
                    .expect("record should exist");
                record.loaded = Some(loaded.loaded);
                record.dependencies = dependencies;
                record.error = None;
                record.loaded_entry_fingerprint = Some(entry_fingerprint);
                record.loaded_cooked_hash = completion.cooked_hash;
                record.state = AssetState::Loaded;
                Ok(true)
            }
            Err(error) => {
                self.release_held_dependencies(completion.id);
                let record = self
                    .records
                    .get_mut(&completion.id)
                    .expect("record should exist");
                record.loaded = None;
                record.installed = None;
                record.error = Some(error.clone());
                record.loaded_entry_fingerprint = None;
                record.loaded_cooked_hash = None;
                record.reload_pending = false;
                record.state = AssetState::Failed;
                self.push_event(completion.id, AssetEventKind::Failed, AssetState::Failed);
                self.schedule_release_if_unused(completion.id);
                Err(error)
            }
        }
    }

    fn drain_load_completions(&mut self) -> Result<usize, AssetError> {
        let mut completed = 0usize;
        loop {
            match self.load_rx.try_recv() {
                Ok(completion) => {
                    if self.finish_load_completion(completion)? {
                        completed += 1;
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => break,
            }
        }
        Ok(completed)
    }

    fn apply_update(&mut self) -> Result<(), AssetError> {
        self.drain_handle_releases();

        while let Some(request) = self.requests.pop_front() {
            let record = self
                .records
                .get_mut(&request.id)
                .ok_or(AssetError::AssetNotFound { id: request.id })?;
            Self::activate_record_for_load(record, request.requested_type);
        }

        let _ = self.drain_load_completions()?;
        let mut installs_remaining = self.config.install_budget_per_update.unwrap_or(usize::MAX);

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
                        let texture_asset = self.records.get(&id).is_some_and(|record| {
                            record.raw_source_path.is_some()
                                || record.asset_type == TextureAsset::TYPE
                        });
                        if self.config.background_loading || texture_asset {
                            if self.spawn_load_record(id)? {
                                progressed = true;
                            }
                        } else {
                            self.load_record(id)?;
                            progressed = true;
                        }
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
                        if installs_remaining > 0 {
                            self.install_record(id)?;
                            installs_remaining -= 1;
                            progressed = true;
                        }
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
                        record.loaded_entry_fingerprint = None;
                        record.loaded_cooked_hash = None;
                        record.reload_pending = false;
                        record.state = AssetState::Unloaded;
                        self.push_event(id, AssetEventKind::Unloaded, AssetState::Unloaded);
                        progressed = true;
                    }
                    AssetState::Unloaded | AssetState::Installed | AssetState::Failed => {}
                }
            }

            if self.drain_load_completions()? > 0 {
                progressed = true;
            }

            if !progressed {
                break;
            }
        }

        Ok(())
    }

    fn insert_runtime<T: Asset>(&mut self, id: AssetId, asset: T) {
        let installed: Arc<dyn Any + Send + Sync> = Arc::new(asset);
        self.records.insert(
            id,
            AssetRecord::new_runtime(T::TYPE.to_string(), TypeId::of::<T>(), installed),
        );
        self.push_event(id, AssetEventKind::Installed, AssetState::Installed);
    }

    fn replace_runtime<T: Asset>(&mut self, id: AssetId, asset: T) -> Result<(), AssetError> {
        let installed: Arc<dyn Any + Send + Sync> = Arc::new(asset);
        self.release_held_dependencies(id);

        match self.records.get_mut(&id) {
            Some(record) => {
                if record
                    .requested_type
                    .is_some_and(|type_id| type_id != TypeId::of::<T>())
                {
                    return Err(AssetError::AssetTypeMismatch {
                        id,
                        expected: T::TYPE,
                        actual: record.asset_type.clone(),
                    });
                }
                record.asset_type = T::TYPE.to_string();
                record.state = AssetState::Installed;
                record.requested_type = Some(TypeId::of::<T>());
                record.dependencies.clear();
                record.held_dependencies.clear();
                record.loaded = Some(installed.clone());
                record.installed = Some(installed);
                record.error = None;
                record.loaded_entry_fingerprint = None;
                record.loaded_cooked_hash = None;
                record.reload_pending = false;
                record.runtime = true;
                record.raw_source_path = None;
            }
            None => {
                self.records.insert(
                    id,
                    AssetRecord::new_runtime(T::TYPE.to_string(), TypeId::of::<T>(), installed),
                );
            }
        }

        self.push_event(id, AssetEventKind::Installed, AssetState::Installed);
        Ok(())
    }

    fn load_record(&mut self, id: AssetId) -> Result<(), AssetError> {
        let entry = self.entry_for_record(id)?;
        let raw_source_path = self
            .records
            .get(&id)
            .and_then(|record| record.raw_source_path.clone());

        if let Some(raw_source_path) = raw_source_path {
            let bytes = std::fs::read(&raw_source_path)
                .map_err(|error| map_raw_source_read_error(&raw_source_path, error))?;
            let result = load_raw_source_asset(&entry, &raw_source_path, &bytes);

            match result {
                Ok(loaded) => {
                    let entry_fingerprint = manifest_entry_fingerprint(&entry)?;
                    let source_hash = hash_bytes(&bytes);
                    self.replace_held_dependencies(id, Vec::new());

                    let record = self.records.get_mut(&id).expect("record should exist");
                    record.loaded = Some(loaded.loaded);
                    record.dependencies.clear();
                    record.error = None;
                    record.loaded_entry_fingerprint = Some(entry_fingerprint);
                    record.loaded_cooked_hash = Some(source_hash);
                    record.state = AssetState::Loaded;
                    return Ok(());
                }
                Err(error) => {
                    self.release_held_dependencies(id);
                    let record = self.records.get_mut(&id).expect("record should exist");
                    record.loaded = None;
                    record.installed = None;
                    record.error = Some(error.clone());
                    record.loaded_entry_fingerprint = None;
                    record.loaded_cooked_hash = None;
                    record.reload_pending = false;
                    record.state = AssetState::Failed;
                    self.push_event(id, AssetEventKind::Failed, AssetState::Failed);
                    self.schedule_release_if_unused(id);
                    return Err(error);
                }
            }
        }

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

        match result {
            Ok(loaded) => {
                let dependencies =
                    Self::normalize_dependencies(if loaded.dependencies.is_empty() {
                        entry.dependencies.clone()
                    } else {
                        loaded.dependencies
                    });
                let entry_fingerprint = manifest_entry_fingerprint(&entry)?;
                let cooked_hash = hash_bytes(&bytes);
                self.replace_held_dependencies(id, dependencies.clone());

                let record = self.records.get_mut(&id).expect("record should exist");
                record.loaded = Some(loaded.loaded);
                record.dependencies = dependencies;
                record.error = None;
                record.loaded_entry_fingerprint = Some(entry_fingerprint);
                record.loaded_cooked_hash = Some(cooked_hash);
                record.state = AssetState::Loaded;
                Ok(())
            }
            Err(error) => {
                self.release_held_dependencies(id);
                let record = self.records.get_mut(&id).expect("record should exist");
                record.loaded = None;
                record.installed = None;
                record.error = Some(error.clone());
                record.loaded_entry_fingerprint = None;
                record.loaded_cooked_hash = None;
                record.reload_pending = false;
                record.state = AssetState::Failed;
                self.push_event(id, AssetEventKind::Failed, AssetState::Failed);
                self.schedule_release_if_unused(id);
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
                self.release_held_dependencies(id);
                let error = AssetError::MissingDependency { id, dependency };
                let record = self.records.get_mut(&id).expect("record should exist");
                record.loaded = None;
                record.installed = None;
                record.error = Some(error.clone());
                record.state = AssetState::Failed;
                self.push_event(id, AssetEventKind::Failed, AssetState::Failed);
                return Err(error);
            };

            if !self.records.contains_key(&dependency) {
                self.records.insert(
                    dependency,
                    AssetRecord::new(dependency, entry.asset_type.clone()),
                );
                let record = self
                    .records
                    .get_mut(&dependency)
                    .expect("record should exist");
                Self::activate_record_for_load(record, None);
                waiting = true;
                continue;
            }

            match self.records.get(&dependency).map(|record| record.state) {
                Some(AssetState::Installed) => {}
                Some(AssetState::Failed) => {
                    self.release_held_dependencies(id);
                    let error = AssetError::DependencyFailed { id, dependency };
                    let record = self.records.get_mut(&id).expect("record should exist");
                    record.loaded = None;
                    record.installed = None;
                    record.error = Some(error.clone());
                    record.state = AssetState::Failed;
                    self.push_event(id, AssetEventKind::Failed, AssetState::Failed);
                    return Err(error);
                }
                Some(_) | None => waiting = true,
            }
        }

        if let Some(cycle) = self.find_dependency_cycle(id) {
            self.release_held_dependencies(id);
            let error = AssetError::DependencyCycle { cycle };
            let record = self.records.get_mut(&id).expect("record should exist");
            record.loaded = None;
            record.installed = None;
            record.error = Some(error.clone());
            record.state = AssetState::Failed;
            self.push_event(id, AssetEventKind::Failed, AssetState::Failed);
            return Err(error);
        }

        Ok(Some(if waiting {
            AssetState::WaitingDependencies
        } else {
            AssetState::Installing
        }))
    }

    fn install_record(&mut self, id: AssetId) -> Result<(), AssetError> {
        let entry = self.entry_for_record(id)?;
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

        match result {
            Ok(installed) => {
                let record = self.records.get_mut(&id).expect("record should exist");
                record.installed = Some(installed);
                record.error = None;
                record.reload_pending = false;
                record.state = AssetState::Installed;
                self.push_event(id, AssetEventKind::Installed, AssetState::Installed);
                self.schedule_release_if_unused(id);
                Ok(())
            }
            Err(error) => {
                self.release_held_dependencies(id);
                let record = self.records.get_mut(&id).expect("record should exist");
                record.loaded = None;
                record.installed = None;
                record.error = Some(error.clone());
                record.loaded_entry_fingerprint = None;
                record.loaded_cooked_hash = None;
                record.reload_pending = false;
                record.state = AssetState::Failed;
                self.push_event(id, AssetEventKind::Failed, AssetState::Failed);
                self.schedule_release_if_unused(id);
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

#[derive(Clone, Debug, PartialEq, Eq)]
struct RawTextureRequest {
    key: String,
    path: PathBuf,
}

impl RawTextureRequest {
    fn new(config: &AssetConfig, path: &Path) -> Self {
        let path = if path.is_absolute() {
            path.to_path_buf()
        } else if config.asset_root.is_absolute() {
            config.asset_root.join(path)
        } else {
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(&config.asset_root)
                .join(path)
        };
        let key = config.source_key(&path);
        Self { key, path }
    }
}

#[derive(Clone)]
struct AssetRecord {
    asset_type: String,
    state: AssetState,
    requested_type: Option<TypeId>,
    strong_ref_count: usize,
    dependency_ref_count: usize,
    dependencies: Vec<AssetId>,
    held_dependencies: Vec<AssetId>,
    loaded: Option<Arc<dyn Any + Send + Sync>>,
    installed: Option<Arc<dyn Any + Send + Sync>>,
    error: Option<AssetError>,
    loaded_entry_fingerprint: Option<String>,
    loaded_cooked_hash: Option<String>,
    reload_pending: bool,
    load_generation: u64,
    runtime: bool,
    raw_source_path: Option<PathBuf>,
}

impl AssetRecord {
    fn new(_id: AssetId, asset_type: String) -> Self {
        Self {
            asset_type,
            state: AssetState::Unloaded,
            requested_type: None,
            strong_ref_count: 0,
            dependency_ref_count: 0,
            dependencies: Vec::new(),
            held_dependencies: Vec::new(),
            loaded: None,
            installed: None,
            error: None,
            loaded_entry_fingerprint: None,
            loaded_cooked_hash: None,
            reload_pending: false,
            load_generation: 0,
            runtime: false,
            raw_source_path: None,
        }
    }

    fn new_raw_texture(path: PathBuf) -> Self {
        Self {
            asset_type: TextureAsset::TYPE.to_string(),
            state: AssetState::Unloaded,
            requested_type: Some(TypeId::of::<TextureAsset>()),
            strong_ref_count: 0,
            dependency_ref_count: 0,
            dependencies: Vec::new(),
            held_dependencies: Vec::new(),
            loaded: None,
            installed: None,
            error: None,
            loaded_entry_fingerprint: None,
            loaded_cooked_hash: None,
            reload_pending: false,
            load_generation: 0,
            runtime: false,
            raw_source_path: Some(path),
        }
    }

    fn new_raw_font(path: PathBuf) -> Self {
        Self {
            asset_type: FontAsset::TYPE.to_string(),
            state: AssetState::Unloaded,
            requested_type: Some(TypeId::of::<FontAsset>()),
            strong_ref_count: 0,
            dependency_ref_count: 0,
            dependencies: Vec::new(),
            held_dependencies: Vec::new(),
            loaded: None,
            installed: None,
            error: None,
            loaded_entry_fingerprint: None,
            loaded_cooked_hash: None,
            reload_pending: false,
            load_generation: 0,
            runtime: false,
            raw_source_path: Some(path),
        }
    }

    fn new_runtime(
        asset_type: String,
        requested_type: TypeId,
        installed: Arc<dyn Any + Send + Sync>,
    ) -> Self {
        Self {
            asset_type,
            state: AssetState::Installed,
            requested_type: Some(requested_type),
            strong_ref_count: 0,
            dependency_ref_count: 0,
            dependencies: Vec::new(),
            held_dependencies: Vec::new(),
            loaded: Some(installed.clone()),
            installed: Some(installed),
            error: None,
            loaded_entry_fingerprint: None,
            loaded_cooked_hash: None,
            reload_pending: false,
            load_generation: 0,
            runtime: true,
            raw_source_path: None,
        }
    }
}

struct CompletedLoad {
    id: AssetId,
    generation: u64,
    entry: AssetManifestEntry,
    cooked_hash: Option<String>,
    result: Result<crate::asset::LoadedAsset<Arc<dyn Any + Send + Sync>>, AssetError>,
}

fn raw_texture_manifest_entry(
    id: AssetId,
    source_path: String,
    import_settings: serde_json::Value,
) -> AssetManifestEntry {
    AssetManifestEntry {
        asset_id: id,
        asset_type: TextureAsset::TYPE.to_string(),
        importer: "texture.raw".to_string(),
        cooker: "texture.raw_rgba8".to_string(),
        version: 1,
        source_path,
        cooked_path: String::new(),
        dependencies: Vec::new(),
        import_settings,
    }
}

fn raw_font_manifest_entry(id: AssetId, source_path: String) -> AssetManifestEntry {
    AssetManifestEntry {
        asset_id: id,
        asset_type: FontAsset::TYPE.to_string(),
        importer: "font.raw".to_string(),
        cooker: "font.raw_bytes".to_string(),
        version: 1,
        source_path,
        cooked_path: String::new(),
        dependencies: Vec::new(),
        import_settings: serde_json::Value::Null,
    }
}

fn load_raw_source_asset(
    entry: &AssetManifestEntry,
    raw_source_path: &Path,
    bytes: &[u8],
) -> Result<crate::asset::LoadedAsset<Arc<dyn Any + Send + Sync>>, AssetError> {
    if entry.asset_type == TextureAsset::TYPE {
        let texture = decode_texture_source_bytes(raw_source_path, bytes, TextureColorSpace::Srgb)?;
        return Ok(crate::asset::LoadedAsset::new(
            Arc::new(texture) as Arc<dyn Any + Send + Sync>
        ));
    }
    if entry.asset_type == FontAsset::TYPE {
        return Ok(crate::asset::LoadedAsset::new(
            Arc::new(FontAsset::new(Arc::<[u8]>::from(bytes.to_vec())))
                as Arc<dyn Any + Send + Sync>,
        ));
    }
    Err(AssetError::Unsupported {
        message: format!(
            "raw source loading is not supported for asset type `{}`",
            entry.asset_type
        ),
    })
}

fn manifest_entry_fingerprint(entry: &AssetManifestEntry) -> Result<String, AssetError> {
    let fingerprint = serde_json::json!({
        "asset_id": entry.asset_id.to_string(),
        "asset_type": entry.asset_type,
        "importer": entry.importer,
        "cooker": entry.cooker,
        "version": entry.version,
        "source_path": entry.source_path,
        "cooked_path": entry.cooked_path,
        "dependencies": entry
            .dependencies
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        "import_settings": entry.import_settings,
    });
    let bytes = serde_json::to_vec(&fingerprint).map_err(|error| AssetError::Internal {
        message: format!("failed to serialize asset manifest fingerprint: {error}"),
    })?;
    Ok(hash_bytes(&bytes))
}

fn file_hash(path: &Path) -> Result<String, AssetError> {
    let bytes = std::fs::read(path).map_err(|error| AssetError::Io {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;
    Ok(hash_bytes(&bytes))
}

fn hash_bytes(bytes: &[u8]) -> String {
    const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;

    let mut hash = FNV_OFFSET_BASIS;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    format!("{hash:016x}")
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

fn map_raw_source_read_error(path: &PathBuf, error: std::io::Error) -> AssetError {
    AssetError::Io {
        path: path.clone(),
        message: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset::{AssetManifestEntry, LoadedAsset};
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

        fn install(
            &self,
            loaded: &Self::Loaded,
            _ctx: AssetInstallContext<'_>,
        ) -> Result<Self::Asset, AssetError> {
            Ok(DummyAsset(loaded.0.clone()))
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

        fn install(
            &self,
            loaded: &Self::Loaded,
            _ctx: AssetInstallContext<'_>,
        ) -> Result<Self::Asset, AssetError> {
            Ok(DummyAsset(loaded.0.clone()))
        }
    }

    #[derive(Clone)]
    struct CountingFactory {
        loads: Arc<Mutex<HashMap<AssetId, usize>>>,
        installs: Arc<Mutex<HashMap<AssetId, usize>>>,
    }

    impl CountingFactory {
        fn new() -> Self {
            Self {
                loads: Arc::new(Mutex::new(HashMap::default())),
                installs: Arc::new(Mutex::new(HashMap::default())),
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

        fn install(
            &self,
            loaded: &Self::Loaded,
            ctx: AssetInstallContext<'_>,
        ) -> Result<Self::Asset, AssetError> {
            let mut installs = self
                .installs
                .lock()
                .expect("counting installs mutex poisoned");
            *installs.entry(ctx.asset_id).or_insert(0) += 1;
            Ok(DummyAsset(loaded.0.clone()))
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
        let handle = server.load_id::<TextureAsset>(asset_id)?;
        server.update()?;
        assert_ne!(server.state(&handle), AssetState::Installed);

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
        let changed = server.reload_changed()?;
        let changed_ids: HashSet<_> = changed.into_iter().collect();
        assert_eq!(changed_ids, HashSet::from([parent_id, dependency_id]));

        server.update()?;
        assert_eq!(factory.load_count(parent_id), 2);
        assert_eq!(factory.load_count(dependency_id), 2);
        assert_eq!(factory.install_count(parent_id), 2);
        assert_eq!(factory.install_count(dependency_id), 2);
        Ok(())
    }
}
