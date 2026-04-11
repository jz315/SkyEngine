use std::any::{Any, TypeId};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
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

    pub fn reload_changed(&self) -> Result<Vec<AssetId>, AssetError> {
        let mut inner = self.inner.lock().expect("asset server mutex poisoned");
        inner.reload_changed()
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
    load_tx: Sender<CompletedLoad>,
    load_rx: Receiver<CompletedLoad>,
    inflight_loads: HashSet<(AssetId, u64)>,
}

impl AssetServerInner {
    fn new(config: AssetConfig, manifest: AssetRegistryManifest) -> Self {
        let (load_tx, load_rx) = mpsc::channel();
        Self {
            config,
            manifest: ManifestIndex::new(manifest),
            factories: HashMap::default(),
            records: HashMap::default(),
            requests: VecDeque::new(),
            unloads: VecDeque::new(),
            load_tx,
            load_rx,
            inflight_loads: HashSet::default(),
        }
    }

    fn set_manifest(&mut self, manifest: AssetRegistryManifest) {
        self.manifest = ManifestIndex::new(manifest);

        let ids: Vec<_> = self.records.keys().copied().collect();
        for id in ids {
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

    fn should_track_for_reload(&self, id: AssetId) -> bool {
        self.records.get(&id).is_some_and(|record| {
            record.direct_request_count > 0
                || record.dependency_ref_count > 0
                || record.installed.is_some()
                || record.loaded.is_some()
                || record.reload_pending
        })
    }

    fn queue_request(&mut self, id: AssetId, requested_type: Option<TypeId>) {
        let record = self.records.entry(id).or_insert_with(|| {
            let entry = self
                .manifest
                .entry(id)
                .expect("asset record must exist in manifest");
            AssetRecord::new(id, entry.asset_type.clone())
        });
        record.direct_request_count += 1;

        if record.requested_type.is_none() {
            record.requested_type = requested_type;
        }

        self.requests.push_back(AssetRequest { id, requested_type });
    }

    fn queue_unload(&mut self, id: AssetId) {
        self.unloads.push_back(id);
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
        self.records.get(&id).is_some_and(|record| {
            record.direct_request_count > 0 || record.dependency_ref_count > 0
        })
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
            }
            AssetState::Uninstalling | AssetState::Unloading => {}
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
        } else {
            record.error = Some(AssetError::AssetNotFound { id });
            record.state = AssetState::Failed;
            record.reload_pending = false;
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

    fn spawn_load_record(&mut self, id: AssetId) -> Result<bool, AssetError> {
        let entry = self
            .manifest
            .entry(id)
            .ok_or(AssetError::AssetNotFound { id })?
            .clone();
        let generation = self
            .records
            .get(&id)
            .map(|record| record.load_generation)
            .ok_or(AssetError::AssetNotFound { id })?;

        if !self.inflight_loads.insert((id, generation)) {
            return Ok(false);
        }

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
            Self::activate_record_for_load(record, request.requested_type);
        }

        while let Some(id) = self.unloads.pop_front() {
            if let Some(record) = self.records.get_mut(&id) {
                record.direct_request_count = record.direct_request_count.saturating_sub(1);
            }
            self.schedule_release_if_unused(id);
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
                        if self.config.background_loading {
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
                        progressed = true;
                    }
                    AssetState::Unloaded | AssetState::Installed | AssetState::Failed => {}
                }
            }

            if self.config.background_loading && self.drain_load_completions()? > 0 {
                progressed = true;
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
            return Err(error);
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

        match result {
            Ok(installed) => {
                let record = self.records.get_mut(&id).expect("record should exist");
                record.installed = Some(installed);
                record.error = None;
                record.reload_pending = false;
                record.state = AssetState::Installed;
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

#[derive(Clone)]
struct AssetRecord {
    asset_type: String,
    state: AssetState,
    requested_type: Option<TypeId>,
    direct_request_count: usize,
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
}

impl AssetRecord {
    fn new(_id: AssetId, asset_type: String) -> Self {
        Self {
            asset_type,
            state: AssetState::Unloaded,
            requested_type: None,
            direct_request_count: 0,
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
        }
    }
}

struct CompletedLoad {
    id: AssetId,
    generation: u64,
    entry: crate::asset::AssetManifestEntry,
    cooked_hash: Option<String>,
    result: Result<crate::asset::LoadedAsset<Arc<dyn Any + Send + Sync>>, AssetError>,
}

fn manifest_entry_fingerprint(
    entry: &crate::asset::AssetManifestEntry,
) -> Result<String, AssetError> {
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

    #[test]
    fn multiple_loads_require_matching_unloads() -> Result<(), Box<dyn std::error::Error>> {
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
        let first = server.load::<DummyAsset>(asset_id)?;
        let second = server.load::<DummyAsset>(asset_id)?;
        server.update()?;

        server.unload(&first);
        server.update()?;
        assert_eq!(server.state(&second), AssetState::Installed);
        assert_eq!(server.get(&second)?.0, "ready");

        server.unload(&second);
        server.update()?;
        assert_eq!(server.state(&second), AssetState::Unloaded);
        Ok(())
    }

    #[test]
    fn dependencies_load_transitively_and_unload_with_parent(
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

        let server = AssetServer::new(config)?;
        server.register_factory(DummyFactory);
        let parent = server.load::<DummyAsset>(parent_id)?;
        let dependency = Handle::<DummyAsset>::new(dependency_id);
        server.update()?;

        assert_eq!(server.state(&parent), AssetState::Installed);
        assert_eq!(server.state(&dependency), AssetState::Installed);

        server.unload(&parent);
        server.update()?;
        assert_eq!(server.state(&parent), AssetState::Unloaded);
        assert_eq!(server.state(&dependency), AssetState::Unloaded);
        Ok(())
    }

    #[test]
    fn direct_dependency_request_survives_parent_unload() -> Result<(), Box<dyn std::error::Error>>
    {
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

        let server = AssetServer::new(config)?;
        server.register_factory(DummyFactory);
        let parent = server.load::<DummyAsset>(parent_id)?;
        let dependency = server.load::<DummyAsset>(dependency_id)?;
        server.update()?;

        server.unload(&parent);
        server.update()?;
        assert_eq!(server.state(&parent), AssetState::Unloaded);
        assert_eq!(server.state(&dependency), AssetState::Installed);

        server.unload(&dependency);
        server.update()?;
        assert_eq!(server.state(&dependency), AssetState::Unloaded);
        Ok(())
    }

    #[test]
    fn load_by_path_uses_manifest_source_lookup() -> Result<(), Box<dyn std::error::Error>> {
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

        let server = AssetServer::new(config)?;
        server.register_factory(DummyFactory);
        let handle =
            server.load_by_path::<DummyAsset>(asset_root.join("nested").join("clip.dummy"))?;
        server.update()?;

        assert_eq!(handle.id(), asset_id);
        assert_eq!(server.get(&handle)?.0, "ready");
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

        let server = AssetServer::new(config.clone())?;
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

        let server = AssetServer::new(config)?;
        server.register_factory(DummyFactory);
        let handle = server.load::<DummyAsset>(first_id)?;
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

        let server = AssetServer::new(config)?;
        server.register_factory(SlowFactory {
            delay: Duration::from_millis(60),
        });
        let handle = server.load::<DummyAsset>(asset_id)?;

        server.update()?;
        assert_eq!(server.state(&handle), AssetState::Loading);

        std::thread::sleep(Duration::from_millis(90));
        server.update()?;
        assert_eq!(server.state(&handle), AssetState::Installed);
        assert_eq!(server.get(&handle)?.0, "ready");
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

        let server = AssetServer::new(config)?;
        server.register_factory(DummyFactory);
        let first = server.load::<DummyAsset>(first_id)?;
        let second = server.load::<DummyAsset>(second_id)?;

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

        let server = AssetServer::new(config)?;
        server.register_factory(DummyFactory);
        let handle = server.load::<DummyAsset>(asset_id)?;
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

        let server = AssetServer::new(config)?;
        server.register_factory(DummyFactory);
        let handle = server.load::<DummyAsset>(asset_id)?;
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

        let server = AssetServer::new(config)?;
        let factory = CountingFactory::new();
        server.register_factory(factory.clone());
        let parent = server.load::<DummyAsset>(parent_id)?;
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
