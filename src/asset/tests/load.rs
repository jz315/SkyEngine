use crate::asset::events::AssetEventLog;
use crate::asset::install::{AssetInstallContext, AssetInstallResult};
use crate::asset::load::*;
use crate::asset::provider::{
    raw_font_manifest_entry, raw_texture_manifest_entry, AssetSourceLocation, MemoryAssetProvider,
    ResolvedAssetSource,
};
use crate::asset::registry::{
    AssetFactories, AssetRuntimeFactory, ErasedAssetFactory, ManifestIndex,
};
use crate::asset::request::{AssetRequestPhase, AssetRequests};
use crate::asset::store::{AssetRecord, AssetStore};
use crate::asset::types::{
    AssetEventKind, AssetLoadContext, AssetManifestEntry, AssetRegistryManifest,
    AssetRequestStatus, AssetState, LoadedAsset, ASSET_SYSTEM_VERSION,
};
use crate::asset::{
    Asset, AssetConfig, AssetError, AssetFailurePhase, AssetId, FontAsset, TextureAsset,
};
use std::any::{Any, TypeId};
use std::path::Path;
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};
struct TestFactory;
impl ErasedAssetFactory for TestFactory {
    fn asset_type(&self) -> &'static str {
        "dummy"
    }
    fn product_type_id(&self) -> TypeId {
        TypeId::of::<String>()
    }
    fn load(
        &self,
        ctx: AssetLoadContext<'_>,
    ) -> Result<LoadedAsset<Arc<dyn Any + Send + Sync>>, AssetError> {
        Ok(LoadedAsset::new(Arc::new(
            String::from_utf8(ctx.bytes.to_vec()).expect("valid test utf8"),
        ) as Arc<dyn Any + Send + Sync>)
        .with_dependencies(ctx.entry.dependencies.clone()))
    }
    fn begin_install(
        &self,
        _loaded: &Arc<dyn Any + Send + Sync>,
        _ctx: crate::asset::install::AssetInstallContext<'_>,
    ) -> Result<crate::asset::install::AssetInstallResult<Arc<dyn Any + Send + Sync>>, AssetError>
    {
        unreachable!("load helper tests do not install")
    }
}
fn test_entry(id: AssetId) -> AssetManifestEntry {
    AssetManifestEntry {
        asset_id: id,
        asset_type: "dummy".to_string(),
        importer: "dummy.importer".to_string(),
        cooker: "dummy.cooker".to_string(),
        version: 1,
        source_path: format!("{id}.dummy"),
        cooked_path: format!("{id}.dummyc"),
        dependencies: Vec::new(),
        import_settings: serde_json::Value::Null,
    }
}
struct RuntimeTestAsset;
impl Asset for RuntimeTestAsset {
    const TYPE: &'static str = "dummy";
}
struct RuntimeTestFactory {
    dependencies: Vec<AssetId>,
}
impl AssetRuntimeFactory for RuntimeTestFactory {
    type Asset = RuntimeTestAsset;
    type Loaded = String;
    fn load(&self, ctx: AssetLoadContext<'_>) -> Result<LoadedAsset<Self::Loaded>, AssetError> {
        let payload =
            std::str::from_utf8(ctx.bytes).map_err(|error| AssetError::InvalidCookedAsset {
                id: Some(ctx.asset_id),
                message: error.to_string(),
            })?;
        Ok(LoadedAsset::new(payload.to_string()).with_dependencies(self.dependencies.clone()))
    }
    fn begin_install(
        &self,
        _loaded: &Self::Loaded,
        _ctx: AssetInstallContext<'_>,
    ) -> Result<AssetInstallResult<Self::Asset>, AssetError> {
        unreachable!("load record tests do not install")
    }
}
#[test]
fn raw_manifest_entry_helpers_describe_raw_texture_and_font_sources() {
    let texture = raw_texture_manifest_entry(
        AssetId::new(),
        "sprite.png".to_string(),
        serde_json::json!({ "srgb": true }),
    );
    assert_eq!(texture.asset_type, TextureAsset::TYPE);
    assert_eq!(texture.importer, "texture.raw");
    assert_eq!(texture.cooker, "texture.raw_rgba8");
    assert_eq!(texture.source_path, "sprite.png");
    let font = raw_font_manifest_entry(AssetId::new(), "font.ttf".to_string());
    assert_eq!(font.asset_type, FontAsset::TYPE);
    assert_eq!(font.importer, "font.raw");
    assert_eq!(font.cooker, "font.raw_bytes");
    assert_eq!(font.source_path, "font.ttf");
}
#[test]
fn fingerprint_and_hash_helpers_are_stable_and_sensitive_to_manifest_content() {
    let id = AssetId::new();
    let first = test_entry(id);
    let mut second = first.clone();
    second.version += 1;
    assert_eq!(hash_bytes(b"abc"), hash_bytes(b"abc"));
    assert_ne!(hash_bytes(b"abc"), hash_bytes(b"abcd"));
    assert_eq!(
        manifest_entry_fingerprint(&first).expect("fingerprint"),
        manifest_entry_fingerprint(&first).expect("fingerprint")
    );
    assert_ne!(
        manifest_entry_fingerprint(&first).expect("fingerprint"),
        manifest_entry_fingerprint(&second).expect("fingerprint")
    );
}
#[test]
fn prepare_loaded_asset_uses_loaded_dependencies_or_manifest_fallback() {
    let id = AssetId::new();
    let manifest_dependency = AssetId::new();
    let loaded_dependency = AssetId::new();
    let mut entry = test_entry(id);
    entry.dependencies = vec![manifest_dependency, manifest_dependency];
    let manifest_fallback = prepare_loaded_asset(
        &entry,
        LoadedAsset::new(Arc::new("manifest".to_string()) as Arc<dyn Any + Send + Sync>),
        Some("hash-a".to_string()),
    )
    .expect("manifest fallback should prepare");
    assert_eq!(manifest_fallback.dependencies, vec![manifest_dependency]);
    assert_eq!(manifest_fallback.content_hash.as_deref(), Some("hash-a"));
    assert_eq!(
        manifest_fallback
            .loaded
            .downcast_ref::<String>()
            .expect("prepared payload should stay intact"),
        "manifest"
    );
    let loaded_override = prepare_loaded_asset(
        &entry,
        LoadedAsset::new(Arc::new("loaded".to_string()) as Arc<dyn Any + Send + Sync>)
            .with_dependencies(vec![
                loaded_dependency,
                manifest_dependency,
                loaded_dependency,
            ]),
        Some("hash-b".to_string()),
    )
    .expect("loaded dependency override should prepare");
    assert_eq!(
        loaded_override.dependencies,
        vec![loaded_dependency, manifest_dependency]
    );
    assert_eq!(loaded_override.content_hash.as_deref(), Some("hash-b"));
}
#[test]
fn resolved_memory_source_loads_through_factory_and_reports_content_hash() {
    let id = AssetId::new();
    let dependency = AssetId::new();
    let mut entry = test_entry(id);
    entry.dependencies = vec![dependency];
    let source = ResolvedAssetSource::new(
        entry,
        AssetSourceLocation::Memory {
            label: "memory://dummy".to_string(),
            bytes: Arc::from(Vec::from(&b"payload"[..])),
            read_error: None,
            read_delay: None,
        },
    );
    let loaded_source =
        load_resolved_source_asset(id, &source, &TestFactory, Path::new("."), Path::new("."))
            .expect("memory source should load");
    let payload = loaded_source
        .loaded
        .loaded
        .downcast_ref::<String>()
        .expect("loaded payload should be a string");
    assert_eq!(payload, "payload");
    assert_eq!(loaded_source.loaded.dependencies, vec![dependency]);
    assert_eq!(loaded_source.content_hash, hash_bytes(b"payload"));
    assert!(loaded_source.timings.total_time >= loaded_source.timings.read_time);
}
#[test]
fn resolved_source_load_marks_decode_phase_for_diagnostics() {
    let id = AssetId::new();
    let source = ResolvedAssetSource::new(
        test_entry(id),
        AssetSourceLocation::Memory {
            label: "memory://dummy".to_string(),
            bytes: Arc::from(Vec::from(&b"payload"[..])),
            read_error: None,
            read_delay: None,
        },
    );
    let phase = AssetLoadPhaseTracker::new();
    let _loaded_source = load_resolved_source_asset_with_phase(
        id,
        &source,
        &TestFactory,
        Path::new("."),
        Path::new("."),
        &phase,
    )
    .expect("memory source should load");
    assert_eq!(phase.phase(), AssetSourceLoadPhase::Decoding);
}
#[test]
fn load_record_now_reports_memory_source_read_failure_without_filesystem() {
    let id = AssetId::new();
    let entry = test_entry(id);
    let config = AssetConfig::new("memory-root", "native");
    let manifest = ManifestIndex::new(AssetRegistryManifest {
        version: ASSET_SYSTEM_VERSION,
        target: "native".to_string(),
        provenance: Vec::new(),
        assets: vec![entry],
    });
    let read_error = AssetError::Io {
        path: Path::new("memory://broken").to_path_buf(),
        message: "simulated read failure".to_string(),
    };
    let provider = MemoryAssetProvider::new().with_read_error(id, read_error.clone());
    let mut factories = AssetFactories::default();
    factories.register(RuntimeTestFactory {
        dependencies: Vec::new(),
    });
    let mut store = AssetStore::default();
    store
        .records
        .insert(id, AssetRecord::new(id, RuntimeTestAsset::TYPE.to_string()));
    let failure = match load_record_now(&config, &provider, &manifest, &factories, &mut store, id) {
        Ok(_) => panic!("memory read failure should fail before factory decode"),
        Err(failure) => failure,
    };
    assert_eq!(failure.error, read_error);
    assert_eq!(
        AssetFailurePhase::from_error(&failure.error),
        AssetFailurePhase::Read
    );
    assert!(failure.timings.total_time >= failure.timings.read_time);
    assert_eq!(store.records[&id].state, AssetState::Unloaded);
}
#[test]
fn load_record_now_resolves_source_factory_and_updates_store() {
    let id = AssetId::new();
    let first_dependency = AssetId::new();
    let second_dependency = AssetId::new();
    let entry = test_entry(id);
    let config = AssetConfig::new("memory-root", "native");
    let manifest = ManifestIndex::new(AssetRegistryManifest {
        version: ASSET_SYSTEM_VERSION,
        target: "native".to_string(),
        provenance: Vec::new(),
        assets: vec![entry.clone()],
    });
    let provider = MemoryAssetProvider::new().with_asset(id, b"payload".to_vec());
    let mut factories = AssetFactories::default();
    factories.register(RuntimeTestFactory {
        dependencies: vec![first_dependency, second_dependency, first_dependency],
    });
    let mut store = AssetStore::default();
    store
        .records
        .insert(id, AssetRecord::new(id, RuntimeTestAsset::TYPE.to_string()));
    let loaded = load_record_now(&config, &provider, &manifest, &factories, &mut store, id)
        .expect("record should load from memory provider");
    assert_eq!(
        loaded.dependency_update.new_dependency_leases,
        vec![first_dependency, second_dependency]
    );
    assert!(loaded.timings.total_time >= loaded.timings.read_time);
    let record = store.records.get(&id).expect("loaded record exists");
    assert_eq!(record.state, AssetState::Loaded);
    assert_eq!(
        record.dependencies,
        vec![first_dependency, second_dependency]
    );
    let expected_hash = hash_bytes(b"payload");
    assert_eq!(
        record.loaded_cooked_hash.as_deref(),
        Some(expected_hash.as_str())
    );
    assert_eq!(
        record
            .loaded
            .as_ref()
            .and_then(|loaded| loaded.downcast_ref::<String>())
            .map(String::as_str),
        Some("payload")
    );
}
#[test]
fn load_record_now_and_apply_records_timing_and_loaded_event() {
    let id = AssetId::new();
    let entry = test_entry(id);
    let config = AssetConfig::new("memory-root", "native");
    let manifest = ManifestIndex::new(AssetRegistryManifest {
        version: ASSET_SYSTEM_VERSION,
        target: "native".to_string(),
        provenance: Vec::new(),
        assets: vec![entry],
    });
    let provider = MemoryAssetProvider::new().with_asset(id, b"payload".to_vec());
    let mut factories = AssetFactories::default();
    factories.register(RuntimeTestFactory {
        dependencies: Vec::new(),
    });
    let mut store = AssetStore::default();
    store
        .records
        .insert(id, AssetRecord::new(id, RuntimeTestAsset::TYPE.to_string()));
    let mut events = AssetEventLog::default();
    let mut cursor = events.cursor();
    let mut load_queue = AssetLoadQueue::new(1, 4);
    load_record_now_and_apply(
        &config,
        &provider,
        &manifest,
        &factories,
        &mut store,
        &mut events,
        &mut load_queue,
        id,
    )
    .expect("record should load and apply from memory provider");
    let record = store.records.get(&id).expect("loaded record exists");
    assert_eq!(record.state, AssetState::Loaded);
    let emitted = events.events_since(&mut cursor);
    assert_eq!(emitted.len(), 1);
    assert_eq!(emitted[0].id, id);
    assert_eq!(emitted[0].kind, AssetEventKind::Loaded);
    let stats = load_queue.timing_stats();
    assert_eq!(stats.completed_source_loads, 1);
    assert_eq!(stats.failed_source_loads, 0);
}
#[test]
fn load_policy_prefers_background_for_global_texture_or_raw_records() {
    let plain = AssetId::new();
    let texture = AssetId::new();
    let raw = AssetId::new();
    let mut store = AssetStore::default();
    store.records.insert(
        plain,
        AssetRecord::new(plain, RuntimeTestAsset::TYPE.to_string()),
    );
    store.records.insert(
        texture,
        AssetRecord::new(texture, TextureAsset::TYPE.to_string()),
    );
    let mut raw_record = AssetRecord::new(raw, RuntimeTestAsset::TYPE.to_string());
    raw_record.raw_source_path = Some(std::path::Path::new("raw.asset").to_path_buf());
    store.records.insert(raw, raw_record);
    let foreground = AssetConfig::new("memory-root", "native");
    assert!(!should_load_record_in_background(
        &foreground,
        &store,
        plain
    ));
    assert!(should_load_record_in_background(
        &foreground,
        &store,
        texture
    ));
    assert!(should_load_record_in_background(&foreground, &store, raw));
    let background = foreground.with_background_loading(true);
    assert!(should_load_record_in_background(&background, &store, plain));
    assert!(should_load_record_in_background(
        &background,
        &store,
        AssetId::new()
    ));
}
#[test]
fn load_policy_detects_current_inflight_generation() {
    let id = AssetId::new();
    let mut store = AssetStore::default();
    let mut record = AssetRecord::new(id, RuntimeTestAsset::TYPE.to_string());
    record.state = AssetState::Loading;
    record.load_generation = 3;
    store.records.insert(id, record);
    let mut load_queue = AssetLoadQueue::new(1, 2);
    assert!(!has_current_inflight_load(&store, &load_queue, id));
    assert!(load_queue
        .submit(id, 3, 0, move || completion(id, 3))
        .expect("submit should succeed"));
    assert!(has_current_inflight_load(&store, &load_queue, id));
    store
        .records
        .get_mut(&id)
        .expect("record should exist")
        .load_generation = 4;
    assert!(!has_current_inflight_load(&store, &load_queue, id));
}
#[test]
fn load_record_now_and_apply_or_fail_records_failed_event_and_request() {
    let id = AssetId::new();
    let now = Instant::now();
    let entry = test_entry(id);
    let config = AssetConfig::new("memory-root", "native");
    let manifest = ManifestIndex::new(AssetRegistryManifest {
        version: ASSET_SYSTEM_VERSION,
        target: "native".to_string(),
        provenance: Vec::new(),
        assets: vec![entry],
    });
    let read_error = AssetError::Io {
        path: Path::new("memory://broken").to_path_buf(),
        message: "simulated read failure".to_string(),
    };
    let provider = MemoryAssetProvider::new().with_read_error(id, read_error.clone());
    let mut factories = AssetFactories::default();
    factories.register(RuntimeTestFactory {
        dependencies: Vec::new(),
    });
    let mut store = AssetStore::default();
    let mut record = AssetRecord::new(id, RuntimeTestAsset::TYPE.to_string());
    record.state = AssetState::Loading;
    record.strong_ref_count = 1;
    record.load_generation = 4;
    store.records.insert(id, record);
    let mut events = AssetEventLog::default();
    let mut cursor = events.cursor();
    let mut requests = AssetRequests::default();
    requests.enqueue(id, 4, None, 2, now);
    let request = requests.pop_queued().expect("queued request");
    requests.activate(request, AssetRequestPhase::Loading, 4, now);
    let mut load_queue = AssetLoadQueue::new(1, 4);
    let error = load_record_now_and_apply_or_fail(
        &config,
        &provider,
        &manifest,
        &factories,
        &mut store,
        &mut events,
        &mut requests,
        &mut load_queue,
        id,
        now,
    )
    .expect_err("read failure should enter failure lifecycle");
    assert_eq!(error, read_error);
    assert_eq!(store.records[&id].state, AssetState::Failed);
    assert_eq!(
        store.records[&id].failure_phase,
        Some(AssetFailurePhase::Read)
    );
    let emitted = events.events_since(&mut cursor);
    assert_eq!(emitted.len(), 1);
    assert_eq!(emitted[0].id, id);
    assert_eq!(emitted[0].kind, AssetEventKind::Failed);
    assert_eq!(emitted[0].failure_phase, Some(AssetFailurePhase::Read));
    let failed = requests.failed_snapshots();
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].asset_id, id);
    assert_eq!(failed[0].generation, 4);
    assert_eq!(failed[0].status, AssetRequestStatus::Failed);
    let stats = load_queue.timing_stats();
    assert_eq!(stats.completed_source_loads, 0);
    assert_eq!(stats.failed_source_loads, 1);
}
#[test]
fn submit_record_load_or_fail_records_pre_worker_failure() {
    let id = AssetId::new();
    let now = Instant::now();
    let entry = test_entry(id);
    let config = AssetConfig::new("memory-root", "native");
    let manifest = ManifestIndex::new(AssetRegistryManifest {
        version: ASSET_SYSTEM_VERSION,
        target: "native".to_string(),
        provenance: Vec::new(),
        assets: vec![entry],
    });
    let provider = MemoryAssetProvider::new().with_asset(id, b"payload".to_vec());
    let factories = AssetFactories::default();
    let mut store = AssetStore::default();
    let mut record = AssetRecord::new(id, RuntimeTestAsset::TYPE.to_string());
    record.state = AssetState::Loading;
    record.strong_ref_count = 1;
    record.load_generation = 6;
    store.records.insert(id, record);
    let mut events = AssetEventLog::default();
    let mut cursor = events.cursor();
    let mut requests = AssetRequests::default();
    requests.enqueue(id, 6, None, 2, now);
    let request = requests.pop_queued().expect("queued request");
    requests.activate(request, AssetRequestPhase::Loading, 6, now);
    let mut load_queue = AssetLoadQueue::new(1, 4);
    let error = submit_record_load_or_fail(
        &config,
        &provider,
        &manifest,
        &factories,
        &mut store,
        &mut events,
        &mut requests,
        &mut load_queue,
        id,
        now,
    )
    .expect_err("missing factory should fail before worker submission");
    assert!(
        matches!(            error,            AssetError::FactoryNotRegistered { asset_type }                if asset_type == RuntimeTestAsset::TYPE        )
    );
    assert_eq!(store.records[&id].state, AssetState::Failed);
    assert_eq!(
        store.records[&id].failure_phase,
        Some(AssetFailurePhase::Lookup)
    );
    assert_eq!(load_queue.inflight_len(), 0);
    let emitted = events.events_since(&mut cursor);
    assert_eq!(emitted.len(), 1);
    assert_eq!(emitted[0].kind, AssetEventKind::Failed);
    let failed = requests.failed_snapshots();
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].asset_id, id);
    assert_eq!(failed[0].status, AssetRequestStatus::Failed);
}
#[test]
fn finish_completed_load_applies_matching_completion_and_reports_failures() {
    let id = AssetId::new();
    let dependency = AssetId::new();
    let mut entry = test_entry(id);
    entry.dependencies = vec![dependency];
    let mut store = AssetStore::default();
    let mut record = AssetRecord::new(id, "dummy".to_string());
    record.state = AssetState::Loading;
    record.load_generation = 2;
    store.records.insert(id, record);
    let update = finish_completed_load(
        &mut store,
        CompletedLoad {
            id,
            generation: 2,
            entry: entry.clone(),
            cooked_hash: Some("hash".to_string()),
            timings: AssetLoadTimingSample::default(),
            result: Ok(LoadedAsset::new(
                Arc::new("payload".to_string()) as Arc<dyn Any + Send + Sync>
            )),
        },
    )
    .expect("successful completion should not fail")
    .expect("matching completion should apply");
    assert_eq!(update.new_dependency_leases, vec![dependency]);
    let record = store.records.get(&id).expect("record exists");
    assert_eq!(record.state, AssetState::Loaded);
    assert_eq!(record.dependencies, vec![dependency]);
    let mut failed_store = AssetStore::default();
    let mut failed_record = AssetRecord::new(id, "dummy".to_string());
    failed_record.state = AssetState::Loading;
    failed_record.load_generation = 3;
    failed_store.records.insert(id, failed_record);
    let failure = match finish_completed_load(
        &mut failed_store,
        CompletedLoad {
            id,
            generation: 3,
            entry,
            cooked_hash: None,
            timings: AssetLoadTimingSample::default(),
            result: Err(AssetError::InvalidCookedAsset {
                id: Some(id),
                message: "bad".to_string(),
            }),
        },
    ) {
        Err(failure) => failure,
        Ok(_) => panic!("failed completion should report failure context"),
    };
    assert_eq!(failure.id, id);
    assert_eq!(failure.phase, AssetFailurePhase::Decode);
}
#[test]
fn apply_completed_load_emits_loaded_and_retains_dependencies() {
    let id = AssetId::new();
    let dependency = AssetId::new();
    let mut entry = test_entry(id);
    entry.dependencies = vec![dependency];
    let dependency_entry = test_entry(dependency);
    let manifest = ManifestIndex::new(AssetRegistryManifest {
        version: ASSET_SYSTEM_VERSION,
        target: "native".to_string(),
        provenance: Vec::new(),
        assets: vec![entry.clone(), dependency_entry],
    });
    let mut store = AssetStore::default();
    let mut record = AssetRecord::new(id, "dummy".to_string());
    record.state = AssetState::Loading;
    record.load_generation = 2;
    store.records.insert(id, record);
    let mut events = AssetEventLog::default();
    let mut cursor = events.cursor();
    let mut load_queue = AssetLoadQueue::new(1, 4);
    let applied = apply_completed_load(
        &mut store,
        &mut events,
        &mut load_queue,
        &manifest,
        CompletedLoad {
            id,
            generation: 2,
            entry,
            cooked_hash: Some("hash".to_string()),
            timings: AssetLoadTimingSample::default(),
            result: Ok(LoadedAsset::new(
                Arc::new("payload".to_string()) as Arc<dyn Any + Send + Sync>
            )),
        },
        9,
    )
    .expect("successful completion should apply");
    assert!(applied);
    assert_eq!(store.records[&id].state, AssetState::Loaded);
    assert_eq!(store.records[&dependency].dependency_ref_count, 1);
    assert_eq!(store.records[&dependency].state, AssetState::Loading);
    assert_eq!(store.records[&dependency].load_priority, 9);
    let emitted = events.events_since(&mut cursor);
    assert_eq!(emitted.len(), 1);
    assert_eq!(emitted[0].id, id);
    assert_eq!(emitted[0].kind, AssetEventKind::Loaded);
}
#[test]
fn drain_completed_loads_or_fail_records_completion_failure() {
    let id = AssetId::new();
    let generation = 8;
    let now = Instant::now();
    let entry = test_entry(id);
    let manifest = ManifestIndex::new(AssetRegistryManifest {
        version: ASSET_SYSTEM_VERSION,
        target: "native".to_string(),
        provenance: Vec::new(),
        assets: vec![entry.clone()],
    });
    let mut store = AssetStore::default();
    let mut record = AssetRecord::new(id, RuntimeTestAsset::TYPE.to_string());
    record.state = AssetState::Loading;
    record.strong_ref_count = 1;
    record.load_generation = generation;
    store.records.insert(id, record);
    let mut events = AssetEventLog::default();
    let mut cursor = events.cursor();
    let mut requests = AssetRequests::default();
    requests.enqueue(id, generation, None, 7, now);
    let request = requests.pop_queued().expect("queued request");
    requests.activate(request, AssetRequestPhase::Loading, generation, now);
    let mut load_queue = AssetLoadQueue::new(1, 4);
    load_queue
        .submit(id, generation, 7, move || CompletedLoad {
            id,
            generation,
            entry,
            cooked_hash: None,
            timings: AssetLoadTimingSample::default(),
            result: Err(AssetError::InvalidCookedAsset {
                id: Some(id),
                message: "bad payload".to_string(),
            }),
        })
        .expect("submit should succeed");
    let started = Instant::now();
    let error = loop {
        match drain_completed_loads_or_fail(
            &mut store,
            &mut events,
            &mut requests,
            &mut load_queue,
            &manifest,
            7,
            now,
        ) {
            Err(error) => break error,
            Ok(0) => {
                assert!(started.elapsed() < Duration::from_secs(1));
                std::thread::sleep(Duration::from_millis(1));
            }
            Ok(completed) => panic!("failed completion should not apply {completed} loads"),
        }
    };
    assert!(
        matches!(            error,            AssetError::InvalidCookedAsset { id: Some(failed), .. } if failed == id        )
    );
    assert_eq!(store.records[&id].state, AssetState::Failed);
    assert_eq!(
        store.records[&id].failure_phase,
        Some(AssetFailurePhase::Decode)
    );
    let emitted = events.events_since(&mut cursor);
    assert_eq!(emitted.len(), 1);
    assert_eq!(emitted[0].kind, AssetEventKind::Failed);
    assert_eq!(emitted[0].failure_phase, Some(AssetFailurePhase::Decode));
    let failed = requests.failed_snapshots();
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].asset_id, id);
    assert_eq!(failed[0].generation, generation);
    assert_eq!(failed[0].status, AssetRequestStatus::Failed);
}
fn completion(id: AssetId, generation: u64) -> CompletedLoad {
    CompletedLoad {
        id,
        generation,
        entry: test_entry(id),
        cooked_hash: None,
        timings: AssetLoadTimingSample {
            sampled: true,
            read_time: Duration::from_millis(2),
            decode_time: Duration::from_millis(3),
            total_time: Duration::from_millis(7),
        },
        result: Err(AssetError::Internal {
            message: "test completion".to_string(),
        }),
    }
}
#[test]
fn load_queue_tracks_inflight_and_drains_completed_loads() {
    let id = AssetId::new();
    let mut queue = AssetLoadQueue::new(1, 4);
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    assert!(queue
        .submit(id, 3, 0, move || {
            started_tx.send(()).expect("started receiver alive");
            release_rx.recv().expect("release sender alive");
            completion(id, 3)
        })
        .expect("submit should succeed"));
    started_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("worker should start running job");
    assert!(queue.contains(id, 3));
    assert_eq!(queue.inflight_len(), 1);
    assert_eq!(queue.running_len(), 1);
    assert_eq!(queue.queued_len(), 0);
    release_tx.send(()).expect("worker should still wait");
    let started = Instant::now();
    let completions = loop {
        let completions = queue.drain_ready();
        if !completions.is_empty() {
            break completions;
        }
        assert!(started.elapsed() < Duration::from_secs(1));
        std::thread::sleep(Duration::from_millis(1));
    };
    assert_eq!(completions.len(), 1);
    assert_eq!(completions[0].id, id);
    assert_eq!(completions[0].generation, 3);
    assert!(!queue.contains(id, 3));
    assert_eq!(queue.inflight_len(), 0);
    assert_eq!(queue.running_len(), 0);
    let stats = queue.timing_stats();
    assert_eq!(stats.failed_source_loads, 1);
    assert_eq!(stats.completed_source_loads, 0);
    assert_eq!(stats.average_read_time, Some(Duration::from_millis(2)));
    assert_eq!(stats.average_decode_time, Some(Duration::from_millis(3)));
    assert_eq!(stats.average_total_time, Some(Duration::from_millis(7)));
}
#[test]
fn load_queue_reports_active_source_load_phase() {
    let id = AssetId::new();
    let mut queue = AssetLoadQueue::new(1, 4);
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    assert!(queue
        .submit_with_phase(id, 3, 0, move |phase| {
            phase.set(AssetSourceLoadPhase::Decoding);
            started_tx.send(()).expect("started receiver alive");
            release_rx.recv().expect("release sender alive");
            completion(id, 3)
        })
        .expect("submit should succeed"));
    started_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("worker should enter decode phase");
    assert_eq!(queue.phase(id, 3), Some(AssetSourceLoadPhase::Decoding));
    let phases = queue.source_load_phase_counts();
    assert_eq!(phases.decoding, 1);
    assert_eq!(phases.reading, 0);
    assert_eq!(phases.queued, 0);
    release_tx.send(()).expect("worker should still wait");
}
#[test]
fn load_queue_reports_oldest_queued_worker_age() {
    let first = AssetId::new();
    let second = AssetId::new();
    let mut queue = AssetLoadQueue::new(1, 4);
    let (release_tx, release_rx) = mpsc::channel();
    assert!(queue
        .submit(first, 1, 0, move || {
            release_rx.recv().expect("release sender alive");
            completion(first, 1)
        })
        .expect("first submit should succeed"));
    let started = Instant::now();
    while queue.running_len() == 0 {
        assert!(started.elapsed() < Duration::from_secs(1));
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(queue
        .submit(second, 1, 0, move || completion(second, 1))
        .expect("second submit should succeed"));
    assert_eq!(queue.queued_len(), 1);
    assert!(queue.oldest_queued_age(Instant::now()).is_some());
    let phases = queue.source_load_phase_counts();
    assert_eq!(phases.queued, 1);
    assert_eq!(phases.reading, 1);
    assert_eq!(phases.decoding, 0);
    release_tx.send(()).expect("worker should still wait");
}
#[test]
fn load_queue_running_len_does_not_count_completed_but_undrained_loads() {
    let id = AssetId::new();
    let mut queue = AssetLoadQueue::new(1, 4);
    assert!(queue
        .submit(id, 1, 0, move || completion(id, 1))
        .expect("submit should succeed"));
    let started = Instant::now();
    loop {
        if queue.running_len() == 0 && queue.inflight_len() == 1 && queue.queued_len() == 0 {
            break;
        }
        assert!(started.elapsed() < Duration::from_secs(1));
        std::thread::sleep(Duration::from_millis(1));
    }
    let completions = queue.drain_ready();
    assert_eq!(completions.len(), 1);
    assert_eq!(queue.inflight_len(), 0);
}
#[test]
fn load_queue_prevents_duplicate_inflight_submission() {
    let id = AssetId::new();
    let mut queue = AssetLoadQueue::new(1, 4);
    let (release_tx, release_rx) = mpsc::channel();
    assert!(queue
        .submit(id, 1, 0, move || {
            release_rx.recv().expect("release sender alive");
            completion(id, 1)
        })
        .expect("first submit should succeed"));
    assert!(!queue
        .submit(id, 1, 0, move || completion(id, 1))
        .expect("duplicate submit should be ignored"));
    release_tx.send(()).expect("worker should still wait");
}
#[test]
fn load_queue_clears_inflight_when_worker_queue_is_full() {
    let mut queue = AssetLoadQueue::new(1, 1);
    let running = AssetId::new();
    let queued = AssetId::new();
    let rejected = AssetId::new();
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    assert!(queue
        .submit(running, 1, 0, move || {
            started_tx.send(()).expect("started receiver alive");
            release_rx.recv().expect("release sender alive");
            completion(running, 1)
        })
        .expect("running submit should succeed"));
    started_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("worker should start running job");
    assert!(queue
        .submit(queued, 1, 0, move || completion(queued, 1))
        .expect("queued submit should succeed"));
    assert_eq!(queue.running_len(), 1);
    assert_eq!(queue.queued_len(), 1);
    assert!(!queue
        .submit(rejected, 1, 0, move || completion(rejected, 1))
        .expect("full queue should defer submission"));
    assert!(!queue.contains(rejected, 1));
    assert_eq!(queue.deferred_submission_count(), 1);
    release_tx.send(()).expect("worker should still wait");
}
#[test]
fn load_queue_reports_uncanceled_worker_queue_depth() {
    let mut queue = AssetLoadQueue::new(1, 2);
    let running = AssetId::new();
    let queued = AssetId::new();
    let canceled = AssetId::new();
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    assert!(queue
        .submit(running, 1, 0, move || {
            started_tx.send(()).expect("started receiver alive");
            release_rx.recv().expect("release sender alive");
            completion(running, 1)
        })
        .expect("running submit should succeed"));
    started_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("worker should start running job");
    assert!(queue
        .submit(queued, 1, 0, move || completion(queued, 1))
        .expect("queued submit should succeed"));
    assert!(queue
        .submit(canceled, 1, 0, move || completion(canceled, 1))
        .expect("cancelable submit should succeed"));
    assert_eq!(queue.queued_len(), 2);
    assert!(queue.cancel(canceled, 1));
    assert_eq!(queue.queued_len(), 1);
    release_tx.send(()).expect("worker should still wait");
}
#[test]
fn load_queue_can_submit_after_canceling_queued_job_at_capacity() {
    let mut queue = AssetLoadQueue::new(1, 1);
    let running = AssetId::new();
    let canceled = AssetId::new();
    let replacement = AssetId::new();
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    assert!(queue
        .submit(running, 1, 0, move || {
            started_tx.send(()).expect("started receiver alive");
            release_rx.recv().expect("release sender alive");
            completion(running, 1)
        })
        .expect("running submit should succeed"));
    started_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("worker should start running job");
    assert!(queue
        .submit(canceled, 1, 0, move || completion(canceled, 1))
        .expect("queued submit should succeed"));
    assert_eq!(queue.queued_len(), 1);
    assert!(queue.cancel(canceled, 1));
    assert_eq!(queue.queued_len(), 0);
    assert!(queue
        .submit(replacement, 1, 0, move || completion(replacement, 1))
        .expect("canceled queued load should free worker queue capacity"));
    release_tx.send(()).expect("worker should still wait");
}
#[test]
fn load_queue_cancels_queued_inflight_load_before_start() {
    let mut queue = AssetLoadQueue::new(1, 2);
    let running = AssetId::new();
    let canceled = AssetId::new();
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let (canceled_tx, canceled_rx) = mpsc::channel();
    assert!(queue
        .submit(running, 1, 0, move || {
            started_tx.send(()).expect("started receiver alive");
            release_rx.recv().expect("release sender alive");
            completion(running, 1)
        })
        .expect("running submit should succeed"));
    started_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("worker should start running job");
    assert!(queue
        .submit(canceled, 1, 0, move || {
            canceled_tx.send(()).expect("canceled receiver alive");
            completion(canceled, 1)
        })
        .expect("queued submit should succeed"));
    assert!(queue.cancel(canceled, 1));
    assert!(!queue.contains(canceled, 1));
    release_tx.send(()).expect("worker should still wait");
    let started = Instant::now();
    loop {
        let completions = queue.drain_ready();
        if completions
            .iter()
            .any(|completion| completion.id == running)
        {
            break;
        }
        assert!(started.elapsed() < Duration::from_secs(1));
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(canceled_rx.recv_timeout(Duration::from_millis(20)).is_err());
}
#[test]
fn load_queue_cancels_non_loading_records() {
    let id = AssetId::new();
    let mut queue = AssetLoadQueue::new(1, 2);
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let mut store = AssetStore::default();
    let mut record = AssetRecord::new(id, "dummy".to_string());
    record.state = AssetState::Loading;
    record.load_generation = 1;
    store.records.insert(id, record);
    assert!(queue
        .submit(id, 1, 0, move || {
            started_tx.send(()).expect("started receiver alive");
            release_rx.recv().expect("release sender alive");
            completion(id, 1)
        })
        .expect("submit should succeed"));
    started_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("worker should start running job");
    store.records.get_mut(&id).expect("record").state = AssetState::Unloaded;
    assert_eq!(queue.cancel_non_loading(&store), 1);
    assert_eq!(queue.inflight_len(), 0);
    release_tx.send(()).expect("worker should still wait");
}
