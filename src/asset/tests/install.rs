use std::any::{Any, TypeId};
use std::sync::mpsc;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

use crate::asset::events::AssetEventLog;
use crate::asset::install::*;
use crate::asset::registry::{
    AssetFactories, AssetRuntimeFactory, ErasedAssetFactory, LocalManifestRegistry,
};
use crate::asset::request::{AssetRequestPhase, AssetRequests};
use crate::asset::store::{AssetRecord, AssetStore};
use crate::asset::types::{
    AssetEventKind, AssetFailurePhase, AssetHandleProvider, AssetLoadContext, AssetManifestEntry,
    AssetRegistryManifest, AssetRequestStatus, AssetState, LoadedAsset, ASSET_SYSTEM_VERSION,
};
use crate::asset::{Asset, AssetConfig, AssetError, AssetId};

struct InstallTestAsset;

impl Asset for InstallTestAsset {
    const TYPE: &'static str = "install_test_asset";
}

struct TestHandleProvider;

impl AssetHandleProvider for TestHandleProvider {
    fn state_for_handle(&self, _id: AssetId) -> AssetState {
        AssetState::Unloaded
    }

    fn error_for_handle(&self, _id: AssetId) -> Option<AssetError> {
        None
    }

    fn get_for_handle(
        &self,
        id: AssetId,
        expected: &'static str,
        _expected_type_id: TypeId,
    ) -> Result<Arc<dyn Any + Send + Sync>, AssetError> {
        Err(AssetError::AssetTypeMismatch {
            id,
            expected,
            actual: "test".to_string(),
        })
    }
}

fn test_install_handles() -> (Sender<AssetId>, Weak<dyn AssetHandleProvider>) {
    let (release_tx, _release_rx) = mpsc::channel();
    let provider: Arc<dyn AssetHandleProvider> = Arc::new(TestHandleProvider);
    (release_tx, Arc::downgrade(&provider))
}

#[test]
fn install_budget_reports_yield_only_for_zero_remaining_time() {
    assert!(!AssetInstallBudget::unlimited().should_yield());
    assert!(!AssetInstallBudget::from_remaining_time(Duration::from_millis(1)).should_yield());
    assert!(AssetInstallBudget::from_remaining_time(Duration::ZERO).should_yield());
}

#[test]
fn install_limiter_enforces_count_time_and_single_poll_per_asset() {
    let first = AssetId::new();
    let second = AssetId::new();
    let mut count_limited = AssetInstallLimiter::per_update(Some(1), None, Instant::now());
    assert_eq!(
        count_limited.budget_for(first),
        Some(AssetInstallBudget::unlimited())
    );
    assert_eq!(count_limited.budget_for(second), None);

    let mut duplicate_limited = AssetInstallLimiter::per_update(None, None, Instant::now());
    assert_eq!(
        duplicate_limited.budget_for(first),
        Some(AssetInstallBudget::unlimited())
    );
    assert_eq!(duplicate_limited.budget_for(first), None);

    let mut time_limited =
        AssetInstallLimiter::per_update(None, Some(Duration::ZERO), Instant::now());
    assert_eq!(time_limited.budget_for(first), None);
}

#[test]
fn blocking_install_limiter_uses_unlimited_budget_but_still_dedupes_asset_poll() {
    let id = AssetId::new();
    let mut limiter = AssetInstallLimiter::blocking(Instant::now());

    assert_eq!(
        limiter.budget_for(id),
        Some(AssetInstallBudget::unlimited())
    );
    assert_eq!(limiter.budget_for(id), None);
}

#[test]
fn install_context_dependency_handle_retains_declared_dependency() {
    let parent = AssetId::new();
    let dependency = AssetId::new();
    let mut entry = test_entry(parent);
    entry.dependencies = vec![dependency];

    let mut store = AssetStore::default();
    let mut record = AssetRecord::new(dependency, InstallTestAsset::TYPE.to_string());
    record.state = AssetState::Installed;
    record.installed = Some(Arc::new(InstallTestAsset));
    store.records.insert(dependency, record);

    let (release_tx, release_rx) = mpsc::channel();
    let provider: Arc<dyn AssetHandleProvider> = Arc::new(TestHandleProvider);
    let mut ctx = AssetInstallContext::with_handle_factory(
        parent,
        &entry,
        &mut store,
        &release_tx,
        Arc::downgrade(&provider),
    );

    let undeclared = AssetId::new();
    assert!(matches!(
        ctx.dependency_handle::<InstallTestAsset>(undeclared),
        Err(AssetError::MissingDependency { dependency: id, .. }) if id == undeclared
    ));

    let handle = ctx
        .dependency_handle::<InstallTestAsset>(dependency)
        .expect("declared installed dependency should produce a strong handle");
    assert_eq!(handle.id(), dependency);
    drop(ctx);
    assert_eq!(store.records[&dependency].strong_ref_count, 1);

    drop(handle);
    let release = crate::asset::lease::drain_handle_releases(&release_rx, &mut store);
    assert_eq!(store.records[&dependency].strong_ref_count, 0);
    assert_eq!(release.immediate_unloaded, Vec::<AssetId>::new());
    assert_eq!(store.records[&dependency].state, AssetState::Uninstalling);
}

#[derive(Clone, Copy)]
enum TestInstallMode {
    Ready,
    PendingThenReady,
    Error,
}

struct TestFactory {
    mode: TestInstallMode,
}

struct TestRuntimeFactory;

impl AssetRuntimeFactory for TestRuntimeFactory {
    type Asset = InstallTestAsset;
    type Loaded = String;

    fn load(&self, _ctx: AssetLoadContext<'_>) -> Result<LoadedAsset<Self::Loaded>, AssetError> {
        unreachable!("install orchestration tests do not load through the factory")
    }

    fn begin_install(
        &self,
        _loaded: &Self::Loaded,
        _ctx: AssetInstallContext<'_>,
    ) -> Result<AssetInstallResult<Self::Asset>, AssetError> {
        Ok(AssetInstallResult::Ready(InstallTestAsset))
    }
}

struct FailingUninstallRuntimeFactory;

impl AssetRuntimeFactory for FailingUninstallRuntimeFactory {
    type Asset = InstallTestAsset;
    type Loaded = String;

    fn load(&self, _ctx: AssetLoadContext<'_>) -> Result<LoadedAsset<Self::Loaded>, AssetError> {
        unreachable!("install orchestration tests do not load through the factory")
    }

    fn begin_install(
        &self,
        _loaded: &Self::Loaded,
        _ctx: AssetInstallContext<'_>,
    ) -> Result<AssetInstallResult<Self::Asset>, AssetError> {
        Ok(AssetInstallResult::Ready(InstallTestAsset))
    }

    fn uninstall(
        &self,
        _installed: &Self::Asset,
        _ctx: AssetUninstallContext<'_>,
    ) -> Result<(), AssetError> {
        Err(AssetError::Unsupported {
            message: "uninstall failed".to_string(),
        })
    }
}

impl ErasedAssetFactory for TestFactory {
    fn asset_type(&self) -> &'static str {
        "dummy"
    }

    fn product_type_id(&self) -> TypeId {
        TypeId::of::<String>()
    }

    fn load(
        &self,
        _ctx: AssetLoadContext<'_>,
    ) -> Result<LoadedAsset<Arc<dyn Any + Send + Sync>>, AssetError> {
        unreachable!("install driver tests do not load through the factory")
    }

    fn begin_install(
        &self,
        loaded: &Arc<dyn Any + Send + Sync>,
        _ctx: AssetInstallContext<'_>,
    ) -> Result<AssetInstallResult<Arc<dyn Any + Send + Sync>>, AssetError> {
        match self.mode {
            TestInstallMode::Ready => {
                let loaded = loaded
                    .downcast_ref::<String>()
                    .expect("test payload should be a String");
                let installed: Arc<dyn Any + Send + Sync> = Arc::new(format!("installed:{loaded}"));
                Ok(AssetInstallResult::Ready(installed))
            }
            TestInstallMode::PendingThenReady => {
                Ok(AssetInstallResult::Pending(Box::new(TestInstallTask {
                    pending_polls: 0,
                })))
            }
            TestInstallMode::Error => Err(AssetError::Internal {
                message: "install failed".to_string(),
            }),
        }
    }
}

struct TestInstallTask {
    pending_polls: usize,
}

impl AssetInstallTask for TestInstallTask {
    type Output = Arc<dyn Any + Send + Sync>;

    fn poll_install(
        &mut self,
        _ctx: AssetInstallContext<'_>,
        _budget: AssetInstallBudget,
    ) -> Result<AssetInstallPoll<Self::Output>, AssetError> {
        if self.pending_polls > 0 {
            self.pending_polls -= 1;
            return Ok(AssetInstallPoll::Pending);
        }
        let installed: Arc<dyn Any + Send + Sync> = Arc::new("task-installed".to_string());
        Ok(AssetInstallPoll::Ready(installed))
    }
}

fn test_entry(id: AssetId) -> AssetManifestEntry {
    AssetManifestEntry {
        asset_id: id,
        asset_type: "dummy".to_string(),
        importer: "dummy.importer".to_string(),
        cooker: "dummy.cooker".to_string(),
        version: 1,
        source_path: "dummy.source".to_string(),
        cooked_path: "dummy.cooked".to_string(),
        dependencies: Vec::new(),
        import_settings: serde_json::Value::Null,
    }
}

fn loaded_record(id: AssetId) -> AssetStore {
    let mut store = AssetStore::default();
    let mut record = AssetRecord::new(id, "dummy".to_string());
    record.state = AssetState::Installing;
    record.loaded = Some(Arc::new("loaded".to_string()));
    record.dependencies.push(AssetId::new());
    store.records.insert(id, record);
    store
}

fn typed_loaded_record(id: AssetId) -> AssetStore {
    let mut store = AssetStore::default();
    let mut record = AssetRecord::new(id, InstallTestAsset::TYPE.to_string());
    record.state = AssetState::Installing;
    record.strong_ref_count = 1;
    record.loaded = Some(Arc::new("loaded".to_string()));
    store.records.insert(id, record);
    store
}

fn manifest_with(entry: AssetManifestEntry) -> LocalManifestRegistry {
    LocalManifestRegistry::new(AssetRegistryManifest {
        version: ASSET_SYSTEM_VERSION,
        target: "native".to_string(),
        provenance: Vec::new(),
        assets: vec![entry],
    })
}

#[test]
fn install_driver_finishes_immediate_factory_result() {
    let id = AssetId::new();
    let entry = test_entry(id);
    let factory = TestFactory {
        mode: TestInstallMode::Ready,
    };
    let mut store = loaded_record(id);
    let loaded = store.records[&id]
        .loaded
        .clone()
        .expect("record should have loaded payload");
    let (release_tx, handle_provider) = test_install_handles();

    let outcome = drive_record_install(
        &mut store,
        id,
        &entry,
        &factory,
        loaded,
        AssetInstallBudget::unlimited(),
        &release_tx,
        handle_provider,
    )
    .expect("install should succeed");

    assert!(matches!(
        outcome,
        AssetInstallRecordOutcome::Installed { .. }
    ));
    assert_eq!(store.records[&id].state, AssetState::Installed);
    assert!(store.records[&id].installed.is_some());
    assert!(store.records[&id].install_task.is_none());
}

#[test]
fn install_record_resolves_manifest_factory_and_emits_event() {
    let id = AssetId::new();
    let mut entry = test_entry(id);
    entry.asset_type = InstallTestAsset::TYPE.to_string();
    let manifest = manifest_with(entry);
    let mut factories = AssetFactories::default();
    factories.register(TestRuntimeFactory);
    let mut store = typed_loaded_record(id);
    let mut events = AssetEventLog::default();
    let mut cursor = events.cursor();
    let (release_tx, handle_provider) = test_install_handles();

    let progressed = install_record(
        &AssetConfig::default(),
        &manifest,
        &factories,
        &mut store,
        &mut events,
        &release_tx,
        Some(handle_provider),
        id,
        AssetInstallBudget::unlimited(),
        0,
    )
    .expect("install helper should drive manifest-backed install");

    assert!(progressed);
    assert_eq!(store.records[&id].state, AssetState::Installed);
    assert!(store.records[&id].installed.is_some());
    let emitted = events.events_since(&mut cursor);
    assert_eq!(emitted.len(), 1);
    assert_eq!(emitted[0].id, id);
    assert_eq!(emitted[0].kind, AssetEventKind::Installed);
}

#[test]
fn install_record_or_fail_records_failed_event_and_request() {
    let id = AssetId::new();
    let now = Instant::now();
    let mut entry = test_entry(id);
    entry.asset_type = InstallTestAsset::TYPE.to_string();
    let manifest = manifest_with(entry);
    let mut factories = AssetFactories::default();
    factories.register(TestRuntimeFactory);
    let mut store = typed_loaded_record(id);
    store
        .records
        .get_mut(&id)
        .expect("record exists")
        .load_generation = 3;
    let mut events = AssetEventLog::default();
    let mut cursor = events.cursor();
    let mut requests = AssetRequests::default();
    requests.enqueue(id, 3, None, 5, now);
    let request = requests.pop_queued().expect("queued request");
    requests.activate(request, AssetRequestPhase::Installing, 3, now);
    let (release_tx, _handle_provider) = test_install_handles();

    let error = install_record_or_fail(
        &AssetConfig::default(),
        &manifest,
        &factories,
        &mut store,
        &mut events,
        &mut requests,
        &release_tx,
        None,
        id,
        AssetInstallBudget::unlimited(),
        5,
        now,
    )
    .expect_err("missing install handle provider should fail");

    assert!(matches!(error, AssetError::Internal { .. }));
    assert_eq!(store.records[&id].state, AssetState::Failed);
    assert_eq!(
        store.records[&id].failure_phase,
        Some(AssetFailurePhase::Install)
    );
    let emitted = events.events_since(&mut cursor);
    assert_eq!(emitted.len(), 1);
    assert_eq!(emitted[0].id, id);
    assert_eq!(emitted[0].kind, AssetEventKind::Failed);
    assert_eq!(emitted[0].failure_phase, Some(AssetFailurePhase::Install));
    let failed = requests.failed_snapshots();
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].asset_id, id);
    assert_eq!(failed[0].generation, 3);
    assert_eq!(failed[0].status, AssetRequestStatus::Failed);
}

#[test]
fn install_driver_defers_and_later_finishes_pending_task() {
    let id = AssetId::new();
    let entry = test_entry(id);
    let factory = TestFactory {
        mode: TestInstallMode::PendingThenReady,
    };
    let mut store = loaded_record(id);
    let loaded = store.records[&id]
        .loaded
        .clone()
        .expect("record should have loaded payload");
    let (release_tx, handle_provider) = test_install_handles();

    let outcome = drive_record_install(
        &mut store,
        id,
        &entry,
        &factory,
        loaded.clone(),
        AssetInstallBudget::unlimited(),
        &release_tx,
        handle_provider.clone(),
    )
    .expect("install should defer");

    assert!(matches!(outcome, AssetInstallRecordOutcome::Pending));
    assert_eq!(store.records[&id].state, AssetState::Installing);
    assert!(store.records[&id].install_task.is_some());

    let outcome = drive_record_install(
        &mut store,
        id,
        &entry,
        &factory,
        loaded,
        AssetInstallBudget::unlimited(),
        &release_tx,
        handle_provider,
    )
    .expect("deferred install should finish");

    assert!(matches!(
        outcome,
        AssetInstallRecordOutcome::Installed { .. }
    ));
    assert_eq!(store.records[&id].state, AssetState::Installed);
    assert!(store.records[&id].installed.is_some());
    assert!(store.records[&id].install_task.is_none());
}

#[test]
fn install_driver_returns_factory_errors_without_recording_failure() {
    let id = AssetId::new();
    let entry = test_entry(id);
    let factory = TestFactory {
        mode: TestInstallMode::Error,
    };
    let mut store = loaded_record(id);
    let loaded = store.records[&id]
        .loaded
        .clone()
        .expect("record should have loaded payload");
    let (release_tx, handle_provider) = test_install_handles();

    let error = drive_record_install(
        &mut store,
        id,
        &entry,
        &factory,
        loaded,
        AssetInstallBudget::unlimited(),
        &release_tx,
        handle_provider,
    )
    .expect_err("factory error should be returned to caller");

    assert!(matches!(error, AssetError::Internal { .. }));
    assert_eq!(store.records[&id].state, AssetState::Installing);
    assert!(store.records[&id].error.is_none());
}

#[test]
fn apply_installed_record_updates_dependency_leases_and_emits_event() {
    let parent = AssetId::new();
    let child = AssetId::new();
    let mut store = AssetStore::default();
    let mut parent_record = AssetRecord::new(parent, "dummy".to_string());
    parent_record.state = AssetState::Installed;
    parent_record.strong_ref_count = 1;
    store.records.insert(parent, parent_record);

    let mut events = AssetEventLog::default();
    let mut cursor = events.cursor();
    apply_installed_record(
        &mut store,
        &mut events,
        parent,
        vec![child],
        false,
        11,
        |_| Some("dummy-dep".to_string()),
    );

    assert_eq!(store.records[&parent].held_dependencies.ids(), &[child]);
    assert_eq!(store.records[&child].dependency_ref_count, 1);
    assert_eq!(store.records[&child].state, AssetState::Loading);
    assert_eq!(store.records[&child].load_priority, 11);

    let emitted = events.events_since(&mut cursor);
    assert_eq!(emitted.len(), 1);
    assert_eq!(emitted[0].id, parent);
    assert_eq!(emitted[0].kind, AssetEventKind::Installed);
    assert_eq!(emitted[0].state, AssetState::Installed);
}

#[test]
fn apply_installed_record_emits_reloaded_for_reload_completion() {
    let id = AssetId::new();
    let mut store = AssetStore::default();
    let mut record = AssetRecord::new(id, "dummy".to_string());
    record.state = AssetState::Installed;
    record.strong_ref_count = 1;
    store.records.insert(id, record);

    let mut events = AssetEventLog::default();
    let mut cursor = events.cursor();
    apply_installed_record(&mut store, &mut events, id, Vec::new(), true, 0, |_| None);

    let emitted = events.events_since(&mut cursor);
    assert_eq!(emitted.len(), 1);
    assert_eq!(emitted[0].id, id);
    assert_eq!(emitted[0].kind, AssetEventKind::Reloaded);
    assert_eq!(emitted[0].state, AssetState::Installed);
}

#[test]
fn uninstall_record_or_fail_records_failed_event_and_request() {
    let id = AssetId::new();
    let now = Instant::now();
    let mut entry = test_entry(id);
    entry.asset_type = InstallTestAsset::TYPE.to_string();
    let manifest = manifest_with(entry);
    let mut factories = AssetFactories::default();
    factories.register(FailingUninstallRuntimeFactory);
    let mut store = AssetStore::default();
    let mut record = AssetRecord::new(id, InstallTestAsset::TYPE.to_string());
    record.state = AssetState::Uninstalling;
    record.strong_ref_count = 1;
    record.load_generation = 4;
    record.installed = Some(Arc::new(InstallTestAsset));
    store.records.insert(id, record);
    let mut events = AssetEventLog::default();
    let mut cursor = events.cursor();
    let mut requests = AssetRequests::default();
    requests.enqueue(id, 4, None, 2, now);
    let request = requests.pop_queued().expect("queued request");
    requests.activate(request, AssetRequestPhase::Unloading, 4, now);

    let error = uninstall_record_or_fail(
        &AssetConfig::default(),
        &manifest,
        &factories,
        &mut store,
        &mut events,
        &mut requests,
        id,
        2,
        now,
    )
    .expect_err("uninstall hook failure should enter failure lifecycle");

    assert!(matches!(error, AssetError::Unsupported { .. }));
    assert_eq!(store.records[&id].state, AssetState::Failed);
    assert_eq!(
        store.records[&id].failure_phase,
        Some(AssetFailurePhase::Uninstall)
    );
    let emitted = events.events_since(&mut cursor);
    assert_eq!(emitted.len(), 1);
    assert_eq!(emitted[0].id, id);
    assert_eq!(emitted[0].kind, AssetEventKind::Failed);
    assert_eq!(emitted[0].failure_phase, Some(AssetFailurePhase::Uninstall));
    let failed = requests.failed_snapshots();
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].asset_id, id);
    assert_eq!(failed[0].generation, 4);
    assert_eq!(failed[0].status, AssetRequestStatus::Failed);
}
