use std::any::{Any, TypeId};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use super::diagnostics;
use super::events::AssetEventLog;
use super::load::AssetLoadQueue;
use super::provider::AssetProvider;
use super::registry::{
    lookup_source_asset, manifest_snapshot, metadata, source_path, typed_asset_path, watch_paths,
    AssetFactories, LocalManifestRegistry,
};
use super::reload::AssetReloadService;
use super::request::AssetRequests;
use super::store::AssetStore;
use super::types::{
    Asset, AssetConfig, AssetDiagnosticsSnapshot, AssetError, AssetEvent, AssetEventCursor,
    AssetFailurePhase, AssetFailureSnapshot, AssetId, AssetMetadata, AssetPath,
    AssetRegistryManifest, AssetReloadReport, AssetReloadStatus, AssetRequestSnapshot, AssetState,
    AssetStats, AssetWatchPaths,
};

pub(crate) fn last_reload_report(reload: &AssetReloadService) -> AssetReloadReport {
    reload.last_report()
}

pub(crate) fn config(config: &AssetConfig) -> AssetConfig {
    config.clone()
}

pub(crate) fn state_for_handle(store: &AssetStore, id: AssetId) -> AssetState {
    store.state_for_handle(id)
}

pub(crate) fn error_for_handle(store: &AssetStore, id: AssetId) -> Option<AssetError> {
    store.error_for_handle(id)
}

pub(crate) fn failure_phase(store: &AssetStore, id: AssetId) -> Option<AssetFailurePhase> {
    store.failure_phase(id)
}

pub(crate) fn get_for_handle(
    store: &AssetStore,
    id: AssetId,
    expected_type: &'static str,
    expected_type_id: TypeId,
) -> Result<Arc<dyn Any + Send + Sync>, AssetError> {
    store.get_for_handle(id, expected_type, expected_type_id)
}

pub(crate) fn event_cursor(events: &AssetEventLog) -> AssetEventCursor {
    events.cursor()
}

pub(crate) fn events_since(
    events: &AssetEventLog,
    cursor: &mut AssetEventCursor,
) -> Vec<AssetEvent> {
    events.events_since(cursor)
}

pub(crate) fn stats(
    store: &AssetStore,
    requests: &AssetRequests,
    load_queue: &AssetLoadQueue,
    provider: &dyn AssetProvider,
    events: &AssetEventLog,
) -> AssetStats {
    diagnostics::stats(
        store,
        requests,
        load_queue,
        provider,
        events,
        Instant::now(),
    )
}

pub(crate) fn queued_request_snapshots(requests: &AssetRequests) -> Vec<AssetRequestSnapshot> {
    diagnostics::queued_request_snapshots(requests, Instant::now())
}

pub(crate) fn active_request_snapshots(
    store: &AssetStore,
    requests: &AssetRequests,
    load_queue: &AssetLoadQueue,
) -> Vec<AssetRequestSnapshot> {
    diagnostics::active_request_snapshots(store, requests, load_queue, Instant::now())
}

pub(crate) fn failed_request_snapshots(
    store: &AssetStore,
    requests: &AssetRequests,
) -> Vec<AssetRequestSnapshot> {
    diagnostics::failed_request_snapshots(store, requests)
}

pub(crate) fn canceled_request_snapshots(requests: &AssetRequests) -> Vec<AssetRequestSnapshot> {
    diagnostics::canceled_request_snapshots(requests)
}

pub(crate) fn failed_asset_snapshots(store: &AssetStore) -> Vec<AssetFailureSnapshot> {
    diagnostics::failed_asset_snapshots(store)
}

pub(crate) fn diagnostics_snapshot(
    config: &AssetConfig,
    store: &AssetStore,
    requests: &AssetRequests,
    load_queue: &AssetLoadQueue,
    provider: &dyn AssetProvider,
    events: &AssetEventLog,
    reload: &AssetReloadService,
) -> AssetDiagnosticsSnapshot {
    diagnostics::snapshot(
        config,
        store,
        requests,
        load_queue,
        provider,
        events,
        reload.controller(),
        Instant::now(),
    )
}

pub(crate) fn reload_status(
    config: &AssetConfig,
    reload: &AssetReloadService,
) -> AssetReloadStatus {
    reload.status(config, Instant::now())
}

pub(crate) fn manifest(registry: &LocalManifestRegistry) -> AssetRegistryManifest {
    manifest_snapshot(registry)
}

pub(crate) fn resolve_path(
    config: &AssetConfig,
    registry: &LocalManifestRegistry,
    path: &Path,
) -> Option<AssetId> {
    lookup_source_asset(config, registry, path)
}

pub(crate) fn source(registry: &LocalManifestRegistry, id: AssetId) -> Option<PathBuf> {
    source_path(registry, id)
}

pub(crate) fn asset_metadata(
    registry: &LocalManifestRegistry,
    id: AssetId,
) -> Option<AssetMetadata> {
    metadata(registry, id)
}

pub(crate) fn asset_watch_paths(
    config: &AssetConfig,
    registry: &LocalManifestRegistry,
    id: AssetId,
) -> Option<AssetWatchPaths> {
    watch_paths(config, registry, id)
}

pub(crate) fn typed_path<T: Asset>(
    registry: &LocalManifestRegistry,
    factories: &AssetFactories,
    id: AssetId,
) -> Result<AssetPath<T>, AssetError> {
    typed_asset_path::<T, _>(registry, factories, id)
}
