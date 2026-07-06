use std::any::TypeId;
use std::path::Path;
use std::sync::mpsc::Receiver;
use std::time::Instant;

use super::events::{self, AssetEventLog};
use super::font::FontAsset;
use super::load::AssetLoadQueue;
use super::provider::RawSourceRequest;
use super::registry::{
    resolve_typed_source_asset, validate_typed_asset_request, AssetFactories, AssetRegistry,
};
use super::request::AssetRequests;
use super::store::{AssetReleaseOutcome, AssetStore};
use super::texture::TextureAsset;
use super::types::{Asset, AssetConfig, AssetError, AssetId};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct AssetDependencyLeases {
    ids: Vec<AssetId>,
}

impl AssetDependencyLeases {
    pub(crate) fn new(ids: Vec<AssetId>) -> Self {
        let mut leases = Self::default();
        leases.extend_missing(&ids);
        leases
    }

    pub(crate) fn ids(&self) -> &[AssetId] {
        &self.ids
    }

    pub(crate) fn contains(&self, id: &AssetId) -> bool {
        self.ids.contains(id)
    }

    pub(crate) fn clear(&mut self) {
        self.ids.clear();
    }

    pub(crate) fn extend_missing(&mut self, ids: &[AssetId]) {
        for id in ids {
            if !self.ids.contains(id) {
                self.ids.push(*id);
            }
        }
    }

    pub(crate) fn into_vec(self) -> Vec<AssetId> {
        self.ids
    }
}

pub(crate) fn drain_handle_releases(
    release_rx: &Receiver<AssetId>,
    store: &mut AssetStore,
) -> AssetReleaseOutcome {
    let mut outcome = AssetReleaseOutcome::default();
    while let Ok(id) = release_rx.try_recv() {
        let release = store.release_direct_reference_and_schedule_unused(id);
        outcome
            .immediate_unloaded
            .extend(release.immediate_unloaded);
    }
    outcome
}

pub(crate) fn apply_handle_releases(
    release_rx: &Receiver<AssetId>,
    store: &mut AssetStore,
    load_queue: &mut AssetLoadQueue,
    events: &mut AssetEventLog,
) -> usize {
    let release = drain_handle_releases(release_rx, store);
    let canceled = load_queue.cancel_non_loading(store);
    events::push_release_events(events, store, release);
    canceled
}

pub(crate) fn enqueue_load_request(
    store: &AssetStore,
    requests: &mut AssetRequests,
    id: AssetId,
    requested_type: Option<TypeId>,
    priority: i32,
    queued_at: Instant,
) {
    requests.enqueue(
        id,
        store.load_generation(id).unwrap_or(0),
        requested_type,
        priority,
        queued_at,
    );
}

pub(crate) fn retain_direct_lease<F>(
    store: &mut AssetStore,
    id: AssetId,
    requested_type: Option<TypeId>,
    asset_type_for: F,
) -> Result<(), AssetError>
where
    F: FnOnce(AssetId) -> Option<String>,
{
    if store.retain_existing_direct(id, requested_type) {
        return Ok(());
    }

    let asset_type = asset_type_for(id).ok_or(AssetError::AssetNotFound { id })?;
    store.retain_direct_with(id, requested_type, || {
        super::store::AssetRecord::new(id, asset_type)
    });
    Ok(())
}

pub(crate) fn retain_existing_direct_lease(
    store: &mut AssetStore,
    id: AssetId,
    requested_type: Option<TypeId>,
) -> Result<(), AssetError> {
    if store.retain_existing_direct(id, requested_type) {
        Ok(())
    } else {
        Err(AssetError::AssetNotFound { id })
    }
}

pub(crate) fn acquire_direct_lease<F>(
    store: &mut AssetStore,
    requests: &mut AssetRequests,
    id: AssetId,
    requested_type: Option<TypeId>,
    priority: i32,
    queued_at: Instant,
    asset_type_for: F,
) -> Result<(), AssetError>
where
    F: FnOnce(AssetId) -> Option<String>,
{
    retain_direct_lease(store, id, requested_type, asset_type_for)?;
    enqueue_load_request(store, requests, id, requested_type, priority, queued_at);
    Ok(())
}

pub(crate) fn acquire_manifest_direct_lease(
    store: &mut AssetStore,
    requests: &mut AssetRequests,
    manifest: &impl AssetRegistry,
    id: AssetId,
    requested_type: Option<TypeId>,
    priority: i32,
    queued_at: Instant,
) -> Result<(), AssetError> {
    acquire_direct_lease(
        store,
        requests,
        id,
        requested_type,
        priority,
        queued_at,
        |id| manifest.asset_type(id),
    )
}

pub(crate) fn acquire_typed_manifest_direct_lease<T: Asset>(
    store: &mut AssetStore,
    requests: &mut AssetRequests,
    manifest: &impl AssetRegistry,
    factories: &AssetFactories,
    id: AssetId,
    priority: i32,
    queued_at: Instant,
) -> Result<(), AssetError> {
    validate_typed_asset_request::<T, _>(manifest, factories, id)?;
    acquire_manifest_direct_lease(
        store,
        requests,
        manifest,
        id,
        Some(TypeId::of::<T>()),
        priority,
        queued_at,
    )
}

pub(crate) fn acquire_typed_manifest_source_lease<T: Asset>(
    config: &AssetConfig,
    manifest: &impl AssetRegistry,
    factories: &AssetFactories,
    store: &mut AssetStore,
    requests: &mut AssetRequests,
    path: &Path,
    priority: i32,
    queued_at: Instant,
) -> Result<AssetId, AssetError> {
    let id = resolve_typed_source_asset::<T, _>(config, manifest, factories, path)?;
    acquire_manifest_direct_lease(
        store,
        requests,
        manifest,
        id,
        Some(TypeId::of::<T>()),
        priority,
        queued_at,
    )?;
    Ok(id)
}

pub(crate) fn acquire_texture_source_lease(
    config: &AssetConfig,
    manifest: &impl AssetRegistry,
    factories: &AssetFactories,
    store: &mut AssetStore,
    requests: &mut AssetRequests,
    path_or_key: &Path,
    priority: i32,
    queued_at: Instant,
) -> Result<AssetId, AssetError> {
    if let Some(id) = acquire_manifest_source_lease::<TextureAsset>(
        config,
        manifest,
        factories,
        store,
        requests,
        path_or_key,
        priority,
        queued_at,
    )? {
        return Ok(id);
    }

    factories.ensure_registered_product::<TextureAsset>()?;
    let request = RawSourceRequest::new(config, path_or_key);
    let id = store.retain_raw_texture_record(request.key, request.path);
    enqueue_load_request(
        store,
        requests,
        id,
        Some(TypeId::of::<TextureAsset>()),
        priority,
        queued_at,
    );
    Ok(id)
}

pub(crate) fn acquire_font_source_lease(
    config: &AssetConfig,
    manifest: &impl AssetRegistry,
    factories: &AssetFactories,
    store: &mut AssetStore,
    requests: &mut AssetRequests,
    path_or_key: &Path,
    priority: i32,
    queued_at: Instant,
) -> Result<AssetId, AssetError> {
    if let Some(id) = acquire_manifest_source_lease::<FontAsset>(
        config,
        manifest,
        factories,
        store,
        requests,
        path_or_key,
        priority,
        queued_at,
    )? {
        return Ok(id);
    }

    factories.ensure_registered_product::<FontAsset>()?;
    let request = RawSourceRequest::new(config, path_or_key);
    let id = store.retain_raw_font_record(request.key, request.path);
    enqueue_load_request(
        store,
        requests,
        id,
        Some(TypeId::of::<FontAsset>()),
        priority,
        queued_at,
    );
    Ok(id)
}

fn acquire_manifest_source_lease<T: Asset>(
    config: &AssetConfig,
    manifest: &impl AssetRegistry,
    factories: &AssetFactories,
    store: &mut AssetStore,
    requests: &mut AssetRequests,
    path_or_key: &Path,
    priority: i32,
    queued_at: Instant,
) -> Result<Option<AssetId>, AssetError> {
    let Some(id) = manifest.lookup_source_asset(config, path_or_key) else {
        return Ok(None);
    };
    let entry = manifest.entry(id).ok_or(AssetError::AssetNotFound { id })?;
    factories.validate_entry_product::<T>(id, entry)?;
    acquire_direct_lease(
        store,
        requests,
        id,
        Some(TypeId::of::<T>()),
        priority,
        queued_at,
        |id| manifest.asset_type(id),
    )?;
    Ok(Some(id))
}
