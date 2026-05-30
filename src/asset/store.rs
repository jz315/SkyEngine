use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::font::FontAsset;
use super::install::AssetInstallTask;
use super::lease::AssetDependencyLeases;
use super::texture::TextureAsset;
use super::types::{
    Asset, AssetError, AssetFailurePhase, AssetId, AssetRequestProgress, AssetState,
};

#[derive(Default)]
pub(crate) struct AssetStore {
    pub(crate) records: HashMap<AssetId, AssetRecord>,
    pub(crate) raw_textures: HashMap<String, AssetId>,
    pub(crate) raw_fonts: HashMap<String, AssetId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AssetReloadScanRecord {
    Tracked {
        id: AssetId,
        loaded_entry_fingerprint: String,
        loaded_cooked_hash: String,
    },
    Untracked {
        id: AssetId,
    },
}

impl AssetReloadScanRecord {
    pub(crate) fn id(&self) -> AssetId {
        match self {
            Self::Tracked { id, .. } | Self::Untracked { id } => *id,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AssetRecordDiagnostic {
    pub(crate) asset_id: AssetId,
    pub(crate) asset_type: String,
    pub(crate) state: AssetState,
    pub(crate) load_generation: u64,
    pub(crate) strong_ref_count: usize,
    pub(crate) dependency_ref_count: usize,
    pub(crate) dependencies: Vec<AssetId>,
    pub(crate) error: Option<AssetError>,
    pub(crate) failure_phase: Option<AssetFailurePhase>,
    pub(crate) reload_pending: bool,
    pub(crate) install_progress: Option<AssetRequestProgress>,
    pub(crate) state_age: Duration,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct AssetEventRecordContext {
    pub(crate) generation: u64,
    pub(crate) asset_type: String,
    pub(crate) manifest_fingerprint: Option<String>,
    pub(crate) content_hash: Option<String>,
    pub(crate) dependencies: Vec<AssetId>,
    pub(crate) reload_pending: bool,
    pub(crate) failure_phase: Option<AssetFailurePhase>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AssetRecordLoadActivation {
    pub(crate) state: AssetState,
    pub(crate) load_generation: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AssetRecordInstallCompletion {
    pub(crate) dependencies: Vec<AssetId>,
    pub(crate) reloaded: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AssetRawSourceRecord {
    pub(crate) asset_type: String,
    pub(crate) path: PathBuf,
}

impl AssetRecordDiagnostic {
    fn from_record(asset_id: AssetId, record: &AssetRecord, now: Instant) -> Self {
        Self {
            asset_id,
            asset_type: record.asset_type.clone(),
            state: record.state,
            load_generation: record.load_generation,
            strong_ref_count: record.strong_ref_count,
            dependency_ref_count: record.dependency_ref_count,
            dependencies: record.dependencies.clone(),
            error: record.error.clone(),
            failure_phase: record.failure_phase,
            reload_pending: record.reload_pending,
            install_progress: record.install_progress(),
            state_age: record.state_age(now),
        }
    }
}

impl AssetEventRecordContext {
    fn from_record(record: &AssetRecord) -> Self {
        Self {
            generation: record.load_generation,
            asset_type: record.asset_type.clone(),
            manifest_fingerprint: record.loaded_entry_fingerprint.clone(),
            content_hash: record.loaded_cooked_hash.clone(),
            dependencies: record.dependencies.clone(),
            reload_pending: record.reload_pending,
            failure_phase: record.failure_phase,
        }
    }
}

impl AssetStore {
    pub(crate) fn ensure_raw_texture_record(&mut self, key: String, path: PathBuf) -> AssetId {
        match self.raw_textures.get(&key).copied() {
            Some(id) => id,
            None => {
                let id = AssetId::new();
                self.raw_textures.insert(key, id);
                self.records.insert(id, AssetRecord::new_raw_texture(path));
                id
            }
        }
    }

    pub(crate) fn ensure_raw_font_record(&mut self, key: String, path: PathBuf) -> AssetId {
        match self.raw_fonts.get(&key).copied() {
            Some(id) => id,
            None => {
                let id = AssetId::new();
                self.raw_fonts.insert(key, id);
                self.records.insert(id, AssetRecord::new_raw_font(path));
                id
            }
        }
    }

    pub(crate) fn retain_raw_texture_record(&mut self, key: String, path: PathBuf) -> AssetId {
        let id = self.ensure_raw_texture_record(key, path.clone());
        self.retain_direct_with(id, Some(TypeId::of::<TextureAsset>()), || {
            AssetRecord::new_raw_texture(path)
        });
        id
    }

    pub(crate) fn retain_raw_font_record(&mut self, key: String, path: PathBuf) -> AssetId {
        let id = self.ensure_raw_font_record(key, path.clone());
        self.retain_direct_with(id, Some(TypeId::of::<FontAsset>()), || {
            AssetRecord::new_raw_font(path)
        });
        id
    }

    pub(crate) fn insert_runtime_asset<T: Asset>(&mut self, id: AssetId, asset: T) {
        let installed: Arc<dyn Any + Send + Sync> = Arc::new(asset);
        self.records.insert(
            id,
            AssetRecord::new_runtime(T::TYPE.to_string(), TypeId::of::<T>(), installed),
        );
    }

    pub(crate) fn replace_runtime_asset<T: Asset>(
        &mut self,
        id: AssetId,
        asset: T,
    ) -> Result<AssetDependencyLeaseUpdate, AssetError> {
        if let Some(record) = self.records.get(&id) {
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
        }

        let dependency_update = self.replace_held_dependencies(id, Vec::new());
        let installed: Arc<dyn Any + Send + Sync> = Arc::new(asset);
        match self.records.get_mut(&id) {
            Some(record) => {
                record.replace_runtime(T::TYPE.to_string(), TypeId::of::<T>(), installed);
            }
            None => {
                self.records.insert(
                    id,
                    AssetRecord::new_runtime(T::TYPE.to_string(), TypeId::of::<T>(), installed),
                );
            }
        }

        Ok(dependency_update)
    }

    pub(crate) fn state_for_handle(&self, id: AssetId) -> AssetState {
        self.records
            .get(&id)
            .map(|record| record.state)
            .unwrap_or(AssetState::Unloaded)
    }

    pub(crate) fn error_for_handle(&self, id: AssetId) -> Option<AssetError> {
        self.records
            .get(&id)
            .and_then(|record| record.error.clone())
    }

    pub(crate) fn failure_phase(&self, id: AssetId) -> Option<AssetFailurePhase> {
        self.records
            .get(&id)
            .and_then(|record| record.failure_phase)
    }

    pub(crate) fn get_for_handle(
        &self,
        id: AssetId,
        expected_type: &'static str,
        expected_type_id: TypeId,
    ) -> Result<Arc<dyn Any + Send + Sync>, AssetError> {
        let record = self.records.get(&id).ok_or(AssetError::AssetNotInstalled {
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

    pub(crate) fn contains_record(&self, id: AssetId) -> bool {
        self.records.contains_key(&id)
    }

    pub(crate) fn record_state(&self, id: AssetId) -> Option<AssetState> {
        self.records.get(&id).map(|record| record.state)
    }

    pub(crate) fn record_dependency_links(&self, id: AssetId) -> Option<Vec<AssetId>> {
        self.records.get(&id).and_then(|record| {
            (!record.dependencies.is_empty()).then(|| record.dependencies.clone())
        })
    }

    pub(crate) fn record_references_dependency(&self, id: AssetId, dependency: AssetId) -> bool {
        self.records.get(&id).is_some_and(|record| {
            record.held_dependencies.contains(&dependency)
                || record.dependencies.contains(&dependency)
        })
    }

    pub(crate) fn record_ids_referencing_dependency(&self, dependency: AssetId) -> Vec<AssetId> {
        self.records
            .keys()
            .copied()
            .filter(|candidate| self.record_references_dependency(*candidate, dependency))
            .collect()
    }

    pub(crate) fn manifest_refresh_record_ids(&self) -> Vec<AssetId> {
        self.records
            .iter()
            .filter_map(|(id, record)| {
                (!record.runtime && record.raw_source_path.is_none()).then_some(*id)
            })
            .collect()
    }

    pub(crate) fn set_manifest_record_asset_type(
        &mut self,
        id: AssetId,
        asset_type: String,
    ) -> bool {
        let Some(record) = self.records.get_mut(&id) else {
            return false;
        };
        record.asset_type = asset_type;
        true
    }

    pub(crate) fn normal_drive_record_ids(&self) -> Vec<AssetId> {
        let mut ids = self.records.keys().copied().collect::<Vec<_>>();
        ids.sort_by(|left, right| {
            let left_record = self.records.get(left).expect("drive id should exist");
            let right_record = self.records.get(right).expect("drive id should exist");
            match (left_record.state, right_record.state) {
                (AssetState::Loading, AssetState::Loading) => right_record
                    .load_priority
                    .cmp(&left_record.load_priority)
                    .then_with(|| left.as_uuid().as_u128().cmp(&right.as_uuid().as_u128())),
                (AssetState::Loading, _) => std::cmp::Ordering::Less,
                (_, AssetState::Loading) => std::cmp::Ordering::Greater,
                _ => left.as_uuid().as_u128().cmp(&right.as_uuid().as_u128()),
            }
        });
        ids
    }

    pub(crate) fn normal_drive_iteration_limit(&self) -> usize {
        self.records.len().saturating_mul(4).max(8)
    }

    pub(crate) fn reload_scan_records(&self) -> Vec<AssetReloadScanRecord> {
        self.records
            .iter()
            .map(|(id, record)| match record.reload_tracking_fingerprint() {
                Some((loaded_entry_fingerprint, loaded_cooked_hash)) => {
                    AssetReloadScanRecord::Tracked {
                        id: *id,
                        loaded_entry_fingerprint,
                        loaded_cooked_hash,
                    }
                }
                None => AssetReloadScanRecord::Untracked { id: *id },
            })
            .collect()
    }

    pub(crate) fn diagnostic_records(&self, now: Instant) -> Vec<AssetRecordDiagnostic> {
        self.records
            .iter()
            .map(|(id, record)| AssetRecordDiagnostic::from_record(*id, record, now))
            .collect()
    }

    pub(crate) fn diagnostic_record(
        &self,
        id: AssetId,
        now: Instant,
    ) -> Option<AssetRecordDiagnostic> {
        self.records
            .get(&id)
            .map(|record| AssetRecordDiagnostic::from_record(id, record, now))
    }

    pub(crate) fn event_record_context(&self, id: AssetId) -> AssetEventRecordContext {
        self.records
            .get(&id)
            .map(AssetEventRecordContext::from_record)
            .unwrap_or_default()
    }

    pub(crate) fn load_generation(&self, id: AssetId) -> Option<u64> {
        self.records.get(&id).map(|record| record.load_generation)
    }

    pub(crate) fn load_generation_or_not_found(&self, id: AssetId) -> Result<u64, AssetError> {
        self.load_generation(id)
            .ok_or(AssetError::AssetNotFound { id })
    }

    pub(crate) fn activate_record_for_load(
        &mut self,
        id: AssetId,
        requested_type: Option<TypeId>,
        priority: i32,
    ) -> Result<AssetRecordLoadActivation, AssetError> {
        let record = self
            .records
            .get_mut(&id)
            .ok_or(AssetError::AssetNotFound { id })?;
        record.activate_for_load(requested_type, priority);
        Ok(AssetRecordLoadActivation {
            state: record.state,
            load_generation: record.load_generation,
        })
    }

    pub(crate) fn load_priority_or(&self, id: AssetId, default: i32) -> i32 {
        self.records
            .get(&id)
            .map(|record| record.load_priority)
            .unwrap_or(default)
    }

    pub(crate) fn accepts_load_completion(&self, id: AssetId, generation: u64) -> bool {
        self.records.get(&id).is_some_and(|record| {
            record.load_generation == generation && record.state == AssetState::Loading
        })
    }

    pub(crate) fn should_cancel_load(&self, id: AssetId, generation: u64) -> bool {
        !self.accepts_load_completion(id, generation)
    }

    pub(crate) fn prefers_background_load(&self, id: AssetId) -> bool {
        self.records.get(&id).is_some_and(|record| {
            record.raw_source_path.is_some() || record.asset_type == TextureAsset::TYPE
        })
    }

    pub(crate) fn is_runtime_record(&self, id: AssetId) -> bool {
        self.records.get(&id).is_some_and(|record| record.runtime)
    }

    pub(crate) fn raw_source_record(
        &self,
        id: AssetId,
    ) -> Result<AssetRawSourceRecord, AssetError> {
        let record = self
            .records
            .get(&id)
            .ok_or(AssetError::AssetNotFound { id })?;
        let path = record
            .raw_source_path
            .clone()
            .ok_or(AssetError::AssetNotFound { id })?;
        Ok(AssetRawSourceRecord {
            asset_type: record.asset_type.clone(),
            path,
        })
    }

    pub(crate) fn raw_source_path(&self, id: AssetId) -> Option<PathBuf> {
        self.records
            .get(&id)
            .and_then(|record| record.raw_source_path.clone())
    }

    pub(crate) fn loaded_payload_for_install(
        &self,
        id: AssetId,
    ) -> Result<Arc<dyn Any + Send + Sync>, AssetError> {
        self.records
            .get(&id)
            .and_then(|record| record.loaded.clone())
            .ok_or_else(|| AssetError::InvalidState {
                id,
                state: AssetState::Installing,
                message: "missing loaded payload".to_string(),
            })
    }

    #[cfg(test)]
    pub(crate) fn force_missing_loaded_payload_for_install_test(
        &mut self,
        id: AssetId,
    ) -> Result<(), AssetError> {
        let record = self
            .records
            .get_mut(&id)
            .ok_or(AssetError::AssetNotFound { id })?;
        record.state = AssetState::Installing;
        record.loaded = None;
        Ok(())
    }

    pub(crate) fn validate_installed_asset_type(
        &self,
        id: AssetId,
        expected_type: &'static str,
    ) -> Result<(), AssetError> {
        let record = self
            .records
            .get(&id)
            .ok_or(AssetError::AssetNotFound { id })?;
        if record.asset_type != expected_type {
            return Err(AssetError::AssetTypeMismatch {
                id,
                expected: expected_type,
                actual: record.asset_type.clone(),
            });
        }
        if record.state != AssetState::Installed {
            return Err(AssetError::AssetNotInstalled {
                id,
                state: record.state,
            });
        }
        Ok(())
    }

    pub(crate) fn take_record_install_task(
        &mut self,
        id: AssetId,
    ) -> Result<Option<Box<dyn AssetInstallTask<Output = Arc<dyn Any + Send + Sync>>>>, AssetError>
    {
        self.records
            .get_mut(&id)
            .ok_or_else(|| AssetError::InvalidState {
                id,
                state: AssetState::Unloaded,
                message: "missing install record".to_string(),
            })
            .map(AssetRecord::take_install_task)
    }

    pub(crate) fn finish_record_install(
        &mut self,
        id: AssetId,
        installed: Arc<dyn Any + Send + Sync>,
    ) -> Result<AssetRecordInstallCompletion, AssetError> {
        let record = self
            .records
            .get_mut(&id)
            .ok_or_else(|| AssetError::InvalidState {
                id,
                state: AssetState::Unloaded,
                message: "missing install record".to_string(),
            })?;
        let dependencies = record.dependencies.clone();
        let reloaded = record.reload_pending || record.reload_backup.is_some();
        record.finish_install(installed);
        Ok(AssetRecordInstallCompletion {
            dependencies,
            reloaded,
        })
    }

    pub(crate) fn defer_record_install(
        &mut self,
        id: AssetId,
        task: Box<dyn AssetInstallTask<Output = Arc<dyn Any + Send + Sync>>>,
    ) -> Result<(), AssetError> {
        let record = self
            .records
            .get_mut(&id)
            .ok_or_else(|| AssetError::InvalidState {
                id,
                state: AssetState::Unloaded,
                message: "missing install record".to_string(),
            })?;
        record.defer_install(task);
        Ok(())
    }

    pub(crate) fn installed_payload_for_uninstall(
        &self,
        id: AssetId,
    ) -> Option<Arc<dyn Any + Send + Sync>> {
        self.records
            .get(&id)
            .and_then(|record| record.installed.clone())
    }

    pub(crate) fn is_referenced(&self, id: AssetId) -> bool {
        self.records
            .get(&id)
            .is_some_and(|record| record.strong_ref_count > 0 || record.dependency_ref_count > 0)
    }

    pub(crate) fn retain_direct_with<F>(
        &mut self,
        id: AssetId,
        requested_type: Option<TypeId>,
        make_record: F,
    ) where
        F: FnOnce() -> AssetRecord,
    {
        let record = self.records.entry(id).or_insert_with(make_record);
        record.strong_ref_count += 1;

        if record.requested_type.is_none() {
            record.requested_type = requested_type;
        }
    }

    pub(crate) fn retain_existing_direct(
        &mut self,
        id: AssetId,
        requested_type: Option<TypeId>,
    ) -> bool {
        let Some(record) = self.records.get_mut(&id) else {
            return false;
        };
        record.strong_ref_count += 1;

        if record.requested_type.is_none() {
            record.requested_type = requested_type;
        }

        true
    }

    pub(crate) fn release_direct_reference(&mut self, id: AssetId) {
        if let Some(record) = self.records.get_mut(&id) {
            record.strong_ref_count = record.strong_ref_count.saturating_sub(1);
        }
    }

    pub(crate) fn release_direct_reference_and_schedule_unused(
        &mut self,
        id: AssetId,
    ) -> AssetReleaseOutcome {
        self.release_direct_reference(id);
        self.schedule_release_if_unused(id)
    }

    pub(crate) fn retain_dependency_with<F>(
        &mut self,
        id: AssetId,
        make_record: F,
    ) -> &mut AssetRecord
    where
        F: FnOnce() -> AssetRecord,
    {
        let record = self.records.entry(id).or_insert_with(make_record);
        record.dependency_ref_count += 1;
        record
    }

    pub(crate) fn release_dependency_reference(&mut self, id: AssetId) {
        if let Some(record) = self.records.get_mut(&id) {
            record.dependency_ref_count = record.dependency_ref_count.saturating_sub(1);
        }
    }

    pub(crate) fn release_dependency_reference_and_schedule_unused(
        &mut self,
        id: AssetId,
    ) -> AssetReleaseOutcome {
        self.release_dependency_reference(id);
        self.schedule_release_if_unused(id)
    }

    pub(crate) fn schedule_release_if_unused(&mut self, id: AssetId) -> AssetReleaseOutcome {
        let mut outcome = AssetReleaseOutcome::default();
        self.schedule_release_if_unused_inner(id, &mut outcome);
        outcome
    }

    pub(crate) fn replace_held_dependencies(
        &mut self,
        id: AssetId,
        new_dependencies: Vec<AssetId>,
    ) -> AssetDependencyLeaseUpdate {
        let new_leases = AssetDependencyLeases::new(new_dependencies);
        let old_dependencies = match self.records.get_mut(&id) {
            Some(record) => {
                std::mem::replace(&mut record.held_dependencies, new_leases.clone()).into_vec()
            }
            None => return AssetDependencyLeaseUpdate::default(),
        };

        let mut update = AssetDependencyLeaseUpdate::default();
        for dependency in &old_dependencies {
            if new_leases.contains(dependency) {
                continue;
            }
            let release = self.release_dependency_reference_and_schedule_unused(*dependency);
            update
                .release
                .immediate_unloaded
                .extend(release.immediate_unloaded);
        }

        for dependency in new_leases.ids() {
            if old_dependencies.contains(dependency) {
                continue;
            }
            update.new_dependency_leases.push(*dependency);
        }

        update
    }

    pub(crate) fn apply_dependency_lease_update<F>(
        &mut self,
        update: AssetDependencyLeaseUpdate,
        priority: i32,
        mut asset_type_for: F,
    ) -> AssetReleaseOutcome
    where
        F: FnMut(AssetId) -> Option<String>,
    {
        for dependency in update.new_dependency_leases {
            let Some(asset_type) = asset_type_for(dependency) else {
                continue;
            };

            let record = self
                .retain_dependency_with(dependency, || AssetRecord::new(dependency, asset_type));
            record.activate_for_load(None, priority);
        }

        update.release
    }

    pub(crate) fn extend_held_dependencies(
        &mut self,
        id: AssetId,
        new_dependencies: &[AssetId],
    ) -> AssetDependencyLeaseUpdate {
        let mut held_dependencies = self
            .records
            .get(&id)
            .map(|record| record.held_dependencies.clone())
            .unwrap_or_default();
        held_dependencies.extend_missing(new_dependencies);
        self.replace_held_dependencies(id, held_dependencies.into_vec())
    }

    pub(crate) fn finish_loaded_record(
        &mut self,
        id: AssetId,
        loaded: Arc<dyn Any + Send + Sync>,
        dependencies: Vec<AssetId>,
        entry_fingerprint: String,
        cooked_hash: Option<String>,
    ) -> Option<AssetDependencyLeaseUpdate> {
        let extend_existing_leases = self.records.get(&id)?.reload_backup.is_some();
        let update = if extend_existing_leases {
            self.extend_held_dependencies(id, &dependencies)
        } else {
            self.replace_held_dependencies(id, dependencies.clone())
        };

        let record = self.records.get_mut(&id).expect("record should exist");
        record.finish_load(loaded, dependencies, entry_fingerprint, cooked_hash);
        Some(update)
    }

    pub(crate) fn fail_record(
        &mut self,
        id: AssetId,
        error: AssetError,
        phase: AssetFailurePhase,
    ) -> Option<AssetFailureOutcome> {
        let backup = self.records.get_mut(&id)?.take_reload_backup();
        if let Some(backup) = backup {
            let dependency_update =
                self.replace_held_dependencies(id, backup.held_dependencies.clone().into_vec());
            let record = self.records.get_mut(&id).expect("record should exist");
            let event_state = record.restore_reload_backup(backup, error, phase);
            return Some(AssetFailureOutcome {
                dependency_update,
                event_state,
            });
        }

        let dependency_update = self.replace_held_dependencies(id, Vec::new());
        let record = self.records.get_mut(&id).expect("record should exist");
        record.fail(error, phase);
        Some(AssetFailureOutcome {
            dependency_update,
            event_state: AssetState::Failed,
        })
    }

    fn schedule_release_if_unused_inner(&mut self, id: AssetId, outcome: &mut AssetReleaseOutcome) {
        let old_dependencies = {
            let Some(record) = self.records.get_mut(&id) else {
                return;
            };
            if record.reload_pending
                || record.strong_ref_count > 0
                || record.dependency_ref_count > 0
            {
                return;
            }
            std::mem::take(&mut record.held_dependencies).into_vec()
        };

        for dependency in old_dependencies {
            self.release_dependency_reference(dependency);
            self.schedule_release_if_unused_inner(dependency, outcome);
        }

        let Some(record) = self.records.get_mut(&id) else {
            return;
        };
        if record.begin_release() {
            outcome.immediate_unloaded.push(id);
        }
    }

    pub(crate) fn evaluate_dependency_records<F>(
        &mut self,
        id: AssetId,
        priority: i32,
        mut asset_type_for: F,
    ) -> AssetDependencyEvaluation
    where
        F: FnMut(AssetId) -> Option<String>,
    {
        let dependencies = self
            .records
            .get(&id)
            .map(|record| record.dependencies.clone())
            .unwrap_or_default();

        if dependencies.is_empty() {
            return AssetDependencyEvaluation::Ready;
        }

        let mut waiting = false;
        for dependency in dependencies {
            let Some(asset_type) = asset_type_for(dependency) else {
                return AssetDependencyEvaluation::MissingManifest { dependency };
            };

            if !self.records.contains_key(&dependency) {
                self.records
                    .insert(dependency, AssetRecord::new(dependency, asset_type));
                let record = self
                    .records
                    .get_mut(&dependency)
                    .expect("dependency record should exist");
                record.activate_for_load(None, priority);
                waiting = true;
                continue;
            }

            match self.records.get(&dependency).map(|record| record.state) {
                Some(AssetState::Installed) => {}
                Some(AssetState::Failed) => {
                    return AssetDependencyEvaluation::Failed { dependency };
                }
                Some(_) | None => waiting = true,
            }
        }

        if waiting {
            AssetDependencyEvaluation::Waiting
        } else {
            AssetDependencyEvaluation::Ready
        }
    }

    pub(crate) fn set_record_state(&mut self, id: AssetId, state: AssetState) -> bool {
        let Some(record) = self.records.get_mut(&id) else {
            return false;
        };
        record.set_state(state)
    }

    pub(crate) fn advance_record_uninstall(&mut self, id: AssetId) -> bool {
        let Some(record) = self.records.get_mut(&id) else {
            return false;
        };
        record.advance_uninstall();
        true
    }

    pub(crate) fn finish_record_unload(&mut self, id: AssetId) -> bool {
        let Some(record) = self.records.get_mut(&id) else {
            return false;
        };
        record.finish_unload();
        true
    }

    pub(crate) fn queue_record_reload(&mut self, id: AssetId) -> bool {
        let Some(record) = self.records.get_mut(&id) else {
            return false;
        };
        record.queue_reload();
        true
    }

    pub(crate) fn prepare_record_reload(
        &mut self,
        id: AssetId,
        has_manifest_entry: bool,
    ) -> AssetReloadPrepareOutcome {
        let Some(record) = self.records.get_mut(&id) else {
            return AssetReloadPrepareOutcome::MissingRecord;
        };

        if record.prepare_reload(has_manifest_entry) {
            AssetReloadPrepareOutcome::Queued
        } else {
            AssetReloadPrepareOutcome::MissingManifest
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AssetDependencyEvaluation {
    Ready,
    Waiting,
    MissingManifest { dependency: AssetId },
    Failed { dependency: AssetId },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AssetReloadPrepareOutcome {
    Queued,
    MissingManifest,
    MissingRecord,
}

#[derive(Default)]
pub(crate) struct AssetReleaseOutcome {
    pub(crate) immediate_unloaded: Vec<AssetId>,
}

#[derive(Default)]
pub(crate) struct AssetDependencyLeaseUpdate {
    pub(crate) release: AssetReleaseOutcome,
    pub(crate) new_dependency_leases: Vec<AssetId>,
}

pub(crate) struct AssetFailureOutcome {
    pub(crate) dependency_update: AssetDependencyLeaseUpdate,
    pub(crate) event_state: AssetState,
}

pub(crate) struct AssetRecord {
    pub(crate) asset_type: String,
    pub(crate) state: AssetState,
    pub(crate) requested_type: Option<TypeId>,
    pub(crate) strong_ref_count: usize,
    pub(crate) dependency_ref_count: usize,
    pub(crate) dependencies: Vec<AssetId>,
    pub(crate) held_dependencies: AssetDependencyLeases,
    pub(crate) loaded: Option<Arc<dyn Any + Send + Sync>>,
    pub(crate) installed: Option<Arc<dyn Any + Send + Sync>>,
    pub(crate) install_task: Option<Box<dyn AssetInstallTask<Output = Arc<dyn Any + Send + Sync>>>>,
    pub(crate) reload_backup: Option<AssetReloadBackup>,
    pub(crate) error: Option<AssetError>,
    pub(crate) failure_phase: Option<AssetFailurePhase>,
    pub(crate) loaded_entry_fingerprint: Option<String>,
    pub(crate) loaded_cooked_hash: Option<String>,
    pub(crate) reload_pending: bool,
    pub(crate) load_generation: u64,
    pub(crate) load_priority: i32,
    pub(crate) runtime: bool,
    pub(crate) raw_source_path: Option<PathBuf>,
    state_entered_at: Instant,
}

impl AssetRecord {
    pub(crate) fn new(_id: AssetId, asset_type: String) -> Self {
        let now = Instant::now();
        Self {
            asset_type,
            state: AssetState::Unloaded,
            requested_type: None,
            strong_ref_count: 0,
            dependency_ref_count: 0,
            dependencies: Vec::new(),
            held_dependencies: AssetDependencyLeases::default(),
            loaded: None,
            installed: None,
            install_task: None,
            reload_backup: None,
            error: None,
            failure_phase: None,
            loaded_entry_fingerprint: None,
            loaded_cooked_hash: None,
            reload_pending: false,
            load_generation: 0,
            load_priority: 0,
            runtime: false,
            raw_source_path: None,
            state_entered_at: now,
        }
    }

    pub(crate) fn new_raw_texture(path: PathBuf) -> Self {
        let now = Instant::now();
        Self {
            asset_type: TextureAsset::TYPE.to_string(),
            state: AssetState::Unloaded,
            requested_type: Some(TypeId::of::<TextureAsset>()),
            strong_ref_count: 0,
            dependency_ref_count: 0,
            dependencies: Vec::new(),
            held_dependencies: AssetDependencyLeases::default(),
            loaded: None,
            installed: None,
            install_task: None,
            reload_backup: None,
            error: None,
            failure_phase: None,
            loaded_entry_fingerprint: None,
            loaded_cooked_hash: None,
            reload_pending: false,
            load_generation: 0,
            load_priority: 0,
            runtime: false,
            raw_source_path: Some(path),
            state_entered_at: now,
        }
    }

    pub(crate) fn new_raw_font(path: PathBuf) -> Self {
        let now = Instant::now();
        Self {
            asset_type: FontAsset::TYPE.to_string(),
            state: AssetState::Unloaded,
            requested_type: Some(TypeId::of::<FontAsset>()),
            strong_ref_count: 0,
            dependency_ref_count: 0,
            dependencies: Vec::new(),
            held_dependencies: AssetDependencyLeases::default(),
            loaded: None,
            installed: None,
            install_task: None,
            reload_backup: None,
            error: None,
            failure_phase: None,
            loaded_entry_fingerprint: None,
            loaded_cooked_hash: None,
            reload_pending: false,
            load_generation: 0,
            load_priority: 0,
            runtime: false,
            raw_source_path: Some(path),
            state_entered_at: now,
        }
    }

    pub(crate) fn new_runtime(
        asset_type: String,
        requested_type: TypeId,
        installed: Arc<dyn Any + Send + Sync>,
    ) -> Self {
        let now = Instant::now();
        Self {
            asset_type,
            state: AssetState::Installed,
            requested_type: Some(requested_type),
            strong_ref_count: 0,
            dependency_ref_count: 0,
            dependencies: Vec::new(),
            held_dependencies: AssetDependencyLeases::default(),
            loaded: Some(installed.clone()),
            installed: Some(installed),
            install_task: None,
            reload_backup: None,
            error: None,
            failure_phase: None,
            loaded_entry_fingerprint: None,
            loaded_cooked_hash: None,
            reload_pending: false,
            load_generation: 0,
            load_priority: 0,
            runtime: true,
            raw_source_path: None,
            state_entered_at: now,
        }
    }

    pub(crate) fn set_state(&mut self, state: AssetState) -> bool {
        if self.state == state {
            return false;
        }
        self.state = state;
        self.state_entered_at = Instant::now();
        true
    }

    pub(crate) fn state_age(&self, now: Instant) -> Duration {
        now.saturating_duration_since(self.state_entered_at)
    }

    pub(crate) fn clear_error(&mut self) {
        self.error = None;
        self.failure_phase = None;
    }

    pub(crate) fn set_error(&mut self, error: AssetError, phase: AssetFailurePhase) {
        self.error = Some(error);
        self.failure_phase = Some(phase);
    }

    pub(crate) fn activate_for_load(&mut self, requested_type: Option<TypeId>, priority: i32) {
        if self.requested_type.is_none() {
            self.requested_type = requested_type;
        }
        self.load_priority = priority;

        match self.state {
            AssetState::Unloaded | AssetState::Failed => {
                self.load_generation = self.load_generation.wrapping_add(1);
                self.set_state(AssetState::Loading);
                self.clear_error();
            }
            AssetState::Uninstalling => {
                self.set_state(AssetState::Installed);
                self.clear_error();
            }
            AssetState::Unloading => {
                self.load_generation = self.load_generation.wrapping_add(1);
                self.clear_error();
                let state = if self.loaded.is_some() {
                    AssetState::Loaded
                } else {
                    AssetState::Loading
                };
                self.set_state(state);
            }
            AssetState::Loading
            | AssetState::Loaded
            | AssetState::WaitingDependencies
            | AssetState::Installing
            | AssetState::Installed => {}
        }
    }

    pub(crate) fn begin_release(&mut self) -> bool {
        match self.state {
            AssetState::Installed => {
                self.set_state(AssetState::Uninstalling);
            }
            AssetState::Loaded
            | AssetState::WaitingDependencies
            | AssetState::Installing
            | AssetState::Failed => {
                self.set_state(AssetState::Unloading);
            }
            AssetState::Unloaded | AssetState::Loading => {
                if matches!(self.state, AssetState::Loading) {
                    self.load_generation = self.load_generation.wrapping_add(1);
                }
                self.clear_runtime_payload();
                self.set_state(AssetState::Unloaded);
                return true;
            }
            AssetState::Uninstalling | AssetState::Unloading => {}
        }
        false
    }

    pub(crate) fn advance_uninstall(&mut self) {
        self.installed = None;
        self.install_task = None;
        self.reload_backup = None;
        self.set_state(AssetState::Unloading);
    }

    pub(crate) fn finish_unload(&mut self) {
        self.clear_runtime_payload();
        self.dependencies.clear();
        self.set_state(AssetState::Unloaded);
    }

    pub(crate) fn take_install_task(
        &mut self,
    ) -> Option<Box<dyn AssetInstallTask<Output = Arc<dyn Any + Send + Sync>>>> {
        self.install_task.take()
    }

    pub(crate) fn install_progress(&self) -> Option<AssetRequestProgress> {
        self.install_task.as_ref().and_then(|task| task.progress())
    }

    fn reload_tracking_fingerprint(&self) -> Option<(String, String)> {
        if self.runtime || self.raw_source_path.is_some() {
            return None;
        }
        if self.strong_ref_count == 0
            && self.dependency_ref_count == 0
            && self.installed.is_none()
            && self.loaded.is_none()
            && !self.reload_pending
        {
            return None;
        }
        Some((
            self.loaded_entry_fingerprint.clone()?,
            self.loaded_cooked_hash.clone()?,
        ))
    }

    pub(crate) fn finish_install(&mut self, installed: Arc<dyn Any + Send + Sync>) {
        self.installed = Some(installed);
        self.install_task = None;
        self.reload_backup = None;
        self.clear_error();
        self.reload_pending = false;
        self.set_state(AssetState::Installed);
    }

    pub(crate) fn defer_install(
        &mut self,
        task: Box<dyn AssetInstallTask<Output = Arc<dyn Any + Send + Sync>>>,
    ) {
        self.install_task = Some(task);
        self.set_state(AssetState::Installing);
    }

    pub(crate) fn finish_load(
        &mut self,
        loaded: Arc<dyn Any + Send + Sync>,
        dependencies: Vec<AssetId>,
        entry_fingerprint: String,
        cooked_hash: Option<String>,
    ) {
        self.loaded = Some(loaded);
        self.dependencies = dependencies;
        self.clear_error();
        self.loaded_entry_fingerprint = Some(entry_fingerprint);
        self.loaded_cooked_hash = cooked_hash;
        self.set_state(AssetState::Loaded);
    }

    pub(crate) fn take_reload_backup(&mut self) -> Option<AssetReloadBackup> {
        self.reload_backup.take()
    }

    pub(crate) fn restore_reload_backup(
        &mut self,
        backup: AssetReloadBackup,
        error: AssetError,
        phase: AssetFailurePhase,
    ) -> AssetState {
        let restored_state = if backup.installed.is_some() {
            AssetState::Installed
        } else if backup.loaded.is_some() {
            AssetState::Loaded
        } else {
            AssetState::Failed
        };

        self.loaded = backup.loaded;
        self.installed = backup.installed;
        self.install_task = None;
        self.dependencies = backup.dependencies;
        self.set_error(error, phase);
        self.loaded_entry_fingerprint = backup.loaded_entry_fingerprint;
        self.loaded_cooked_hash = backup.loaded_cooked_hash;
        self.reload_pending = false;
        self.set_state(restored_state);
        restored_state
    }

    pub(crate) fn fail(&mut self, error: AssetError, phase: AssetFailurePhase) {
        self.loaded = None;
        self.installed = None;
        self.install_task = None;
        self.reload_backup = None;
        self.set_error(error, phase);
        self.loaded_entry_fingerprint = None;
        self.loaded_cooked_hash = None;
        self.reload_pending = false;
        self.set_state(AssetState::Failed);
    }

    pub(crate) fn queue_reload(&mut self) {
        self.reload_pending = true;
    }

    pub(crate) fn prepare_reload(&mut self, has_manifest_entry: bool) -> bool {
        if self.reload_backup.is_none() {
            self.reload_backup = Some(AssetReloadBackup::from_record(self));
        }
        self.load_generation = self.load_generation.wrapping_add(1);
        self.loaded = None;
        self.install_task = None;
        self.dependencies.clear();
        self.clear_error();
        self.loaded_entry_fingerprint = None;
        self.loaded_cooked_hash = None;

        if has_manifest_entry {
            self.set_state(AssetState::Loading);
            true
        } else {
            false
        }
    }

    pub(crate) fn replace_runtime(
        &mut self,
        asset_type: String,
        requested_type: TypeId,
        installed: Arc<dyn Any + Send + Sync>,
    ) {
        self.asset_type = asset_type;
        self.set_state(AssetState::Installed);
        self.requested_type = Some(requested_type);
        self.dependencies.clear();
        self.held_dependencies.clear();
        self.loaded = Some(installed.clone());
        self.installed = Some(installed);
        self.install_task = None;
        self.reload_backup = None;
        self.clear_error();
        self.loaded_entry_fingerprint = None;
        self.loaded_cooked_hash = None;
        self.reload_pending = false;
        self.runtime = true;
        self.raw_source_path = None;
    }

    fn clear_runtime_payload(&mut self) {
        self.loaded = None;
        self.installed = None;
        self.install_task = None;
        self.reload_backup = None;
        self.clear_error();
        self.loaded_entry_fingerprint = None;
        self.loaded_cooked_hash = None;
        self.reload_pending = false;
    }
}

pub(crate) struct AssetReloadBackup {
    pub(crate) loaded: Option<Arc<dyn Any + Send + Sync>>,
    pub(crate) installed: Option<Arc<dyn Any + Send + Sync>>,
    pub(crate) dependencies: Vec<AssetId>,
    pub(crate) held_dependencies: AssetDependencyLeases,
    pub(crate) loaded_entry_fingerprint: Option<String>,
    pub(crate) loaded_cooked_hash: Option<String>,
}

impl AssetReloadBackup {
    pub(crate) fn from_record(record: &AssetRecord) -> Self {
        Self {
            loaded: record.loaded.clone(),
            installed: record.installed.clone(),
            dependencies: record.dependencies.clone(),
            held_dependencies: record.held_dependencies.clone(),
            loaded_entry_fingerprint: record.loaded_entry_fingerprint.clone(),
            loaded_cooked_hash: record.loaded_cooked_hash.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset::install::{AssetInstallBudget, AssetInstallContext, AssetInstallPoll};

    #[test]
    fn reference_counts_are_owned_by_store_and_saturate_on_release() {
        let id = AssetId::new();
        let mut store = AssetStore::default();

        store.retain_direct_with(id, None, || AssetRecord::new(id, "dummy".to_string()));
        assert!(store.is_referenced(id));
        assert_eq!(store.records[&id].strong_ref_count, 1);

        assert!(store.retain_existing_direct(id, None));
        assert_eq!(store.records[&id].strong_ref_count, 2);
        assert!(!store.retain_existing_direct(AssetId::new(), None));

        store.release_direct_reference(id);
        store.release_direct_reference(id);
        store.release_direct_reference(id);
        assert!(!store.is_referenced(id));
        assert_eq!(store.records[&id].strong_ref_count, 0);

        store.retain_dependency_with(id, || AssetRecord::new(id, "dummy".to_string()));
        assert!(store.is_referenced(id));
        assert_eq!(store.records[&id].dependency_ref_count, 1);

        store.release_dependency_reference(id);
        store.release_dependency_reference(id);
        assert!(!store.is_referenced(id));
        assert_eq!(store.records[&id].dependency_ref_count, 0);
    }

    #[test]
    fn raw_source_record_indexes_reuse_existing_texture_and_font_records() {
        let mut store = AssetStore::default();

        let texture = store.ensure_raw_texture_record(
            "textures/hero.png".to_string(),
            PathBuf::from("textures/hero.png"),
        );
        let same_texture = store.ensure_raw_texture_record(
            "textures/hero.png".to_string(),
            PathBuf::from("textures/other.png"),
        );
        let font =
            store.ensure_raw_font_record("fonts/ui.ttf".to_string(), PathBuf::from("fonts/ui.ttf"));
        let same_font = store
            .ensure_raw_font_record("fonts/ui.ttf".to_string(), PathBuf::from("fonts/other.ttf"));

        assert_eq!(texture, same_texture);
        assert_eq!(font, same_font);
        assert_eq!(
            store.raw_textures[&"textures/hero.png".to_string()],
            texture
        );
        assert_eq!(store.raw_fonts[&"fonts/ui.ttf".to_string()], font);
        assert_eq!(store.records[&texture].asset_type, TextureAsset::TYPE);
        assert_eq!(store.records[&font].asset_type, FontAsset::TYPE);
        assert_eq!(store.records.len(), 2);

        let retained_texture = store.retain_raw_texture_record(
            "textures/hero.png".to_string(),
            PathBuf::from("textures/hero.png"),
        );
        let retained_font =
            store.retain_raw_font_record("fonts/ui.ttf".to_string(), PathBuf::from("fonts/ui.ttf"));
        assert_eq!(retained_texture, texture);
        assert_eq!(retained_font, font);
        assert_eq!(store.records[&texture].strong_ref_count, 1);
        assert_eq!(store.records[&font].strong_ref_count, 1);
    }

    #[derive(Debug)]
    struct RuntimeAsset(&'static str);

    impl Asset for RuntimeAsset {
        const TYPE: &'static str = "runtime";
    }

    struct OtherRuntimeAsset;

    impl Asset for OtherRuntimeAsset {
        const TYPE: &'static str = "other_runtime";
    }

    struct PendingInstallTask;

    impl AssetInstallTask for PendingInstallTask {
        type Output = Arc<dyn Any + Send + Sync>;

        fn poll_install(
            &mut self,
            _ctx: AssetInstallContext<'_>,
            _budget: AssetInstallBudget,
        ) -> Result<AssetInstallPoll<Self::Output>, AssetError> {
            Ok(AssetInstallPoll::Pending)
        }
    }

    #[test]
    fn runtime_asset_replacement_is_owned_by_store() {
        let id = AssetId::new();
        let dependency = AssetId::new();
        let mut store = AssetStore::default();

        store.insert_runtime_asset(id, RuntimeAsset("old"));
        store.records.insert(
            dependency,
            AssetRecord::new(dependency, "dependency".to_string()),
        );
        store
            .records
            .get_mut(&dependency)
            .unwrap()
            .dependency_ref_count = 1;
        store.records.get_mut(&id).unwrap().held_dependencies =
            AssetDependencyLeases::new(vec![dependency]);

        let update = store
            .replace_runtime_asset(id, RuntimeAsset("new"))
            .expect("runtime replacement should succeed");
        let record = store.records.get(&id).expect("runtime record");
        let installed = record
            .installed
            .as_ref()
            .and_then(|asset| asset.downcast_ref::<RuntimeAsset>())
            .expect("runtime asset payload");

        assert_eq!(installed.0, "new");
        assert!(record.runtime);
        assert!(record.held_dependencies.ids().is_empty());
        assert_eq!(store.records[&dependency].dependency_ref_count, 0);
        assert_eq!(update.release.immediate_unloaded, vec![dependency]);
    }

    #[test]
    fn runtime_asset_replacement_rejects_requested_type_mismatch() {
        let id = AssetId::new();
        let mut store = AssetStore::default();
        store.records.insert(
            id,
            AssetRecord::new_runtime(
                OtherRuntimeAsset::TYPE.to_string(),
                TypeId::of::<OtherRuntimeAsset>(),
                Arc::new(OtherRuntimeAsset),
            ),
        );

        let error = match store.replace_runtime_asset(id, RuntimeAsset("wrong")) {
            Ok(_) => panic!("type mismatch should be rejected"),
            Err(error) => error,
        };

        assert_eq!(
            error,
            AssetError::AssetTypeMismatch {
                id,
                expected: RuntimeAsset::TYPE,
                actual: OtherRuntimeAsset::TYPE.to_string()
            }
        );
    }

    #[test]
    fn handle_read_helpers_are_owned_by_store() {
        let id = AssetId::new();
        let missing = AssetId::new();
        let mut store = AssetStore::default();
        store.insert_runtime_asset(id, RuntimeAsset("ready"));

        assert_eq!(store.state_for_handle(id), AssetState::Installed);
        assert_eq!(store.state_for_handle(missing), AssetState::Unloaded);
        assert_eq!(store.error_for_handle(id), None);
        assert_eq!(store.failure_phase(id), None);
        assert!(store.is_runtime_record(id));
        assert!(!store.is_runtime_record(missing));

        let installed = store
            .get_for_handle(id, RuntimeAsset::TYPE, TypeId::of::<RuntimeAsset>())
            .expect("runtime asset should be installed");
        let installed = installed
            .downcast_ref::<RuntimeAsset>()
            .expect("runtime asset type");
        assert_eq!(installed.0, "ready");

        assert_eq!(
            store
                .get_for_handle(missing, RuntimeAsset::TYPE, TypeId::of::<RuntimeAsset>())
                .expect_err("missing handle payload should fail"),
            AssetError::AssetNotInstalled {
                id: missing,
                state: AssetState::Unloaded,
            }
        );

        assert_eq!(
            store
                .get_for_handle(
                    id,
                    OtherRuntimeAsset::TYPE,
                    TypeId::of::<OtherRuntimeAsset>()
                )
                .expect_err("requested type mismatch should fail"),
            AssetError::AssetTypeMismatch {
                id,
                expected: OtherRuntimeAsset::TYPE,
                actual: RuntimeAsset::TYPE.to_string(),
            }
        );
    }

    #[test]
    fn store_read_helpers_cover_existence_generation_and_background_hint() {
        let raw_texture = AssetId::new();
        let dummy = AssetId::new();
        let missing = AssetId::new();
        let mut store = AssetStore::default();
        store
            .records
            .insert(dummy, AssetRecord::new(dummy, "dummy".to_string()));
        store.records.get_mut(&dummy).unwrap().load_generation = 7;
        store.records.get_mut(&dummy).unwrap().load_priority = 3;
        store
            .records
            .insert(raw_texture, AssetRecord::new_raw_texture("hero.png".into()));

        assert!(store.contains_record(dummy));
        assert!(!store.contains_record(missing));
        assert_eq!(store.record_state(dummy), Some(AssetState::Unloaded));
        assert_eq!(store.record_state(missing), None);
        assert_eq!(store.load_generation(dummy), Some(7));
        assert_eq!(store.load_generation_or_not_found(dummy), Ok(7));
        assert_eq!(
            store.load_generation_or_not_found(missing),
            Err(AssetError::AssetNotFound { id: missing })
        );
        assert_eq!(store.load_generation(missing), None);
        assert_eq!(store.load_priority_or(dummy, 99), 3);
        assert_eq!(store.load_priority_or(missing, 99), 99);
        assert_eq!(store.normal_drive_iteration_limit(), 8);
        assert!(!store.accepts_load_completion(dummy, 7));
        assert!(store.should_cancel_load(dummy, 7));
        store.records.get_mut(&dummy).unwrap().state = AssetState::Loading;
        assert!(store.accepts_load_completion(dummy, 7));
        assert!(!store.should_cancel_load(dummy, 7));
        assert!(!store.accepts_load_completion(dummy, 6));
        assert!(store.should_cancel_load(dummy, 6));
        assert!(!store.prefers_background_load(dummy));
        assert!(store.prefers_background_load(raw_texture));
        assert!(!store.prefers_background_load(missing));

        assert_eq!(
            store
                .loaded_payload_for_install(dummy)
                .expect_err("missing loaded payload should fail"),
            AssetError::InvalidState {
                id: dummy,
                state: AssetState::Installing,
                message: "missing loaded payload".to_string(),
            }
        );
        store.records.get_mut(&dummy).unwrap().loaded = Some(Arc::new(123usize));
        let payload = store
            .loaded_payload_for_install(dummy)
            .expect("loaded payload should be available");
        assert_eq!(payload.downcast_ref::<usize>(), Some(&123));
    }

    #[test]
    fn store_raw_source_helpers_expose_only_raw_record_data() {
        let texture = AssetId::new();
        let font = AssetId::new();
        let manifest = AssetId::new();
        let mut store = AssetStore::default();

        store.records.insert(
            texture,
            AssetRecord::new_raw_texture(PathBuf::from("textures/hero.png")),
        );
        store.records.insert(
            font,
            AssetRecord::new_raw_font(PathBuf::from("fonts/ui.ttf")),
        );
        store
            .records
            .insert(manifest, AssetRecord::new(manifest, "dummy".to_string()));

        assert_eq!(
            store.raw_source_record(texture),
            Ok(AssetRawSourceRecord {
                asset_type: TextureAsset::TYPE.to_string(),
                path: PathBuf::from("textures/hero.png"),
            })
        );
        assert_eq!(
            store.raw_source_path(font),
            Some(PathBuf::from("fonts/ui.ttf"))
        );
        assert!(matches!(
            store.raw_source_record(manifest),
            Err(AssetError::AssetNotFound { .. })
        ));
        assert_eq!(store.raw_source_path(manifest), None);
        assert!(matches!(
            store.raw_source_record(AssetId::new()),
            Err(AssetError::AssetNotFound { .. })
        ));
    }

    #[test]
    fn store_normal_drive_ids_prioritize_loading_records() {
        let low = AssetId::new();
        let high = AssetId::new();
        let installed = AssetId::new();
        let mut store = AssetStore::default();

        let mut low_record = AssetRecord::new(low, "dummy".to_string());
        low_record.state = AssetState::Loading;
        low_record.load_priority = 1;
        let mut high_record = AssetRecord::new(high, "dummy".to_string());
        high_record.state = AssetState::Loading;
        high_record.load_priority = 10;
        let mut installed_record = AssetRecord::new(installed, "dummy".to_string());
        installed_record.state = AssetState::Installed;

        store.records.insert(low, low_record);
        store.records.insert(high, high_record);
        store.records.insert(installed, installed_record);

        let ids = store.normal_drive_record_ids();

        assert_eq!(ids[0], high);
        assert_eq!(ids[1], low);
        assert!(ids.contains(&installed));
        assert_eq!(store.normal_drive_iteration_limit(), 12);
    }

    #[test]
    fn store_reload_scan_records_expose_only_reload_tracked_fingerprints() {
        let tracked = AssetId::new();
        let unreferenced = AssetId::new();
        let raw = AssetId::new();
        let runtime = AssetId::new();
        let mut store = AssetStore::default();

        let mut tracked_record = AssetRecord::new(tracked, "dummy".to_string());
        tracked_record.installed = Some(Arc::new(1u32));
        tracked_record.loaded_entry_fingerprint = Some("tracked-entry".to_string());
        tracked_record.loaded_cooked_hash = Some("tracked-hash".to_string());
        store.records.insert(tracked, tracked_record);

        let mut unreferenced_record = AssetRecord::new(unreferenced, "dummy".to_string());
        unreferenced_record.loaded_entry_fingerprint = Some("unreferenced-entry".to_string());
        unreferenced_record.loaded_cooked_hash = Some("unreferenced-hash".to_string());
        store.records.insert(unreferenced, unreferenced_record);

        let mut raw_record = AssetRecord::new_raw_texture("hero.png".into());
        raw_record.strong_ref_count = 1;
        raw_record.loaded_entry_fingerprint = Some("raw-entry".to_string());
        raw_record.loaded_cooked_hash = Some("raw-hash".to_string());
        store.records.insert(raw, raw_record);

        store.insert_runtime_asset(runtime, RuntimeAsset("runtime"));

        let records = store.reload_scan_records();

        assert!(records.iter().any(|record| {
            matches!(
                record,
                AssetReloadScanRecord::Tracked {
                    id,
                    loaded_entry_fingerprint,
                    loaded_cooked_hash,
                } if *id == tracked
                    && loaded_entry_fingerprint == "tracked-entry"
                    && loaded_cooked_hash == "tracked-hash"
            )
        }));
        for id in [unreferenced, raw, runtime] {
            assert!(records
                .iter()
                .any(|record| matches!(record, AssetReloadScanRecord::Untracked { id: found } if *found == id)));
        }
        assert_eq!(records.len(), 4);
    }

    #[test]
    fn store_diagnostic_records_expose_read_only_lifecycle_fields() {
        let parent = AssetId::new();
        let dependency = AssetId::new();
        let mut store = AssetStore::default();

        let mut record = AssetRecord::new(parent, "dummy".to_string());
        record.state = AssetState::Failed;
        record.load_generation = 9;
        record.strong_ref_count = 2;
        record.dependency_ref_count = 1;
        record.dependencies = vec![dependency];
        record.error = Some(AssetError::DependencyFailed {
            id: parent,
            dependency,
        });
        record.failure_phase = Some(AssetFailurePhase::Dependency);
        record.reload_pending = true;
        store.records.insert(parent, record);

        let now = Instant::now();
        let diagnostic = store
            .diagnostic_record(parent, now)
            .expect("diagnostic snapshot should exist");
        assert_eq!(diagnostic.asset_id, parent);
        assert_eq!(diagnostic.asset_type, "dummy");
        assert_eq!(diagnostic.state, AssetState::Failed);
        assert_eq!(diagnostic.load_generation, 9);
        assert_eq!(diagnostic.strong_ref_count, 2);
        assert_eq!(diagnostic.dependency_ref_count, 1);
        assert_eq!(diagnostic.dependencies, vec![dependency]);
        assert_eq!(
            diagnostic.error,
            Some(AssetError::DependencyFailed {
                id: parent,
                dependency,
            })
        );
        assert_eq!(
            diagnostic.failure_phase,
            Some(AssetFailurePhase::Dependency)
        );
        assert!(diagnostic.reload_pending);
        assert!(diagnostic.install_progress.is_none());
        assert!(diagnostic.state_age <= Duration::from_secs(1));
        assert_eq!(store.diagnostic_records(now), vec![diagnostic]);
        assert!(store.diagnostic_record(AssetId::new(), now).is_none());
    }

    #[test]
    fn store_event_context_exposes_semantic_event_fields() {
        let id = AssetId::new();
        let dependency = AssetId::new();
        let mut store = AssetStore::default();

        let mut record = AssetRecord::new(id, "dummy".to_string());
        record.load_generation = 4;
        record.dependencies = vec![dependency];
        record.loaded_entry_fingerprint = Some("entry".to_string());
        record.loaded_cooked_hash = Some("hash".to_string());
        record.failure_phase = Some(AssetFailurePhase::Decode);
        record.reload_pending = true;
        store.records.insert(id, record);

        let context = store.event_record_context(id);
        assert_eq!(context.generation, 4);
        assert_eq!(context.asset_type, "dummy");
        assert_eq!(context.manifest_fingerprint.as_deref(), Some("entry"));
        assert_eq!(context.content_hash.as_deref(), Some("hash"));
        assert_eq!(context.dependencies, vec![dependency]);
        assert_eq!(context.failure_phase, Some(AssetFailurePhase::Decode));
        assert!(context.reload_pending);
        assert_eq!(
            store.event_record_context(AssetId::new()),
            AssetEventRecordContext::default()
        );
    }

    #[test]
    fn store_dependency_relation_queries_expose_links_without_record_layout() {
        let root = AssetId::new();
        let held = AssetId::new();
        let parent = AssetId::new();
        let empty = AssetId::new();
        let mut store = AssetStore::default();

        let mut parent_record = AssetRecord::new(parent, "dummy".to_string());
        parent_record.dependencies = vec![root];
        parent_record.held_dependencies = AssetDependencyLeases::new(vec![held]);
        store.records.insert(parent, parent_record);
        store
            .records
            .insert(empty, AssetRecord::new(empty, "dummy".to_string()));

        assert_eq!(store.record_dependency_links(parent), Some(vec![root]));
        assert_eq!(store.record_dependency_links(empty), None);
        assert!(store.record_references_dependency(parent, root));
        assert!(store.record_references_dependency(parent, held));
        assert!(!store.record_references_dependency(empty, root));
        assert_eq!(store.record_ids_referencing_dependency(root), vec![parent]);
        assert_eq!(store.record_ids_referencing_dependency(held), vec![parent]);
    }

    #[test]
    fn store_manifest_refresh_helpers_skip_runtime_and_raw_records() {
        let manifest_bound = AssetId::new();
        let runtime = AssetId::new();
        let raw = AssetId::new();
        let mut store = AssetStore::default();

        store.records.insert(
            manifest_bound,
            AssetRecord::new(manifest_bound, "old".to_string()),
        );
        store.insert_runtime_asset(runtime, RuntimeAsset("runtime"));
        store
            .records
            .insert(raw, AssetRecord::new_raw_texture("hero.png".into()));

        let ids = store.manifest_refresh_record_ids();
        assert_eq!(ids, vec![manifest_bound]);
        assert!(store.set_manifest_record_asset_type(manifest_bound, "new".to_string()));
        assert_eq!(store.records[&manifest_bound].asset_type, "new");
        assert!(!store.set_manifest_record_asset_type(AssetId::new(), "missing".to_string()));
    }

    #[test]
    fn store_record_state_helpers_only_mutate_existing_records() {
        let id = AssetId::new();
        let missing = AssetId::new();
        let mut store = AssetStore::default();
        store
            .records
            .insert(id, AssetRecord::new(id, "dummy".to_string()));

        assert!(store.set_record_state(id, AssetState::WaitingDependencies));
        assert_eq!(store.records[&id].state, AssetState::WaitingDependencies);
        assert!(!store.set_record_state(missing, AssetState::Loaded));

        assert!(store.advance_record_uninstall(id));
        assert_eq!(store.records[&id].state, AssetState::Unloading);
        assert!(!store.advance_record_uninstall(missing));

        assert!(store.finish_record_unload(id));
        assert_eq!(store.records[&id].state, AssetState::Unloaded);
        assert!(!store.finish_record_unload(missing));
    }

    #[test]
    fn store_reload_helpers_queue_and_prepare_existing_records() {
        let id = AssetId::new();
        let missing = AssetId::new();
        let mut store = AssetStore::default();
        store
            .records
            .insert(id, AssetRecord::new(id, "dummy".to_string()));

        assert!(store.queue_record_reload(id));
        assert!(store.records[&id].reload_pending);
        assert!(!store.queue_record_reload(missing));

        assert_eq!(
            store.prepare_record_reload(id, true),
            AssetReloadPrepareOutcome::Queued
        );
        assert_eq!(store.records[&id].state, AssetState::Loading);
        assert_eq!(
            store.prepare_record_reload(missing, true),
            AssetReloadPrepareOutcome::MissingRecord
        );
    }

    #[test]
    fn record_activation_advances_load_generation_for_fresh_or_failed_records() {
        let mut record = AssetRecord::new(AssetId::new(), "dummy".to_string());

        record.activate_for_load(None, 11);
        assert_eq!(record.state, AssetState::Loading);
        assert_eq!(record.load_generation, 1);
        assert_eq!(record.load_priority, 11);

        record.set_error(
            AssetError::Internal {
                message: "failed".to_string(),
            },
            AssetFailurePhase::Read,
        );
        record.state = AssetState::Failed;
        record.activate_for_load(None, 7);
        assert_eq!(record.state, AssetState::Loading);
        assert_eq!(record.load_generation, 2);
        assert_eq!(record.load_priority, 7);
        assert!(record.error.is_none());
    }

    #[test]
    fn store_record_activation_returns_state_and_generation() {
        let id = AssetId::new();
        let mut store = AssetStore::default();
        store
            .records
            .insert(id, AssetRecord::new(id, "dummy".to_string()));

        let activation = store
            .activate_record_for_load(id, None, 13)
            .expect("record should activate");

        assert_eq!(
            activation,
            AssetRecordLoadActivation {
                state: AssetState::Loading,
                load_generation: 1,
            }
        );
        assert_eq!(store.records[&id].load_priority, 13);
        assert!(matches!(
            store.activate_record_for_load(AssetId::new(), None, 0),
            Err(AssetError::AssetNotFound { .. })
        ));
    }

    #[test]
    fn record_release_transitions_cover_loading_installed_and_loaded_states() {
        let mut loading = AssetRecord::new(AssetId::new(), "dummy".to_string());
        loading.state = AssetState::Loading;
        assert!(loading.begin_release());
        assert_eq!(loading.state, AssetState::Unloaded);
        assert_eq!(loading.load_generation, 1);

        let mut installed = AssetRecord::new(AssetId::new(), "dummy".to_string());
        installed.state = AssetState::Installed;
        assert!(!installed.begin_release());
        assert_eq!(installed.state, AssetState::Uninstalling);
        installed.advance_uninstall();
        assert_eq!(installed.state, AssetState::Unloading);

        let mut loaded = AssetRecord::new(AssetId::new(), "dummy".to_string());
        loaded.state = AssetState::Loaded;
        loaded.dependencies.push(AssetId::new());
        assert!(!loaded.begin_release());
        assert_eq!(loaded.state, AssetState::Unloading);
        loaded.finish_unload();
        assert_eq!(loaded.state, AssetState::Unloaded);
        assert!(loaded.dependencies.is_empty());
    }

    #[test]
    fn store_release_scheduling_clears_dependency_leases_in_dependency_first_order() {
        let parent = AssetId::new();
        let dependency = AssetId::new();
        let mut store = AssetStore::default();

        let mut parent_record = AssetRecord::new(parent, "dummy".to_string());
        parent_record.state = AssetState::Loading;
        parent_record.held_dependencies = AssetDependencyLeases::new(vec![dependency]);
        store.records.insert(parent, parent_record);

        let mut dependency_record = AssetRecord::new(dependency, "dummy".to_string());
        dependency_record.state = AssetState::Loading;
        dependency_record.dependency_ref_count = 1;
        store.records.insert(dependency, dependency_record);

        let outcome = store.schedule_release_if_unused(parent);

        assert_eq!(outcome.immediate_unloaded, vec![dependency, parent]);
        assert_eq!(store.records[&parent].state, AssetState::Unloaded);
        assert_eq!(store.records[&dependency].state, AssetState::Unloaded);
        assert!(store.records[&parent].held_dependencies.ids().is_empty());
        assert_eq!(store.records[&dependency].dependency_ref_count, 0);
    }

    #[test]
    fn store_release_scheduling_skips_referenced_or_reload_pending_records() {
        let referenced = AssetId::new();
        let reload_pending = AssetId::new();
        let mut store = AssetStore::default();

        let mut referenced_record = AssetRecord::new(referenced, "dummy".to_string());
        referenced_record.state = AssetState::Loading;
        referenced_record.strong_ref_count = 1;
        store.records.insert(referenced, referenced_record);

        let mut reload_record = AssetRecord::new(reload_pending, "dummy".to_string());
        reload_record.state = AssetState::Loading;
        reload_record.reload_pending = true;
        store.records.insert(reload_pending, reload_record);

        assert!(store
            .schedule_release_if_unused(referenced)
            .immediate_unloaded
            .is_empty());
        assert!(store
            .schedule_release_if_unused(reload_pending)
            .immediate_unloaded
            .is_empty());
        assert_eq!(store.records[&referenced].state, AssetState::Loading);
        assert_eq!(store.records[&reload_pending].state, AssetState::Loading);
    }

    #[test]
    fn store_replaces_held_dependencies_and_reports_release_and_new_leases() {
        let parent = AssetId::new();
        let kept = AssetId::new();
        let removed = AssetId::new();
        let added = AssetId::new();
        let mut store = AssetStore::default();

        let mut parent_record = AssetRecord::new(parent, "dummy".to_string());
        parent_record.held_dependencies = AssetDependencyLeases::new(vec![kept, removed]);
        store.records.insert(parent, parent_record);

        let mut kept_record = AssetRecord::new(kept, "dummy".to_string());
        kept_record.dependency_ref_count = 1;
        store.records.insert(kept, kept_record);

        let mut removed_record = AssetRecord::new(removed, "dummy".to_string());
        removed_record.state = AssetState::Loading;
        removed_record.dependency_ref_count = 1;
        store.records.insert(removed, removed_record);

        let update = store.replace_held_dependencies(parent, vec![kept, added]);

        assert_eq!(update.new_dependency_leases, vec![added]);
        assert_eq!(update.release.immediate_unloaded, vec![removed]);
        assert_eq!(
            store.records[&parent].held_dependencies.ids(),
            &[kept, added]
        );
        assert_eq!(store.records[&kept].dependency_ref_count, 1);
        assert_eq!(store.records[&removed].dependency_ref_count, 0);
        assert_eq!(store.records[&removed].state, AssetState::Unloaded);
    }

    #[test]
    fn store_applies_dependency_lease_update_by_retain_and_activation() {
        let released = AssetId::new();
        let dependency = AssetId::new();
        let missing = AssetId::new();
        let mut store = AssetStore::default();
        store
            .records
            .insert(released, AssetRecord::new(released, "old".to_string()));

        let release = store.apply_dependency_lease_update(
            AssetDependencyLeaseUpdate {
                release: AssetReleaseOutcome {
                    immediate_unloaded: vec![released],
                },
                new_dependency_leases: vec![dependency, missing],
            },
            42,
            |id| (id == dependency).then(|| "dependency".to_string()),
        );

        assert_eq!(release.immediate_unloaded, vec![released]);
        let record = store.records.get(&dependency).expect("dependency record");
        assert_eq!(record.asset_type, "dependency");
        assert_eq!(record.dependency_ref_count, 1);
        assert_eq!(record.state, AssetState::Loading);
        assert_eq!(record.load_priority, 42);
        assert!(!store.records.contains_key(&missing));
    }

    #[test]
    fn store_extends_held_dependencies_without_releasing_existing_leases() {
        let parent = AssetId::new();
        let existing = AssetId::new();
        let added = AssetId::new();
        let mut store = AssetStore::default();

        let mut parent_record = AssetRecord::new(parent, "dummy".to_string());
        parent_record.held_dependencies = AssetDependencyLeases::new(vec![existing]);
        store.records.insert(parent, parent_record);

        let update = store.extend_held_dependencies(parent, &[existing, added]);

        assert_eq!(update.new_dependency_leases, vec![added]);
        assert!(update.release.immediate_unloaded.is_empty());
        assert_eq!(
            store.records[&parent].held_dependencies.ids(),
            &[existing, added]
        );
    }

    #[test]
    fn store_finish_loaded_record_updates_payload_and_dependency_leases() {
        let parent = AssetId::new();
        let old_dependency = AssetId::new();
        let new_dependency = AssetId::new();
        let mut store = AssetStore::default();

        let mut parent_record = AssetRecord::new(parent, "dummy".to_string());
        parent_record.held_dependencies = AssetDependencyLeases::new(vec![old_dependency]);
        store.records.insert(parent, parent_record);

        let mut old_record = AssetRecord::new(old_dependency, "dummy".to_string());
        old_record.state = AssetState::Loading;
        old_record.dependency_ref_count = 1;
        store.records.insert(old_dependency, old_record);

        let update = store
            .finish_loaded_record(
                parent,
                Arc::new(7u32),
                vec![new_dependency],
                "fingerprint".to_string(),
                Some("hash".to_string()),
            )
            .expect("parent record should exist");

        assert_eq!(update.new_dependency_leases, vec![new_dependency]);
        assert_eq!(update.release.immediate_unloaded, vec![old_dependency]);
        assert_eq!(store.records[&parent].state, AssetState::Loaded);
        assert_eq!(store.records[&parent].dependencies, vec![new_dependency]);
        assert_eq!(
            store.records[&parent].held_dependencies.ids(),
            &[new_dependency]
        );
        assert_eq!(store.records[&old_dependency].state, AssetState::Unloaded);
    }

    #[test]
    fn store_finish_loaded_record_extends_reload_dependency_leases() {
        let parent = AssetId::new();
        let existing = AssetId::new();
        let added = AssetId::new();
        let mut store = AssetStore::default();

        let mut parent_record = AssetRecord::new(parent, "dummy".to_string());
        parent_record.held_dependencies = AssetDependencyLeases::new(vec![existing]);
        let backup = AssetReloadBackup::from_record(&parent_record);
        parent_record.reload_backup = Some(backup);
        store.records.insert(parent, parent_record);

        let update = store
            .finish_loaded_record(
                parent,
                Arc::new(7u32),
                vec![existing, added],
                "fingerprint".to_string(),
                None,
            )
            .expect("parent record should exist");

        assert_eq!(update.new_dependency_leases, vec![added]);
        assert!(update.release.immediate_unloaded.is_empty());
        assert_eq!(
            store.records[&parent].held_dependencies.ids(),
            &[existing, added]
        );
        assert_eq!(store.records[&parent].dependencies, vec![existing, added]);
    }

    #[test]
    fn record_finish_install_clears_pending_reload_and_error_state() {
        let mut record = AssetRecord::new(AssetId::new(), "dummy".to_string());
        record.state = AssetState::Installing;
        record.reload_pending = true;
        record.set_error(
            AssetError::Internal {
                message: "install failed once".to_string(),
            },
            AssetFailurePhase::Install,
        );

        record.finish_install(Arc::new(7u32));

        assert_eq!(record.state, AssetState::Installed);
        assert!(record.installed.is_some());
        assert!(record.install_task.is_none());
        assert!(record.error.is_none());
        assert!(!record.reload_pending);
    }

    #[test]
    fn store_install_record_helpers_own_task_payload_and_type_checks() {
        let id = AssetId::new();
        let dependency = AssetId::new();
        let mut store = AssetStore::default();

        let mut record = AssetRecord::new(id, "dummy".to_string());
        record.state = AssetState::Installing;
        record.dependencies = vec![dependency];
        record.reload_pending = true;
        record.defer_install(Box::new(PendingInstallTask));
        store.records.insert(id, record);

        assert!(store
            .take_record_install_task(id)
            .expect("record should exist")
            .is_some());
        store
            .defer_record_install(id, Box::new(PendingInstallTask))
            .expect("record should exist");
        assert!(store
            .take_record_install_task(id)
            .expect("record should exist")
            .is_some());

        let completion = store
            .finish_record_install(id, Arc::new(9u32))
            .expect("install should finish");

        assert_eq!(
            completion,
            AssetRecordInstallCompletion {
                dependencies: vec![dependency],
                reloaded: true,
            }
        );
        assert!(store.validate_installed_asset_type(id, "dummy").is_ok());
        assert!(matches!(
            store.validate_installed_asset_type(id, "other"),
            Err(AssetError::AssetTypeMismatch { .. })
        ));
        let payload = store
            .installed_payload_for_uninstall(id)
            .expect("installed payload should be retained");
        assert_eq!(payload.downcast_ref::<u32>(), Some(&9));

        let not_installed = AssetId::new();
        store.records.insert(
            not_installed,
            AssetRecord::new(not_installed, "dummy".to_string()),
        );
        assert!(matches!(
            store.validate_installed_asset_type(not_installed, "dummy"),
            Err(AssetError::AssetNotInstalled { .. })
        ));
        assert!(matches!(
            store.take_record_install_task(AssetId::new()),
            Err(AssetError::InvalidState { .. })
        ));
    }

    #[test]
    fn record_finish_load_installs_loaded_payload_and_metadata() {
        let dependency = AssetId::new();
        let mut record = AssetRecord::new(AssetId::new(), "dummy".to_string());
        record.set_error(
            AssetError::Internal {
                message: "load failed once".to_string(),
            },
            AssetFailurePhase::Read,
        );

        record.finish_load(
            Arc::new(42u32),
            vec![dependency],
            "entry-fingerprint".to_string(),
            Some("hash".to_string()),
        );

        assert_eq!(record.state, AssetState::Loaded);
        assert!(record.loaded.is_some());
        assert_eq!(record.dependencies, vec![dependency]);
        assert_eq!(
            record.loaded_entry_fingerprint.as_deref(),
            Some("entry-fingerprint")
        );
        assert_eq!(record.loaded_cooked_hash.as_deref(), Some("hash"));
        assert!(record.error.is_none());
    }

    #[test]
    fn record_restore_reload_backup_keeps_last_good_payload_but_records_failure() {
        let dependency = AssetId::new();
        let mut record = AssetRecord::new(AssetId::new(), "dummy".to_string());
        record.loaded = Some(Arc::new(1u32));
        record.installed = Some(Arc::new(2u32));
        record.dependencies = vec![dependency];
        record.loaded_entry_fingerprint = Some("fingerprint".to_string());
        record.loaded_cooked_hash = Some("hash".to_string());
        let backup = AssetReloadBackup::from_record(&record);

        record.loaded = None;
        record.installed = None;
        record.dependencies.clear();
        record.reload_pending = true;
        let state = record.restore_reload_backup(
            backup,
            AssetError::Internal {
                message: "reload failed".to_string(),
            },
            AssetFailurePhase::Read,
        );

        assert_eq!(state, AssetState::Installed);
        assert_eq!(record.state, AssetState::Installed);
        assert!(record.loaded.is_some());
        assert!(record.installed.is_some());
        assert_eq!(record.dependencies, vec![dependency]);
        assert_eq!(
            record.loaded_entry_fingerprint.as_deref(),
            Some("fingerprint")
        );
        assert_eq!(record.loaded_cooked_hash.as_deref(), Some("hash"));
        assert!(record.error.is_some());
        assert!(!record.reload_pending);
    }

    #[test]
    fn record_fail_clears_payload_and_marks_failed() {
        let mut record = AssetRecord::new(AssetId::new(), "dummy".to_string());
        record.loaded = Some(Arc::new(1u32));
        record.installed = Some(Arc::new(2u32));
        record.reload_pending = true;
        record.loaded_entry_fingerprint = Some("fingerprint".to_string());
        record.loaded_cooked_hash = Some("hash".to_string());

        record.fail(
            AssetError::Internal {
                message: "failed".to_string(),
            },
            AssetFailurePhase::Install,
        );

        assert_eq!(record.state, AssetState::Failed);
        assert!(record.loaded.is_none());
        assert!(record.installed.is_none());
        assert!(record.error.is_some());
        assert!(record.loaded_entry_fingerprint.is_none());
        assert!(record.loaded_cooked_hash.is_none());
        assert!(!record.reload_pending);
    }

    #[test]
    fn store_fail_record_restores_reload_backup_or_clears_dependencies() {
        let parent = AssetId::new();
        let kept = AssetId::new();
        let removed = AssetId::new();
        let mut store = AssetStore::default();

        let mut parent_record = AssetRecord::new(parent, "dummy".to_string());
        parent_record.state = AssetState::Installed;
        parent_record.loaded = Some(Arc::new(1u32));
        parent_record.installed = Some(Arc::new(2u32));
        parent_record.dependencies = vec![kept];
        parent_record.held_dependencies = AssetDependencyLeases::new(vec![kept]);
        parent_record.loaded_entry_fingerprint = Some("fingerprint".to_string());
        parent_record.loaded_cooked_hash = Some("hash".to_string());
        parent_record.queue_reload();
        assert!(parent_record.prepare_reload(true));
        parent_record.held_dependencies = AssetDependencyLeases::new(vec![removed]);
        store.records.insert(parent, parent_record);

        let mut kept_record = AssetRecord::new(kept, "dummy".to_string());
        kept_record.dependency_ref_count = 1;
        store.records.insert(kept, kept_record);

        let mut removed_record = AssetRecord::new(removed, "dummy".to_string());
        removed_record.state = AssetState::Loading;
        removed_record.dependency_ref_count = 1;
        store.records.insert(removed, removed_record);

        let restored = store
            .fail_record(
                parent,
                AssetError::Internal {
                    message: "reload failed".to_string(),
                },
                AssetFailurePhase::Read,
            )
            .expect("parent record should exist");
        assert_eq!(restored.event_state, AssetState::Installed);
        assert_eq!(restored.dependency_update.new_dependency_leases, vec![kept]);
        assert_eq!(
            restored.dependency_update.release.immediate_unloaded,
            vec![removed]
        );
        assert_eq!(store.records[&parent].state, AssetState::Installed);
        assert_eq!(store.records[&parent].held_dependencies.ids(), &[kept]);
        assert!(store.records[&parent].error.is_some());

        let plain = AssetId::new();
        let dependency = AssetId::new();
        let mut plain_record = AssetRecord::new(plain, "dummy".to_string());
        plain_record.loaded = Some(Arc::new(3u32));
        plain_record.held_dependencies = AssetDependencyLeases::new(vec![dependency]);
        store.records.insert(plain, plain_record);
        let mut dependency_record = AssetRecord::new(dependency, "dummy".to_string());
        dependency_record.state = AssetState::Loading;
        dependency_record.dependency_ref_count = 1;
        store.records.insert(dependency, dependency_record);

        let failed = store
            .fail_record(
                plain,
                AssetError::Internal {
                    message: "failed".to_string(),
                },
                AssetFailurePhase::Install,
            )
            .expect("plain record should exist");
        assert_eq!(failed.event_state, AssetState::Failed);
        assert!(failed.dependency_update.new_dependency_leases.is_empty());
        assert_eq!(
            failed.dependency_update.release.immediate_unloaded,
            vec![dependency]
        );
        assert_eq!(store.records[&plain].state, AssetState::Failed);
        assert!(store.records[&plain].held_dependencies.ids().is_empty());
    }

    #[test]
    fn record_prepare_reload_backs_up_and_enters_loading_when_manifest_exists() {
        let dependency = AssetId::new();
        let mut record = AssetRecord::new(AssetId::new(), "dummy".to_string());
        record.state = AssetState::Installed;
        record.loaded = Some(Arc::new(1u32));
        record.installed = Some(Arc::new(2u32));
        record.dependencies = vec![dependency];
        record.loaded_entry_fingerprint = Some("fingerprint".to_string());
        record.loaded_cooked_hash = Some("hash".to_string());
        record.queue_reload();

        assert!(record.prepare_reload(true));

        assert_eq!(record.state, AssetState::Loading);
        assert_eq!(record.load_generation, 1);
        assert!(record.reload_pending);
        assert!(record.reload_backup.is_some());
        assert!(record.loaded.is_none());
        assert!(record.installed.is_some());
        assert!(record.dependencies.is_empty());
        assert!(record.loaded_entry_fingerprint.is_none());
        assert!(record.loaded_cooked_hash.is_none());
    }

    #[test]
    fn record_replace_runtime_resets_dependencies_and_marks_runtime_installed() {
        let mut record = AssetRecord::new(AssetId::new(), "old".to_string());
        record.dependencies.push(AssetId::new());
        record.held_dependencies = AssetDependencyLeases::new(record.dependencies.clone());
        record.loaded_entry_fingerprint = Some("fingerprint".to_string());
        record.loaded_cooked_hash = Some("hash".to_string());
        record.reload_pending = true;

        record.replace_runtime("dummy".to_string(), TypeId::of::<u32>(), Arc::new(5u32));

        assert_eq!(record.asset_type, "dummy");
        assert_eq!(record.state, AssetState::Installed);
        assert_eq!(record.requested_type, Some(TypeId::of::<u32>()));
        assert!(record.dependencies.is_empty());
        assert!(record.held_dependencies.ids().is_empty());
        assert!(record.loaded.is_some());
        assert!(record.installed.is_some());
        assert!(record.loaded_entry_fingerprint.is_none());
        assert!(record.loaded_cooked_hash.is_none());
        assert!(!record.reload_pending);
        assert!(record.runtime);
    }

    #[test]
    fn dependency_evaluation_creates_missing_records_and_reports_failed_dependencies() {
        let target = AssetId::new();
        let dependency = AssetId::new();
        let mut store = AssetStore::default();
        let mut record = AssetRecord::new(target, "dummy".to_string());
        record.dependencies.push(dependency);
        store.records.insert(target, record);

        assert_eq!(
            store.evaluate_dependency_records(target, 3, |id| {
                assert_eq!(id, dependency);
                Some("dummy".to_string())
            }),
            AssetDependencyEvaluation::Waiting
        );
        let dependency_record = store
            .records
            .get(&dependency)
            .expect("dependency record should be created");
        assert_eq!(dependency_record.state, AssetState::Loading);
        assert_eq!(dependency_record.load_priority, 3);

        store
            .records
            .get_mut(&dependency)
            .expect("dependency record should exist")
            .state = AssetState::Failed;

        assert_eq!(
            store.evaluate_dependency_records(target, 3, |_| Some("dummy".to_string())),
            AssetDependencyEvaluation::Failed { dependency }
        );
    }

    #[test]
    fn dependency_evaluation_reports_ready_when_all_dependencies_are_installed() {
        let target = AssetId::new();
        let dependency = AssetId::new();
        let mut store = AssetStore::default();
        let mut record = AssetRecord::new(target, "dummy".to_string());
        record.dependencies.push(dependency);
        store.records.insert(target, record);
        let mut dependency_record = AssetRecord::new(dependency, "dummy".to_string());
        dependency_record.state = AssetState::Installed;
        store.records.insert(dependency, dependency_record);

        assert_eq!(
            store.evaluate_dependency_records(target, 0, |_| Some("dummy".to_string())),
            AssetDependencyEvaluation::Ready
        );
    }
}
