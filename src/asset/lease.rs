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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset::load::{AssetLoadTimingSample, CompletedLoad};
    use crate::asset::registry::LocalManifestRegistry;
    use crate::asset::store::AssetRecord;
    use crate::asset::texture::TextureAssetFactory;
    use crate::asset::types::{
        AssetEventKind, AssetManifestEntry, AssetRegistryManifest, AssetState,
    };
    use crate::asset::ASSET_SYSTEM_VERSION;
    use tempfile::tempdir;

    fn texture_entry(id: AssetId, source_path: &str) -> AssetManifestEntry {
        AssetManifestEntry {
            asset_id: id,
            asset_type: TextureAsset::TYPE.to_string(),
            importer: "texture.importer".to_string(),
            cooker: "texture.raw_rgba8".to_string(),
            version: 1,
            source_path: source_path.to_string(),
            cooked_path: "hero.texture".to_string(),
            dependencies: Vec::new(),
            import_settings: serde_json::Value::Null,
        }
    }

    #[test]
    fn dependency_leases_deduplicate_in_order() {
        let first = AssetId::new();
        let second = AssetId::new();
        let leases = AssetDependencyLeases::new(vec![first, second, first]);
        assert_eq!(leases.ids(), &[first, second]);
    }

    #[test]
    fn handle_release_drain_releases_direct_refs_and_reports_unloaded_records() {
        let id = AssetId::new();
        let (tx, rx) = std::sync::mpsc::channel();
        let mut store = AssetStore::default();
        let mut record = AssetRecord::new(id, "dummy".to_string());
        record.strong_ref_count = 1;
        record.state = AssetState::Loading;
        store.records.insert(id, record);

        tx.send(id).expect("release send");
        let outcome = drain_handle_releases(&rx, &mut store);

        assert_eq!(store.records[&id].strong_ref_count, 0);
        assert_eq!(store.records[&id].state, AssetState::Unloaded);
        assert_eq!(outcome.immediate_unloaded, vec![id]);
        assert!(drain_handle_releases(&rx, &mut store)
            .immediate_unloaded
            .is_empty());
    }

    #[test]
    fn apply_handle_releases_cancels_stale_loads_and_emits_unloaded_event() {
        let id = AssetId::new();
        let generation = 1;
        let (handle_tx, handle_rx) = std::sync::mpsc::channel();
        let (unblock_tx, unblock_rx) = std::sync::mpsc::channel();
        let mut store = AssetStore::default();
        let mut record = AssetRecord::new(id, "dummy".to_string());
        record.strong_ref_count = 1;
        record.load_generation = generation;
        record.state = AssetState::Loading;
        store.records.insert(id, record);
        let mut load_queue = AssetLoadQueue::new(1, 4);
        let mut events = AssetEventLog::new(8);
        let mut cursor = events.cursor();
        let entry = AssetManifestEntry {
            asset_id: id,
            asset_type: "dummy".to_string(),
            importer: "dummy.importer".to_string(),
            cooker: "dummy.cooker".to_string(),
            version: 1,
            source_path: "dummy.source".to_string(),
            cooked_path: "dummy.cooked".to_string(),
            dependencies: Vec::new(),
            import_settings: serde_json::Value::Null,
        };

        load_queue
            .submit(id, generation, 0, move || {
                unblock_rx.recv().expect("test should unblock worker");
                CompletedLoad {
                    id,
                    generation,
                    entry,
                    cooked_hash: None,
                    timings: AssetLoadTimingSample::default(),
                    result: Err(AssetError::AssetNotFound { id }),
                }
            })
            .expect("submit should succeed");
        assert_eq!(load_queue.inflight_len(), 1);
        handle_tx.send(id).expect("release should send");

        let canceled = apply_handle_releases(&handle_rx, &mut store, &mut load_queue, &mut events);

        assert_eq!(canceled, 1);
        assert_eq!(load_queue.inflight_len(), 0);
        assert_eq!(store.records[&id].state, AssetState::Unloaded);
        let emitted = events.events_since(&mut cursor);
        assert_eq!(emitted.len(), 1);
        assert_eq!(emitted[0].id, id);
        assert_eq!(emitted[0].kind, AssetEventKind::Unloaded);
        unblock_tx.send(()).expect("worker should be released");
    }

    #[test]
    fn acquire_direct_lease_creates_manifest_record_and_enqueues_request() {
        let id = AssetId::new();
        let queued_at = Instant::now();
        let mut store = AssetStore::default();
        let mut requests = AssetRequests::default();

        acquire_direct_lease(
            &mut store,
            &mut requests,
            id,
            Some(TypeId::of::<usize>()),
            17,
            queued_at,
            |_| Some("dummy".to_string()),
        )
        .expect("direct lease should be acquired");

        let record = &store.records[&id];
        assert_eq!(record.strong_ref_count, 1);
        assert_eq!(record.requested_type, Some(TypeId::of::<usize>()));
        assert_eq!(requests.queued_len(), 1);
        assert_eq!(requests.submitted_count(), 1);
        let snapshot = requests.queued_snapshots(queued_at)[0].clone();
        assert_eq!(snapshot.asset_id, id);
        assert_eq!(snapshot.priority, 17);
        assert_eq!(snapshot.generation, 0);
    }

    #[test]
    fn acquire_manifest_direct_lease_uses_registry_asset_type() {
        let id = AssetId::new();
        let queued_at = Instant::now();
        let manifest = LocalManifestRegistry::new(AssetRegistryManifest {
            version: ASSET_SYSTEM_VERSION,
            target: "native".to_string(),
            provenance: Vec::new(),
            assets: vec![texture_entry(id, "textures/hero.png")],
        });
        let mut store = AssetStore::default();
        let mut requests = AssetRequests::default();

        acquire_manifest_direct_lease(
            &mut store,
            &mut requests,
            &manifest,
            id,
            Some(TypeId::of::<TextureAsset>()),
            19,
            queued_at,
        )
        .expect("manifest direct lease should resolve asset type");

        let record = &store.records[&id];
        assert_eq!(record.asset_type, TextureAsset::TYPE);
        assert_eq!(record.strong_ref_count, 1);
        assert_eq!(requests.queued_snapshots(queued_at)[0].priority, 19);
    }

    #[test]
    fn typed_manifest_lease_helpers_validate_resolve_retain_and_enqueue() {
        let id = AssetId::new();
        let queued_at = Instant::now();
        let manifest = LocalManifestRegistry::new(AssetRegistryManifest {
            version: ASSET_SYSTEM_VERSION,
            target: "native".to_string(),
            provenance: Vec::new(),
            assets: vec![texture_entry(id, "textures/hero.png")],
        });
        let config = AssetConfig::new("assets", "native");
        let mut factories = AssetFactories::default();
        factories.register(TextureAssetFactory);
        let mut store = AssetStore::default();
        let mut requests = AssetRequests::default();

        let resolved = acquire_typed_manifest_source_lease::<TextureAsset>(
            &config,
            &manifest,
            &factories,
            &mut store,
            &mut requests,
            Path::new("textures/hero.png"),
            23,
            queued_at,
        )
        .expect("typed manifest source lease should resolve");

        assert_eq!(resolved, id);
        assert_eq!(store.records[&id].strong_ref_count, 1);
        assert_eq!(
            store.records[&id].requested_type,
            Some(TypeId::of::<TextureAsset>())
        );
        assert_eq!(requests.queued_snapshots(queued_at)[0].priority, 23);

        acquire_typed_manifest_direct_lease::<TextureAsset>(
            &mut store,
            &mut requests,
            &manifest,
            &factories,
            id,
            29,
            queued_at,
        )
        .expect("typed manifest direct lease should validate");

        assert_eq!(store.records[&id].strong_ref_count, 2);
        assert_eq!(requests.queued_snapshots(queued_at)[1].priority, 29);
    }

    #[test]
    fn acquire_direct_lease_reuses_existing_generation_for_request() {
        let id = AssetId::new();
        let queued_at = Instant::now();
        let mut store = AssetStore::default();
        let mut record = AssetRecord::new(id, "dummy".to_string());
        record.load_generation = 9;
        store.records.insert(id, record);
        let mut requests = AssetRequests::default();

        acquire_direct_lease(&mut store, &mut requests, id, None, 3, queued_at, |_| None)
            .expect("existing record should not require manifest lookup");

        assert_eq!(store.records[&id].strong_ref_count, 1);
        assert_eq!(requests.queued_snapshots(queued_at)[0].generation, 9);
    }

    #[test]
    fn retain_direct_lease_reports_missing_manifest_record() {
        let id = AssetId::new();
        let mut store = AssetStore::default();
        let error = retain_direct_lease(&mut store, id, None, |_| None)
            .expect_err("missing manifest entry should fail");

        assert!(matches!(error, AssetError::AssetNotFound { id: missing } if missing == id));
        assert!(!store.records.contains_key(&id));
    }

    #[test]
    fn retain_existing_direct_lease_only_retains_known_records() {
        let existing = AssetId::new();
        let missing = AssetId::new();
        let mut store = AssetStore::default();
        store
            .records
            .insert(existing, AssetRecord::new(existing, "dummy".to_string()));

        retain_existing_direct_lease(&mut store, existing, Some(TypeId::of::<usize>()))
            .expect("existing direct lease should retain");
        assert_eq!(store.records[&existing].strong_ref_count, 1);
        assert_eq!(
            store.records[&existing].requested_type,
            Some(TypeId::of::<usize>())
        );

        let error = retain_existing_direct_lease(&mut store, missing, None)
            .expect_err("missing direct lease should fail");
        assert!(matches!(error, AssetError::AssetNotFound { id } if id == missing));
        assert!(!store.records.contains_key(&missing));
    }

    #[test]
    fn acquire_texture_source_lease_prefers_manifest_entry() {
        let dir = tempdir().expect("temporary asset root");
        let config = AssetConfig::new(dir.path(), "native");
        let id = AssetId::new();
        let manifest = LocalManifestRegistry::new(AssetRegistryManifest {
            version: ASSET_SYSTEM_VERSION,
            target: "native".to_string(),
            provenance: Vec::new(),
            assets: vec![texture_entry(id, "textures/hero.png")],
        });
        let mut factories = AssetFactories::default();
        factories.register(TextureAssetFactory);
        let mut store = AssetStore::default();
        let mut requests = AssetRequests::default();
        let queued_at = Instant::now();

        let acquired = acquire_texture_source_lease(
            &config,
            &manifest,
            &factories,
            &mut store,
            &mut requests,
            Path::new("textures/hero.png"),
            11,
            queued_at,
        )
        .expect("manifest texture should acquire");

        assert_eq!(acquired, id);
        assert_eq!(store.records[&id].strong_ref_count, 1);
        assert_eq!(requests.queued_snapshots(queued_at)[0].priority, 11);
        assert!(store.raw_source_path(id).is_none());
    }

    #[test]
    fn acquire_texture_source_lease_creates_raw_record_when_manifest_misses() {
        let dir = tempdir().expect("temporary asset root");
        let config = AssetConfig::new(dir.path(), "native");
        let manifest = LocalManifestRegistry::new(AssetRegistryManifest::default());
        let mut factories = AssetFactories::default();
        factories.register(TextureAssetFactory);
        let mut store = AssetStore::default();
        let mut requests = AssetRequests::default();
        let queued_at = Instant::now();

        let id = acquire_texture_source_lease(
            &config,
            &manifest,
            &factories,
            &mut store,
            &mut requests,
            Path::new("textures/raw.png"),
            -2,
            queued_at,
        )
        .expect("raw texture should acquire");

        assert_eq!(store.records[&id].strong_ref_count, 1);
        assert_eq!(
            store.raw_source_path(id),
            Some(dir.path().join("textures/raw.png"))
        );
        let snapshot = requests.queued_snapshots(queued_at)[0].clone();
        assert_eq!(snapshot.asset_id, id);
        assert_eq!(snapshot.priority, -2);
    }
}
