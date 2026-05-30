use std::time::{Duration, Instant};

use super::install::{AssetInstallBudget, AssetInstallLimiter};
use super::store::AssetStore;
use super::types::{AssetError, AssetId, AssetState};

#[derive(Clone, Copy, Debug)]
pub(crate) enum AssetDriveMode {
    Normal {
        install_budget_per_update: Option<usize>,
        install_time_budget: Option<Duration>,
        started_at: Instant,
    },
    Blocking {
        target_id: AssetId,
        force_synchronous_load: bool,
        started_at: Instant,
    },
}

pub(crate) trait AssetRecordDriver {
    fn store(&self) -> &AssetStore;
    fn store_mut(&mut self) -> &mut AssetStore;
    fn drain_load_completions(&mut self) -> Result<usize, AssetError>;
    fn spawn_load_record(&mut self, id: AssetId) -> Result<bool, AssetError>;
    fn load_record(&mut self, id: AssetId) -> Result<(), AssetError>;
    fn evaluate_dependencies(&mut self, id: AssetId) -> Result<Option<AssetState>, AssetError>;
    fn install_record(
        &mut self,
        id: AssetId,
        budget: AssetInstallBudget,
    ) -> Result<bool, AssetError>;
    fn uninstall_record(&mut self, id: AssetId) -> Result<bool, AssetError>;
    fn should_load_in_background(&self, id: AssetId) -> bool;
    fn has_current_inflight_load(&self, id: AssetId) -> bool;
    fn blocking_relevant_ids(&self, target_id: AssetId) -> Vec<AssetId>;
    fn record_unloaded(&mut self, id: AssetId);
}

pub(crate) fn drive_records<D>(driver: &mut D, mode: AssetDriveMode) -> Result<(), AssetError>
where
    D: AssetRecordDriver,
{
    if let AssetDriveMode::Blocking { target_id, .. } = mode {
        if !driver.store().contains_record(target_id) {
            return Err(AssetError::AssetNotFound { id: target_id });
        }
    }

    let mut install_limiter = match mode {
        AssetDriveMode::Normal {
            install_budget_per_update,
            install_time_budget,
            started_at,
        } => AssetInstallLimiter::per_update(
            install_budget_per_update,
            install_time_budget,
            started_at,
        ),
        AssetDriveMode::Blocking { started_at, .. } => AssetInstallLimiter::blocking(started_at),
    };

    let mut iterations = 0usize;
    loop {
        let ids = record_ids(driver, mode);
        iterations += 1;
        if iterations > max_iterations(driver, mode, ids.len()) {
            break;
        }

        let mut progressed = false;
        for id in ids {
            let state = match driver.store().record_state(id) {
                Some(state) => state,
                None => continue,
            };

            match state {
                AssetState::Loading => {
                    if should_skip_inflight_background_load(driver, mode, id) {
                        continue;
                    }

                    if should_submit_background_load(driver, mode, id) {
                        if driver.spawn_load_record(id)? {
                            progressed = true;
                        }
                    } else {
                        driver.load_record(id)?;
                        progressed = true;
                    }
                }
                AssetState::Loaded | AssetState::WaitingDependencies => {
                    if let Some(next_state) = driver.evaluate_dependencies(id)? {
                        if driver.store_mut().set_record_state(id, next_state) {
                            progressed = true;
                        }
                    }
                }
                AssetState::Installing => {
                    if let Some(budget) = install_limiter.budget_for(id) {
                        if driver.install_record(id, budget)? {
                            progressed = true;
                        }
                    }
                }
                AssetState::Uninstalling => {
                    if driver.uninstall_record(id)? {
                        progressed = true;
                    }
                }
                AssetState::Unloading => {
                    if driver.store_mut().finish_record_unload(id) {
                        driver.record_unloaded(id);
                        progressed = true;
                    }
                }
                AssetState::Unloaded | AssetState::Installed | AssetState::Failed => {}
            }
        }

        if driver.drain_load_completions()? > 0 {
            progressed = true;
        }

        if !progressed {
            break;
        }
    }

    Ok(())
}

fn record_ids<D>(driver: &D, mode: AssetDriveMode) -> Vec<AssetId>
where
    D: AssetRecordDriver,
{
    match mode {
        AssetDriveMode::Normal { .. } => driver.store().normal_drive_record_ids(),
        AssetDriveMode::Blocking { target_id, .. } => driver.blocking_relevant_ids(target_id),
    }
}

fn max_iterations<D>(driver: &D, mode: AssetDriveMode, current_id_count: usize) -> usize
where
    D: AssetRecordDriver,
{
    match mode {
        AssetDriveMode::Normal { .. } => driver.store().normal_drive_iteration_limit(),
        AssetDriveMode::Blocking { .. } => current_id_count.saturating_mul(4).max(8),
    }
}

fn should_skip_inflight_background_load<D>(driver: &D, mode: AssetDriveMode, id: AssetId) -> bool
where
    D: AssetRecordDriver,
{
    matches!(mode, AssetDriveMode::Blocking { .. }) && driver.has_current_inflight_load(id)
}

fn should_submit_background_load<D>(driver: &D, mode: AssetDriveMode, id: AssetId) -> bool
where
    D: AssetRecordDriver,
{
    match mode {
        AssetDriveMode::Normal { .. } => driver.should_load_in_background(id),
        AssetDriveMode::Blocking {
            force_synchronous_load,
            ..
        } => !force_synchronous_load && driver.should_load_in_background(id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset::store::{AssetRecord, AssetStore};

    #[derive(Default)]
    struct TestDriver {
        store: AssetStore,
        unloaded: Vec<AssetId>,
        loads: Vec<AssetId>,
        background_submits: Vec<AssetId>,
        dependency_states: Vec<AssetState>,
        install_results: Vec<bool>,
        uninstalls: Vec<AssetId>,
        completions: usize,
        inflight: bool,
    }

    impl AssetRecordDriver for TestDriver {
        fn store(&self) -> &AssetStore {
            &self.store
        }

        fn store_mut(&mut self) -> &mut AssetStore {
            &mut self.store
        }

        fn drain_load_completions(&mut self) -> Result<usize, AssetError> {
            let completions = self.completions;
            self.completions = 0;
            Ok(completions)
        }

        fn spawn_load_record(&mut self, id: AssetId) -> Result<bool, AssetError> {
            self.background_submits.push(id);
            if let Some(record) = self.store.records.get_mut(&id) {
                record.state = AssetState::Loaded;
            }
            Ok(true)
        }

        fn load_record(&mut self, id: AssetId) -> Result<(), AssetError> {
            self.loads.push(id);
            if let Some(record) = self.store.records.get_mut(&id) {
                record.state = AssetState::Loaded;
            }
            Ok(())
        }

        fn evaluate_dependencies(
            &mut self,
            _id: AssetId,
        ) -> Result<Option<AssetState>, AssetError> {
            Ok(self.dependency_states.pop())
        }

        fn install_record(
            &mut self,
            _id: AssetId,
            _budget: AssetInstallBudget,
        ) -> Result<bool, AssetError> {
            Ok(self.install_results.pop().unwrap_or(false))
        }

        fn uninstall_record(&mut self, id: AssetId) -> Result<bool, AssetError> {
            self.uninstalls.push(id);
            Ok(self.store.advance_record_uninstall(id))
        }

        fn should_load_in_background(&self, _id: AssetId) -> bool {
            true
        }

        fn has_current_inflight_load(&self, _id: AssetId) -> bool {
            self.inflight
        }

        fn blocking_relevant_ids(&self, target_id: AssetId) -> Vec<AssetId> {
            vec![target_id]
        }

        fn record_unloaded(&mut self, id: AssetId) {
            self.unloaded.push(id);
        }
    }

    fn record_with_state(id: AssetId, state: AssetState) -> AssetRecord {
        let mut record = AssetRecord::new(id, "dummy".to_string());
        record.state = state;
        record
    }

    #[test]
    fn normal_driver_finishes_uninstall_and_unload_with_event_callback() {
        let id = AssetId::new();
        let mut driver = TestDriver::default();
        driver
            .store
            .records
            .insert(id, record_with_state(id, AssetState::Uninstalling));

        drive_records(
            &mut driver,
            AssetDriveMode::Normal {
                install_budget_per_update: None,
                install_time_budget: None,
                started_at: Instant::now(),
            },
        )
        .expect("normal drive should finish");

        assert_eq!(driver.store.records[&id].state, AssetState::Unloaded);
        assert_eq!(driver.unloaded, vec![id]);
        assert_eq!(driver.uninstalls, vec![id]);
    }

    #[test]
    fn blocking_driver_rejects_missing_target() {
        let id = AssetId::new();
        let mut driver = TestDriver::default();

        let error = drive_records(
            &mut driver,
            AssetDriveMode::Blocking {
                target_id: id,
                force_synchronous_load: false,
                started_at: Instant::now(),
            },
        )
        .expect_err("missing target should be reported");

        assert_eq!(error, AssetError::AssetNotFound { id });
    }

    #[test]
    fn blocking_driver_skips_current_inflight_background_load() {
        let id = AssetId::new();
        let mut driver = TestDriver {
            inflight: true,
            ..Default::default()
        };
        driver
            .store
            .records
            .insert(id, record_with_state(id, AssetState::Loading));

        drive_records(
            &mut driver,
            AssetDriveMode::Blocking {
                target_id: id,
                force_synchronous_load: false,
                started_at: Instant::now(),
            },
        )
        .expect("blocking drive should finish");

        assert!(driver.loads.is_empty());
        assert!(driver.background_submits.is_empty());
    }

    #[test]
    fn normal_driver_submits_loading_records_by_priority() {
        let low = AssetId::new();
        let high = AssetId::new();
        let medium = AssetId::new();
        let mut driver = TestDriver::default();

        let mut low_record = record_with_state(low, AssetState::Loading);
        low_record.load_priority = 0;
        let mut high_record = record_with_state(high, AssetState::Loading);
        high_record.load_priority = 10;
        let mut medium_record = record_with_state(medium, AssetState::Loading);
        medium_record.load_priority = 5;
        driver.store.records.insert(low, low_record);
        driver.store.records.insert(high, high_record);
        driver.store.records.insert(medium, medium_record);

        drive_records(
            &mut driver,
            AssetDriveMode::Normal {
                install_budget_per_update: None,
                install_time_budget: None,
                started_at: Instant::now(),
            },
        )
        .expect("normal drive should submit loads");

        assert_eq!(driver.background_submits, vec![high, medium, low]);
    }
}
