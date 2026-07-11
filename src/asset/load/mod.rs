use super::events::AssetEventLog;
use super::failure;
use super::provider::{self, AssetProvider};
use super::registry::{AssetFactories, AssetRegistry};
use super::request::AssetRequests;
use super::store::{AssetDependencyLeaseUpdate, AssetStore};
use super::types::{
    AssetConfig, AssetError, AssetFailurePhase, AssetId, AssetManifestEntry, LoadedAsset,
};
use std::any::Any;
use std::sync::Arc;
use std::time::Instant;
mod types;
pub(crate) use types::{
    AssetLoadPhaseTracker, AssetLoadTimingSample, AssetSourceLoadPhase, CompletedLoad,
    CompletedLoadFailure, LoadedSourceAsset, TimedAssetLoadError,
};
mod policy;
pub(crate) use policy::{has_current_inflight_load, should_load_record_in_background};
mod fingerprint;
use fingerprint::normalize_dependencies;
pub(crate) use fingerprint::{hash_bytes, manifest_entry_fingerprint};
pub(crate) struct PreparedLoadedAsset {
    pub(crate) loaded: Arc<dyn Any + Send + Sync>,
    pub(crate) dependencies: Vec<AssetId>,
    pub(crate) entry_fingerprint: String,
    pub(crate) content_hash: Option<String>,
}
pub(crate) fn prepare_loaded_asset(
    entry: &AssetManifestEntry,
    loaded: LoadedAsset<Arc<dyn Any + Send + Sync>>,
    content_hash: Option<String>,
) -> Result<PreparedLoadedAsset, AssetError> {
    let dependencies = if loaded.dependencies.is_empty() {
        entry.dependencies.clone()
    } else {
        loaded.dependencies
    };
    Ok(PreparedLoadedAsset {
        loaded: loaded.loaded,
        dependencies: normalize_dependencies(dependencies),
        entry_fingerprint: manifest_entry_fingerprint(entry)?,
        content_hash,
    })
}
pub(crate) fn submit_record_load(
    config: &AssetConfig,
    provider: &dyn AssetProvider,
    manifest: &impl AssetRegistry,
    factories: &AssetFactories,
    store: &AssetStore,
    load_queue: &mut AssetLoadQueue,
    id: AssetId,
) -> Result<bool, AssetError> {
    let source = provider::resolve_record_source(config, provider, manifest, store, id)?;
    let entry = source.entry().clone();
    let generation = store.load_generation_or_not_found(id)?;
    let asset_root = config.asset_root.clone();
    let cooked_root = config.cooked_root();
    let factory = factories.for_entry(&entry)?;
    let priority = store.load_priority_or(id, config.io_default_priority);
    load_queue.submit_with_phase(id, generation, priority, move |phase| {
        let result = load_resolved_source_asset_with_phase(
            id,
            &source,
            factory.as_ref(),
            &asset_root,
            &cooked_root,
            &phase,
        );
        match result {
            Ok(loaded_source) => CompletedLoad {
                id,
                generation,
                entry,
                cooked_hash: Some(loaded_source.content_hash),
                timings: loaded_source.timings,
                result: Ok(loaded_source.loaded),
            },
            Err(failure) => CompletedLoad {
                id,
                generation,
                entry,
                cooked_hash: None,
                timings: failure.timings,
                result: Err(*failure.error),
            },
        }
    })
}
pub(crate) fn submit_record_load_or_fail(
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
) -> Result<bool, AssetError> {
    match submit_record_load(config, provider, manifest, factories, store, load_queue, id) {
        Ok(submitted) => Ok(submitted),
        Err(error) => {
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
pub(crate) struct LoadedRecordNow {
    pub(crate) dependency_update: AssetDependencyLeaseUpdate,
    pub(crate) timings: AssetLoadTimingSample,
}
pub(crate) fn load_record_now(
    config: &AssetConfig,
    provider: &dyn AssetProvider,
    manifest: &impl AssetRegistry,
    factories: &AssetFactories,
    store: &mut AssetStore,
    id: AssetId,
) -> Result<LoadedRecordNow, TimedAssetLoadError> {
    let source = provider::resolve_record_source(config, provider, manifest, store, id)?;
    let entry = source.entry().clone();
    let cooked_root = config.cooked_root();
    let factory = factories.for_entry(&entry)?;
    let loaded_source = load_resolved_source_asset(
        id,
        &source,
        factory.as_ref(),
        &config.asset_root,
        &cooked_root,
    )?;
    let timings = loaded_source.timings;
    let prepared = prepare_loaded_asset(
        &entry,
        loaded_source.loaded,
        Some(loaded_source.content_hash),
    )
    .map_err(|error| TimedAssetLoadError {
        error: Box::new(error),
        timings,
    })?;
    let dependency_update = store
        .finish_loaded_record(
            id,
            prepared.loaded,
            prepared.dependencies,
            prepared.entry_fingerprint,
            prepared.content_hash,
        )
        .ok_or(AssetError::AssetNotFound { id })
        .map_err(|error| TimedAssetLoadError {
            error: Box::new(error),
            timings,
        })?;
    Ok(LoadedRecordNow {
        dependency_update,
        timings,
    })
}
mod queue;
mod timing;
pub(crate) use queue::AssetLoadQueue;
mod source;
pub(crate) use source::{load_resolved_source_asset, load_resolved_source_asset_with_phase};
mod completion;
#[cfg(test)]
pub(crate) use completion::{
    apply_completed_load, finish_completed_load, load_record_now_and_apply,
};
pub(crate) use completion::{drain_completed_loads_or_fail, load_record_now_and_apply_or_fail};
