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
