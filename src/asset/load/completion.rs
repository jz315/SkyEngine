use std::time::Instant;

use super::{load_record_now, prepare_loaded_asset, AssetLoadQueue};
use super::{CompletedLoad, CompletedLoadFailure, TimedAssetLoadError};
use crate::asset::events::{self, AssetEventLog};
use crate::asset::failure;
use crate::asset::provider::AssetProvider;
use crate::asset::registry::{AssetFactories, AssetRegistry};
use crate::asset::request::AssetRequests;
use crate::asset::store::{AssetDependencyLeaseUpdate, AssetStore};
use crate::asset::types::{AssetConfig, AssetError, AssetFailurePhase, AssetId};

pub(crate) fn finish_completed_load(
    store: &mut AssetStore,
    completion: CompletedLoad,
) -> Result<Option<AssetDependencyLeaseUpdate>, CompletedLoadFailure> {
    if !store.accepts_load_completion(completion.id, completion.generation) {
        return Ok(None);
    }
    match completion.result {
        Ok(loaded) => {
            let prepared = prepare_loaded_asset(&completion.entry, loaded, completion.cooked_hash)
                .map_err(|error| CompletedLoadFailure {
                    id: completion.id,
                    phase: AssetFailurePhase::from_error(&error),
                    error,
                })?;
            let update = store
                .finish_loaded_record(
                    completion.id,
                    prepared.loaded,
                    prepared.dependencies,
                    prepared.entry_fingerprint,
                    prepared.content_hash,
                )
                .expect("record should exist after stale check");
            Ok(Some(update))
        }
        Err(error) => Err(CompletedLoadFailure {
            id: completion.id,
            phase: AssetFailurePhase::from_error(&error),
            error,
        }),
    }
}

pub(crate) fn apply_completed_load(
    store: &mut AssetStore,
    events: &mut AssetEventLog,
    load_queue: &mut AssetLoadQueue,
    manifest: &impl AssetRegistry,
    completion: CompletedLoad,
    dependency_priority: i32,
) -> Result<bool, CompletedLoadFailure> {
    let id = completion.id;
    match finish_completed_load(store, completion) {
        Ok(Some(update)) => {
            apply_loaded_dependency_update(
                store,
                events,
                load_queue,
                manifest,
                update,
                dependency_priority,
            );
            events::push_loaded_event(events, store, id);
            Ok(true)
        }
        Ok(None) => Ok(false),
        Err(failure) => Err(failure),
    }
}

pub(crate) fn drain_completed_loads(
    store: &mut AssetStore,
    events: &mut AssetEventLog,
    load_queue: &mut AssetLoadQueue,
    manifest: &impl AssetRegistry,
    dependency_priority: i32,
) -> Result<usize, CompletedLoadFailure> {
    let mut completed = 0usize;
    for completion in load_queue.drain_ready() {
        if apply_completed_load(
            store,
            events,
            load_queue,
            manifest,
            completion,
            dependency_priority,
        )? {
            completed += 1;
        }
    }
    Ok(completed)
}

pub(crate) fn drain_completed_loads_or_fail(
    store: &mut AssetStore,
    events: &mut AssetEventLog,
    requests: &mut AssetRequests,
    load_queue: &mut AssetLoadQueue,
    manifest: &impl AssetRegistry,
    dependency_priority: i32,
    failed_at: Instant,
) -> Result<usize, AssetError> {
    match drain_completed_loads(store, events, load_queue, manifest, dependency_priority) {
        Ok(completed) => Ok(completed),
        Err(load_failure) => {
            failure::fail_record_and_request(
                store,
                events,
                requests,
                load_failure.id,
                load_failure.error.clone(),
                load_failure.phase,
                dependency_priority,
                failed_at,
                |dependency| manifest.asset_type(dependency),
            );
            Err(load_failure.error)
        }
    }
}

pub(crate) fn load_record_now_and_apply(
    config: &AssetConfig,
    provider: &dyn AssetProvider,
    manifest: &impl AssetRegistry,
    factories: &AssetFactories,
    store: &mut AssetStore,
    events: &mut AssetEventLog,
    load_queue: &mut AssetLoadQueue,
    id: AssetId,
) -> Result<(), TimedAssetLoadError> {
    match load_record_now(config, provider, manifest, factories, store, id) {
        Ok(loaded) => {
            load_queue.record_timing(loaded.timings, true);
            apply_loaded_dependency_update(
                store,
                events,
                load_queue,
                manifest,
                loaded.dependency_update,
                config.io_default_priority,
            );
            events::push_loaded_event(events, store, id);
            Ok(())
        }
        Err(failure) => {
            load_queue.record_timing(failure.timings, false);
            Err(failure)
        }
    }
}

pub(crate) fn load_record_now_and_apply_or_fail(
    config: &AssetConfig,
    provider: &dyn AssetProvider,
    manifest: &impl AssetRegistry,
    factories: &AssetFactories,
    store: &mut AssetStore,
    events: &mut AssetEventLog,
    requests: &mut AssetRequests,
    load_queue: &mut AssetLoadQueue,
    id: AssetId,
    failed_at: Instant,
) -> Result<(), AssetError> {
    match load_record_now_and_apply(
        config, provider, manifest, factories, store, events, load_queue, id,
    ) {
        Ok(()) => Ok(()),
        Err(load_failure) => {
            let error = load_failure.error;
            failure::fail_record_and_request(
                store,
                events,
                requests,
                id,
                error.clone(),
                AssetFailurePhase::from_error(&error),
                config.io_default_priority,
                failed_at,
                |dependency| manifest.asset_type(dependency),
            );
            Err(error)
        }
    }
}

fn apply_loaded_dependency_update(
    store: &mut AssetStore,
    events: &mut AssetEventLog,
    load_queue: &mut AssetLoadQueue,
    manifest: &impl AssetRegistry,
    update: AssetDependencyLeaseUpdate,
    dependency_priority: i32,
) {
    let release = store.apply_dependency_lease_update(update, dependency_priority, |dependency| {
        manifest.asset_type(dependency)
    });
    load_queue.cancel_non_loading(store);
    events::push_release_events(events, store, release);
}
