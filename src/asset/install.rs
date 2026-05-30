use std::any::{Any, TypeId};
use std::collections::HashSet;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::sync::Weak;
use std::time::{Duration, Instant};

use super::events::{self, AssetEventLog};
use super::failure;
use super::provider;
use super::registry::{AssetFactories, AssetRegistry, ErasedAssetFactory};
use super::request::AssetRequests;
use super::store::AssetStore;
use super::types::{
    Asset, AssetConfig, AssetError, AssetFailurePhase, AssetHandleProvider, AssetId, AssetLease,
    AssetManifestEntry, AssetRequestProgress, Handle,
};

pub struct AssetInstallContext<'a> {
    pub asset_id: AssetId,
    pub entry: &'a AssetManifestEntry,
    handle_factory: Option<AssetInstallHandleFactory<'a>>,
}

struct AssetInstallHandleFactory<'a> {
    store: &'a mut AssetStore,
    release_tx: &'a Sender<AssetId>,
    handle_provider: Weak<dyn AssetHandleProvider>,
}

impl<'a> AssetInstallContext<'a> {
    #[must_use]
    pub fn new(asset_id: AssetId, entry: &'a AssetManifestEntry) -> Self {
        Self {
            asset_id,
            entry,
            handle_factory: None,
        }
    }

    pub(crate) fn with_handle_factory(
        asset_id: AssetId,
        entry: &'a AssetManifestEntry,
        store: &'a mut AssetStore,
        release_tx: &'a Sender<AssetId>,
        handle_provider: Weak<dyn AssetHandleProvider>,
    ) -> Self {
        Self {
            asset_id,
            entry,
            handle_factory: Some(AssetInstallHandleFactory {
                store,
                release_tx,
                handle_provider,
            }),
        }
    }

    pub fn dependency_handle<T: Asset>(&mut self, id: AssetId) -> Result<Handle<T>, AssetError> {
        if !self.entry.dependencies.contains(&id) {
            return Err(AssetError::MissingDependency {
                id: self.asset_id,
                dependency: id,
            });
        }

        let factory = self
            .handle_factory
            .as_mut()
            .ok_or_else(|| AssetError::Internal {
                message: "asset install context cannot create dependency handles outside the Assets install driver".to_string(),
            })?;
        factory.store.validate_installed_asset_type(id, T::TYPE)?;

        if !factory
            .store
            .retain_existing_direct(id, Some(TypeId::of::<T>()))
        {
            return Err(AssetError::AssetNotFound { id });
        }
        let lease = Arc::new(AssetLease::new(
            id,
            factory.release_tx.clone(),
            factory.handle_provider.clone(),
        ));
        Ok(Handle::from_lease(id, lease))
    }
}

pub struct AssetUninstallContext<'a> {
    pub asset_id: AssetId,
    pub entry: &'a AssetManifestEntry,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AssetInstallBudget {
    remaining_time: Option<Duration>,
}

impl AssetInstallBudget {
    #[must_use]
    pub fn unlimited() -> Self {
        Self {
            remaining_time: None,
        }
    }

    #[must_use]
    pub fn from_remaining_time(remaining_time: Duration) -> Self {
        Self {
            remaining_time: Some(remaining_time),
        }
    }

    #[must_use]
    pub fn remaining_time(self) -> Option<Duration> {
        self.remaining_time
    }

    #[must_use]
    pub fn should_yield(self) -> bool {
        self.remaining_time
            .is_some_and(|remaining| remaining.is_zero())
    }
}

pub(crate) struct AssetInstallLimiter {
    remaining_polls: usize,
    time_budget: Option<Duration>,
    started_at: Instant,
    polled: HashSet<AssetId>,
}

impl AssetInstallLimiter {
    pub(crate) fn per_update(
        max_polls: Option<usize>,
        time_budget: Option<Duration>,
        started_at: Instant,
    ) -> Self {
        Self {
            remaining_polls: max_polls.unwrap_or(usize::MAX),
            time_budget,
            started_at,
            polled: HashSet::default(),
        }
    }

    pub(crate) fn blocking(started_at: Instant) -> Self {
        Self::per_update(None, None, started_at)
    }

    pub(crate) fn budget_for(&mut self, id: AssetId) -> Option<AssetInstallBudget> {
        if self.remaining_polls == 0 || !self.polled.insert(id) {
            return None;
        }

        let budget = match self.time_budget {
            Some(time_budget) => {
                if self.started_at.elapsed() >= time_budget {
                    return None;
                }
                AssetInstallBudget::from_remaining_time(
                    time_budget.saturating_sub(self.started_at.elapsed()),
                )
            }
            None => AssetInstallBudget::unlimited(),
        };

        self.remaining_polls = self.remaining_polls.saturating_sub(1);
        Some(budget)
    }
}

pub enum AssetInstallPoll<T> {
    Pending,
    Ready(T),
}

pub trait AssetInstallTask: Send + 'static {
    type Output: Send + Sync + 'static;

    fn progress(&self) -> Option<AssetRequestProgress> {
        None
    }

    fn poll_install(
        &mut self,
        ctx: AssetInstallContext<'_>,
        budget: AssetInstallBudget,
    ) -> Result<AssetInstallPoll<Self::Output>, AssetError>;
}

pub enum AssetInstallResult<T: Send + Sync + 'static> {
    Ready(T),
    Pending(Box<dyn AssetInstallTask<Output = T>>),
}

#[derive(Debug)]
pub(crate) enum AssetInstallRecordOutcome {
    Installed {
        dependencies: Vec<AssetId>,
        reloaded: bool,
    },
    Pending,
}

pub(crate) fn drive_record_install(
    store: &mut AssetStore,
    id: AssetId,
    entry: &AssetManifestEntry,
    factory: &dyn ErasedAssetFactory,
    loaded: Arc<dyn Any + Send + Sync>,
    budget: AssetInstallBudget,
    release_tx: &Sender<AssetId>,
    handle_provider: Weak<dyn AssetHandleProvider>,
) -> Result<AssetInstallRecordOutcome, AssetError> {
    let result = if let Some(mut task) = store.take_record_install_task(id)? {
        let context =
            AssetInstallContext::with_handle_factory(id, entry, store, release_tx, handle_provider);
        match task.poll_install(context, budget) {
            Ok(AssetInstallPoll::Ready(installed)) => Ok(AssetInstallResult::Ready(installed)),
            Ok(AssetInstallPoll::Pending) => Ok(AssetInstallResult::Pending(task)),
            Err(error) => Err(error),
        }
    } else {
        let context =
            AssetInstallContext::with_handle_factory(id, entry, store, release_tx, handle_provider);
        factory.begin_install(&loaded, context)
    }?;

    match result {
        AssetInstallResult::Ready(installed) => {
            let completion = store.finish_record_install(id, installed)?;
            Ok(AssetInstallRecordOutcome::Installed {
                dependencies: completion.dependencies,
                reloaded: completion.reloaded,
            })
        }
        AssetInstallResult::Pending(task) => {
            store.defer_record_install(id, task)?;
            Ok(AssetInstallRecordOutcome::Pending)
        }
    }
}

pub(crate) fn drive_record_uninstall(
    store: &mut AssetStore,
    id: AssetId,
    entry: &AssetManifestEntry,
    factory: &dyn ErasedAssetFactory,
) -> Result<bool, AssetError> {
    if let Some(installed) = store.installed_payload_for_uninstall(id) {
        factory.uninstall(
            &installed,
            AssetUninstallContext {
                asset_id: id,
                entry,
            },
        )?;
    }
    Ok(store.advance_record_uninstall(id))
}

pub(crate) fn install_record(
    config: &AssetConfig,
    manifest: &impl AssetRegistry,
    factories: &AssetFactories,
    store: &mut AssetStore,
    events: &mut AssetEventLog,
    release_tx: &Sender<AssetId>,
    handle_provider: Option<Weak<dyn AssetHandleProvider>>,
    id: AssetId,
    budget: AssetInstallBudget,
    dependency_priority: i32,
) -> Result<bool, AssetError> {
    let entry = provider::record_manifest_entry(config, manifest, store, id)?;
    let factory = factories.for_entry(&entry)?;
    let loaded = store.loaded_payload_for_install(id)?;
    let handle_provider = handle_provider.ok_or_else(|| AssetError::Internal {
        message: "asset install handle provider is not initialized".to_string(),
    })?;

    match drive_record_install(
        store,
        id,
        &entry,
        factory.as_ref(),
        loaded,
        budget,
        release_tx,
        handle_provider,
    )? {
        AssetInstallRecordOutcome::Installed {
            dependencies,
            reloaded,
        } => {
            apply_installed_record(
                store,
                events,
                id,
                dependencies,
                reloaded,
                dependency_priority,
                |dependency| manifest.asset_type(dependency),
            );
            Ok(true)
        }
        AssetInstallRecordOutcome::Pending => Ok(false),
    }
}

pub(crate) fn install_record_or_fail(
    config: &AssetConfig,
    manifest: &impl AssetRegistry,
    factories: &AssetFactories,
    store: &mut AssetStore,
    events: &mut AssetEventLog,
    requests: &mut AssetRequests,
    release_tx: &Sender<AssetId>,
    handle_provider: Option<Weak<dyn AssetHandleProvider>>,
    id: AssetId,
    budget: AssetInstallBudget,
    dependency_priority: i32,
    failed_at: Instant,
) -> Result<bool, AssetError> {
    match install_record(
        config,
        manifest,
        factories,
        store,
        events,
        release_tx,
        handle_provider,
        id,
        budget,
        dependency_priority,
    ) {
        Ok(progressed) => Ok(progressed),
        Err(error) => {
            failure::fail_record_and_request(
                store,
                events,
                requests,
                id,
                error.clone(),
                AssetFailurePhase::Install,
                dependency_priority,
                failed_at,
                |dependency| manifest.asset_type(dependency),
            );
            Err(error)
        }
    }
}

pub(crate) fn uninstall_record(
    config: &AssetConfig,
    manifest: &impl AssetRegistry,
    factories: &AssetFactories,
    store: &mut AssetStore,
    id: AssetId,
) -> Result<bool, AssetError> {
    if store.is_runtime_record(id) {
        return Ok(store.advance_record_uninstall(id));
    }

    let entry = provider::record_manifest_entry(config, manifest, store, id)?;
    let factory = factories.for_entry(&entry)?;
    drive_record_uninstall(store, id, &entry, factory.as_ref())
}

pub(crate) fn uninstall_record_or_fail(
    config: &AssetConfig,
    manifest: &impl AssetRegistry,
    factories: &AssetFactories,
    store: &mut AssetStore,
    events: &mut AssetEventLog,
    requests: &mut AssetRequests,
    id: AssetId,
    dependency_priority: i32,
    failed_at: Instant,
) -> Result<bool, AssetError> {
    match uninstall_record(config, manifest, factories, store, id) {
        Ok(progressed) => Ok(progressed),
        Err(error) => {
            failure::fail_record_and_request(
                store,
                events,
                requests,
                id,
                error.clone(),
                AssetFailurePhase::Uninstall,
                dependency_priority,
                failed_at,
                |dependency| manifest.asset_type(dependency),
            );
            Err(error)
        }
    }
}

pub(crate) fn apply_installed_record<F>(
    store: &mut AssetStore,
    events: &mut AssetEventLog,
    id: AssetId,
    dependencies: Vec<AssetId>,
    reloaded: bool,
    dependency_priority: i32,
    asset_type_for: F,
) where
    F: FnMut(AssetId) -> Option<String>,
{
    let update = store.replace_held_dependencies(id, dependencies);
    let release = store.apply_dependency_lease_update(update, dependency_priority, asset_type_for);
    events::push_release_events(events, store, release);
    if reloaded {
        events::push_reloaded_event(events, store, id);
    } else {
        events::push_installed_event(events, store, id);
    }
    let release = store.schedule_release_if_unused(id);
    events::push_release_events(events, store, release);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::any::TypeId;
    use std::sync::mpsc;

    use crate::asset::registry::{AssetRuntimeFactory, LocalManifestRegistry};
    use crate::asset::request::{AssetRequestPhase, AssetRequests};
    use crate::asset::store::{AssetRecord, AssetStore};
    use crate::asset::types::{
        AssetEventKind, AssetFailurePhase, AssetLoadContext, AssetRegistryManifest,
        AssetRequestStatus, AssetState, LoadedAsset, ASSET_SYSTEM_VERSION,
    };

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

        fn load(
            &self,
            _ctx: AssetLoadContext<'_>,
        ) -> Result<LoadedAsset<Self::Loaded>, AssetError> {
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

        fn load(
            &self,
            _ctx: AssetLoadContext<'_>,
        ) -> Result<LoadedAsset<Self::Loaded>, AssetError> {
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
                    let installed: Arc<dyn Any + Send + Sync> =
                        Arc::new(format!("installed:{loaded}"));
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
}
