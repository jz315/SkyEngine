use crate::asset::events::AssetEventLog;
use crate::asset::lease::*;
use crate::asset::load::{AssetLoadQueue, AssetLoadTimingSample, CompletedLoad};
use crate::asset::registry::{AssetFactories, LocalManifestRegistry};
use crate::asset::request::AssetRequests;
use crate::asset::store::{AssetRecord, AssetStore};
use crate::asset::texture::TextureAssetFactory;
use crate::asset::types::{AssetEventKind, AssetManifestEntry, AssetRegistryManifest, AssetState};
use crate::asset::{Asset, AssetConfig, AssetError, AssetId, TextureAsset, ASSET_SYSTEM_VERSION};
use std::any::TypeId;
use std::path::Path;
use std::time::Instant;
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
