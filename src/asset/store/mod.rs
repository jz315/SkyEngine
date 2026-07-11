use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

type InstalledAsset = Arc<dyn Any + Send + Sync>;
type ErasedAssetInstallTask = Box<dyn AssetInstallTask<Output = InstalledAsset>>;

use super::font::FontAsset;
use super::install::AssetInstallTask;
use super::lease::AssetDependencyLeases;
use super::texture::TextureAsset;
use super::types::{
    Asset, AssetError, AssetFailurePhase, AssetId, AssetRequestProgress, AssetState,
};

mod record;
pub(crate) use record::{
    AssetEventRecordContext, AssetRawSourceRecord, AssetRecordDiagnostic,
    AssetRecordInstallCompletion, AssetRecordLoadActivation, AssetReloadScanRecord,
};
mod reload;
pub(crate) use reload::AssetReloadBackup;
mod diagnostics;
mod runtime;

#[derive(Default)]
pub(crate) struct AssetStore {
    pub(crate) records: HashMap<AssetId, AssetRecord>,
    pub(crate) raw_textures: HashMap<String, AssetId>,
    pub(crate) raw_fonts: HashMap<String, AssetId>,
}

impl AssetStore {
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
    ) -> Result<Option<ErasedAssetInstallTask>, AssetError> {
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
        task: ErasedAssetInstallTask,
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

            if let std::collections::hash_map::Entry::Vacant(e) = self.records.entry(dependency) {
                e.insert(AssetRecord::new(dependency, asset_type));
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
    pub(crate) install_task: Option<ErasedAssetInstallTask>,
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

    pub(crate) fn take_install_task(&mut self) -> Option<ErasedAssetInstallTask> {
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

    pub(crate) fn defer_install(&mut self, task: ErasedAssetInstallTask) {
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
