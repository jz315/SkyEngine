use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

use crate::asset::events::AssetEventLog;
use crate::asset::load::{hash_bytes, manifest_entry_fingerprint};
use crate::asset::provider::{AssetSourceLocation, MemoryAssetProvider, ResolvedAssetSource};
use crate::asset::registry::{AssetRegistryLoader, ManifestIndex};
use crate::asset::reload::*;
use crate::asset::request::AssetRequests;
use crate::asset::store::{AssetRecord, AssetStore};
use crate::asset::types::{
    AssetEventKind, AssetManifestEntry, AssetRegistryManifest, AssetReloadSkipReason, AssetState,
    ASSET_SYSTEM_VERSION,
};
use crate::asset::watcher::AssetWatchEvent;
use crate::asset::{AssetConfig, AssetError, AssetId, AssetReloadReport};

struct CountingRegistryLoader {
    manifest: AssetRegistryManifest,
    loads: Arc<AtomicUsize>,
}

impl AssetRegistryLoader for CountingRegistryLoader {
    fn load(&self, _config: &AssetConfig) -> Result<AssetRegistryManifest, AssetError> {
        self.loads.fetch_add(1, Ordering::SeqCst);
        Ok(self.manifest.clone())
    }
}

fn config() -> AssetConfig {
    AssetConfig::default()
        .with_auto_reload(true)
        .with_auto_reload_interval(Duration::from_secs(5))
        .with_auto_reload_debounce(Duration::from_millis(20))
}

#[test]
fn scan_interval_honors_watcher_override_and_frozen_state() {
    let mut reload = AssetReloadController::default();
    let config = config();
    let now = Instant::now();

    assert!(reload.interval_scan_due(&config, now));
    assert!(reload.should_scan(&config, now, false));
    assert!(!reload.should_scan(&config, now + Duration::from_secs(1), false));
    assert!(reload.should_scan(&config, now + Duration::from_secs(1), true));

    reload.set_frozen(true);
    assert!(!reload.should_scan(&config, now + Duration::from_secs(10), true));
}

#[test]
fn watch_events_resolve_known_paths_and_request_scan_for_unknown_paths() {
    let dir = tempfile::tempdir().expect("temporary asset root");
    let config = AssetConfig::new(dir.path(), "native")
        .with_auto_reload(true)
        .with_auto_reload_interval(Duration::from_secs(5))
        .with_package_root("packages/base");
    let id = AssetId::new();
    let entry = AssetManifestEntry {
        asset_id: id,
        asset_type: "dummy".to_string(),
        importer: "dummy".to_string(),
        cooker: "dummy".to_string(),
        version: 1,
        source_path: "source/clip.dummy".to_string(),
        cooked_path: "clip.dummyc".to_string(),
        dependencies: Vec::new(),
        import_settings: serde_json::Value::Null,
    };
    let manifest = manifest(vec![entry]);

    let source_roots = changed_roots_from_watch_events(
        &config,
        &manifest,
        &[AssetWatchEvent::Changed(
            config.asset_root.join("source/clip.dummy"),
        )],
    );
    assert_eq!(source_roots.changed_roots, vec![id]);
    assert!(!source_roots.requires_scan);
    assert!(source_roots.requested_reload());

    let cooked_roots = changed_roots_from_watch_events(
        &config,
        &manifest,
        &[AssetWatchEvent::Changed(
            config.cooked_root().join("clip.dummyc"),
        )],
    );
    assert_eq!(cooked_roots.changed_roots, vec![id]);
    assert!(!cooked_roots.requires_scan);

    let package_roots = changed_roots_from_watch_events(
        &config,
        &manifest,
        &[AssetWatchEvent::Changed(
            config.asset_root.join("packages/base/clip.dummyc"),
        )],
    );
    assert_eq!(package_roots.changed_roots, vec![id]);
    assert!(!package_roots.requires_scan);

    let unknown = changed_roots_from_watch_events(
        &config,
        &manifest,
        &[AssetWatchEvent::Changed(
            config.asset_root.join("manifest-or-package.meta"),
        )],
    );
    assert!(unknown.changed_roots.is_empty());
    assert!(unknown.requires_scan);

    let rescan = changed_roots_from_watch_events(&config, &manifest, &[AssetWatchEvent::Rescan]);
    assert!(rescan.requires_scan);
}

#[test]
fn auto_reload_uses_targeted_watch_roots_without_interval_scan() {
    let dir = tempfile::tempdir().expect("temporary asset root");
    let config = AssetConfig::new(dir.path(), "native")
        .with_auto_reload(true)
        .with_auto_reload_interval(Duration::from_secs(60))
        .with_auto_reload_debounce(Duration::ZERO);
    let id = AssetId::new();
    let entry = AssetManifestEntry {
        asset_id: id,
        asset_type: "dummy".to_string(),
        importer: "dummy".to_string(),
        cooker: "dummy".to_string(),
        version: 1,
        source_path: "source/clip.dummy".to_string(),
        cooked_path: "clip.dummyc".to_string(),
        dependencies: Vec::new(),
        import_settings: serde_json::Value::Null,
    };
    let manifest_data = AssetRegistryManifest {
        version: ASSET_SYSTEM_VERSION,
        target: "native".to_string(),
        provenance: Vec::new(),
        assets: vec![entry.clone()],
    };
    let mut manifest = ManifestIndex::new(manifest_data.clone());
    let registry_loads = Arc::new(AtomicUsize::new(0));
    let registry_loader = CountingRegistryLoader {
        manifest: manifest_data,
        loads: Arc::clone(&registry_loads),
    };
    let provider = MemoryAssetProvider::new();
    let mut store = AssetStore::default();
    let mut record = AssetRecord::new(id, "dummy".to_string());
    record.state = AssetState::Installed;
    record.strong_ref_count = 1;
    record.installed = Some(Arc::new("installed".to_string()));
    store.records.insert(id, record);
    let mut events = AssetEventLog::new(8);
    let mut cursor = events.cursor();
    let mut requests = AssetRequests::default();
    let mut reload = AssetReloadController::default();
    let first_check = Instant::now();
    assert!(reload.should_scan(&config, first_check, false));

    drive_auto_reload(
        &config,
        &mut store,
        &mut events,
        &mut requests,
        &mut reload,
        &provider,
        &registry_loader,
        &mut manifest,
        vec![AssetWatchEvent::Changed(
            config.asset_root.join(&entry.source_path),
        )],
        3,
        first_check + Duration::from_secs(1),
    )
    .expect("targeted auto reload should apply");

    assert_eq!(registry_loads.load(Ordering::SeqCst), 0);
    assert_eq!(store.records[&id].state, AssetState::Loading);
    let emitted = events.events_since(&mut cursor);
    assert_eq!(emitted.len(), 1);
    assert_eq!(emitted[0].id, id);
    assert_eq!(emitted[0].kind, AssetEventKind::ReloadQueued);
    assert_eq!(reload.last_report().changed_roots, vec![id]);
    assert_eq!(reload.last_report().impacted, vec![id]);
}

#[test]
fn manual_reload_changed_report_refreshes_manifest_scans_and_applies_roots(
) -> Result<(), Box<dyn std::error::Error>> {
    let id = AssetId::new();
    let entry = entry(id, "clip.dummyc");
    let manifest_data = AssetRegistryManifest {
        version: ASSET_SYSTEM_VERSION,
        target: "native".to_string(),
        provenance: Vec::new(),
        assets: vec![entry.clone()],
    };
    let mut manifest = ManifestIndex::new(AssetRegistryManifest::default());
    let registry_loads = Arc::new(AtomicUsize::new(0));
    let registry_loader = CountingRegistryLoader {
        manifest: manifest_data,
        loads: Arc::clone(&registry_loads),
    };
    let provider = MemoryAssetProvider::new().with_asset(id, b"new");
    let mut store = loaded_store(id, &entry, b"old")?;
    let mut events = AssetEventLog::new(8);
    let mut cursor = events.cursor();
    let mut requests = AssetRequests::default();
    let mut reload = AssetReloadController::default();

    let report = reload_changed_with_report(
        &AssetConfig::default(),
        &mut store,
        &mut events,
        &mut requests,
        &mut reload,
        &provider,
        &registry_loader,
        &mut manifest,
        5,
        Instant::now(),
    )?;

    assert_eq!(registry_loads.load(Ordering::SeqCst), 1);
    assert_eq!(report.changed_roots, vec![id]);
    assert_eq!(report.impacted, vec![id]);
    assert!(report.skipped.is_empty());
    assert_eq!(reload.last_report(), report);
    assert_eq!(store.records[&id].state, AssetState::Loading);
    let emitted = events.events_since(&mut cursor);
    assert_eq!(emitted.len(), 1);
    assert_eq!(emitted[0].id, id);
    assert_eq!(emitted[0].kind, AssetEventKind::ReloadQueued);
    Ok(())
}

#[test]
fn force_reload_root_refreshes_manifest_and_queues_even_when_hash_is_unchanged(
) -> Result<(), Box<dyn std::error::Error>> {
    let id = AssetId::new();
    let entry = entry(id, "clip.dummyc");
    let manifest_data = AssetRegistryManifest {
        version: ASSET_SYSTEM_VERSION,
        target: "native".to_string(),
        provenance: Vec::new(),
        assets: vec![entry.clone()],
    };
    let mut manifest = ManifestIndex::new(AssetRegistryManifest::default());
    let registry_loads = Arc::new(AtomicUsize::new(0));
    let registry_loader = CountingRegistryLoader {
        manifest: manifest_data,
        loads: Arc::clone(&registry_loads),
    };
    let provider = MemoryAssetProvider::new().with_asset(id, b"same");
    let mut store = loaded_store(id, &entry, b"same")?;
    let mut events = AssetEventLog::new(8);
    let mut cursor = events.cursor();
    let mut requests = AssetRequests::default();
    let mut reload = AssetReloadController::default();
    reload.merge_pending_roots(vec![id], Instant::now());

    let report = force_reload_root(
        &AssetConfig::default(),
        &mut store,
        &mut events,
        &mut requests,
        &mut reload,
        &provider,
        &registry_loader,
        &mut manifest,
        id,
        7,
        Instant::now(),
    )?;

    assert_eq!(registry_loads.load(Ordering::SeqCst), 1);
    assert_eq!(report.changed_roots, vec![id]);
    assert_eq!(report.impacted, vec![id]);
    assert_eq!(reload.last_report(), report);
    assert!(reload
        .status(&config(), Instant::now())
        .pending_roots
        .is_empty());
    assert_eq!(store.records[&id].state, AssetState::Loading);
    let emitted = events.events_since(&mut cursor);
    assert_eq!(emitted.len(), 1);
    assert_eq!(emitted[0].id, id);
    assert_eq!(emitted[0].kind, AssetEventKind::ReloadQueued);
    Ok(())
}

#[test]
fn pending_roots_are_deduped_debounced_and_reported_in_status() {
    let mut reload = AssetReloadController::default();
    let config = config();
    let now = Instant::now();
    let first = AssetId::new();
    let second = AssetId::new();

    reload.merge_pending_roots(vec![second, first, second], now);
    let status = reload.status(&config, now + Duration::from_millis(5));
    assert_eq!(status.pending_roots.len(), 2);
    assert_eq!(status.pending_age, Some(Duration::from_millis(5)));
    assert!(reload
        .take_due_pending(&config, now + Duration::from_millis(10))
        .is_none());

    let due = reload
        .take_due_pending(&config, now + Duration::from_millis(25))
        .expect("pending reload roots should become due after debounce");
    assert_eq!(due.len(), 2);
    assert_eq!(
        reload.status(&config, now).pending_roots,
        Vec::<AssetId>::new()
    );
}

#[test]
fn force_discard_clears_pending_root_and_empty_since_marker() {
    let mut reload = AssetReloadController::default();
    let config = config();
    let now = Instant::now();
    let id = AssetId::new();

    reload.merge_pending_roots(vec![id], now);
    reload.discard_pending_root(id);

    let status = reload.status(&config, now + Duration::from_millis(1));
    assert!(status.pending_roots.is_empty());
    assert_eq!(status.pending_age, None);
}

#[test]
fn last_report_is_recorded_for_diagnostics() {
    let mut reload = AssetReloadController::default();
    let id = AssetId::new();
    let report = AssetReloadReport {
        changed_roots: vec![id],
        impacted: vec![id],
        skipped: Vec::new(),
    };

    reload.record_report(report.clone());

    assert_eq!(reload.last_report(), report);
}

fn entry(id: AssetId, cooked_path: &str) -> AssetManifestEntry {
    AssetManifestEntry {
        asset_id: id,
        asset_type: "dummy".to_string(),
        importer: "dummy".to_string(),
        cooker: "dummy".to_string(),
        version: 1,
        source_path: format!("{cooked_path}.src"),
        cooked_path: cooked_path.to_string(),
        dependencies: Vec::new(),
        import_settings: serde_json::Value::Null,
    }
}

fn manifest(entries: Vec<AssetManifestEntry>) -> ManifestIndex {
    ManifestIndex::new(AssetRegistryManifest {
        version: ASSET_SYSTEM_VERSION,
        target: "native".to_string(),
        provenance: Vec::new(),
        assets: entries,
    })
}

fn loaded_store(
    id: AssetId,
    entry: &AssetManifestEntry,
    bytes: &[u8],
) -> Result<AssetStore, AssetError> {
    let mut record = AssetRecord::new(id, entry.asset_type.clone());
    record.strong_ref_count = 1;
    record.loaded = Some(Arc::new("installed".to_string()));
    record.loaded_entry_fingerprint = Some(manifest_entry_fingerprint(entry)?);
    record.loaded_cooked_hash = Some(hash_bytes(bytes));

    let mut store = AssetStore::default();
    store.records.insert(id, record);
    Ok(store)
}

fn memory_source(entry: AssetManifestEntry, bytes: &'static [u8]) -> ResolvedAssetSource {
    ResolvedAssetSource::new(
        entry,
        AssetSourceLocation::Memory {
            label: "memory://reload-test".to_string(),
            bytes: Arc::from(bytes.to_vec()),
            read_error: None,
            read_delay: None,
        },
    )
}

#[test]
fn reload_scan_detects_content_hash_changes() -> Result<(), Box<dyn std::error::Error>> {
    let id = AssetId::new();
    let entry = entry(id, "clip.dummyc");
    let manifest = manifest(vec![entry.clone()]);
    let store = loaded_store(id, &entry, b"old")?;

    let scan = detect_reload_scan(&store, &manifest, |asset_id| {
        assert_eq!(asset_id, id);
        Ok(memory_source(entry.clone(), b"new"))
    })?;

    assert_eq!(scan.changed_roots, vec![id]);
    assert!(scan.skipped.is_empty());
    Ok(())
}

#[test]
fn reload_scan_from_provider_resolves_current_source() -> Result<(), Box<dyn std::error::Error>> {
    let id = AssetId::new();
    let entry = entry(id, "clip.dummyc");
    let manifest = manifest(vec![entry.clone()]);
    let store = loaded_store(id, &entry, b"old")?;
    let provider = MemoryAssetProvider::new().with_asset(id, b"new");

    let scan =
        detect_reload_scan_from_provider(&AssetConfig::default(), &store, &manifest, &provider)?;

    assert_eq!(scan.changed_roots, vec![id]);
    assert!(scan.skipped.is_empty());
    Ok(())
}

#[test]
fn reload_scan_ignores_unreferenced_or_unchanged_records() -> Result<(), Box<dyn std::error::Error>>
{
    let id = AssetId::new();
    let entry = entry(id, "clip.dummyc");
    let manifest = manifest(vec![entry.clone()]);
    let mut store = loaded_store(id, &entry, b"same")?;

    let unchanged = detect_reload_scan(&store, &manifest, |asset_id| {
        assert_eq!(asset_id, id);
        Ok(memory_source(entry.clone(), b"same"))
    })?;
    assert!(unchanged.changed_roots.is_empty());
    assert_eq!(unchanged.skipped.len(), 1);
    assert_eq!(unchanged.skipped[0].asset_id, id);
    assert_eq!(
        unchanged.skipped[0].reason,
        AssetReloadSkipReason::Unchanged
    );

    let record = store.records.get_mut(&id).expect("record exists");
    record.strong_ref_count = 0;
    record.loaded = None;
    let unreferenced = detect_reload_scan(&store, &manifest, |asset_id| {
        assert_eq!(asset_id, id);
        Ok(memory_source(entry.clone(), b"changed"))
    })?;
    assert!(unreferenced.changed_roots.is_empty());
    assert_eq!(unreferenced.skipped.len(), 1);
    assert_eq!(unreferenced.skipped[0].asset_id, id);
    assert_eq!(
        unreferenced.skipped[0].reason,
        AssetReloadSkipReason::UntrackedRecord
    );
    Ok(())
}

#[test]
fn reload_scan_reports_skipped_reasons() -> Result<(), Box<dyn std::error::Error>> {
    let unchanged = AssetId::new();
    let missing_manifest = AssetId::new();
    let untracked = AssetId::new();
    let unchanged_entry = entry(unchanged, "unchanged.dummyc");
    let missing_entry = entry(missing_manifest, "missing.dummyc");
    let manifest = manifest(vec![unchanged_entry.clone()]);
    let mut store = loaded_store(unchanged, &unchanged_entry, b"same")?;

    let mut missing_record = AssetRecord::new(missing_manifest, "dummy".to_string());
    missing_record.strong_ref_count = 1;
    missing_record.loaded = Some(Arc::new("installed".to_string()));
    missing_record.loaded_entry_fingerprint = Some(manifest_entry_fingerprint(&missing_entry)?);
    missing_record.loaded_cooked_hash = Some(hash_bytes(b"missing"));
    store.records.insert(missing_manifest, missing_record);

    store
        .records
        .insert(untracked, AssetRecord::new(untracked, "dummy".to_string()));

    let scan = detect_reload_scan(&store, &manifest, |asset_id| {
        assert_eq!(asset_id, unchanged);
        Ok(memory_source(unchanged_entry.clone(), b"same"))
    })?;

    assert!(scan.changed_roots.is_empty());
    assert_eq!(scan.skipped.len(), 3);
    assert!(scan.skipped.iter().any(|skip| {
        skip.asset_id == unchanged && skip.reason == AssetReloadSkipReason::Unchanged
    }));
    assert!(scan.skipped.iter().any(|skip| {
        skip.asset_id == missing_manifest
            && skip.reason == AssetReloadSkipReason::MissingManifestEntry
    }));
    assert!(scan.skipped.iter().any(|skip| {
        skip.asset_id == untracked && skip.reason == AssetReloadSkipReason::UntrackedRecord
    }));
    Ok(())
}

#[test]
fn reload_scan_detects_manifest_fingerprint_changes() -> Result<(), Box<dyn std::error::Error>> {
    let id = AssetId::new();
    let old_entry = entry(id, "old.dummyc");
    let new_entry = entry(id, "new.dummyc");
    let manifest = manifest(vec![new_entry.clone()]);
    let store = loaded_store(id, &old_entry, b"same")?;

    let scan = detect_reload_scan(&store, &manifest, |asset_id| {
        assert_eq!(asset_id, id);
        Ok(memory_source(new_entry.clone(), b"same"))
    })?;

    assert_eq!(scan.changed_roots, vec![id]);
    assert!(scan.skipped.is_empty());
    Ok(())
}

#[test]
fn prepare_reload_roots_queues_impacted_records_and_reports_missing_manifest() {
    let root = AssetId::new();
    let dependent = AssetId::new();
    let mut store = AssetStore::default();
    store
        .records
        .insert(root, AssetRecord::new(root, "dummy".to_string()));
    let mut dependent_record = AssetRecord::new(dependent, "dummy".to_string());
    dependent_record.dependencies = vec![root];
    store.records.insert(dependent, dependent_record);

    let preparation = prepare_reload_roots(&mut store, vec![root, root], |id| id == root);

    assert_eq!(preparation.report.changed_roots, vec![root]);
    assert_eq!(preparation.report.impacted, vec![root, dependent]);
    assert_eq!(preparation.queued, vec![root]);
    assert_eq!(preparation.missing_manifest, vec![dependent]);
    assert_eq!(
        store.records[&root].state,
        super::super::types::AssetState::Loading
    );
    assert!(store.records[&dependent].reload_pending);
}

#[test]
fn apply_reload_roots_records_report_events_and_missing_manifest_failure() {
    let root = AssetId::new();
    let dependent = AssetId::new();
    let manifest = manifest(vec![entry(root, "root.dummyc")]);
    let mut store = AssetStore::default();
    store
        .records
        .insert(root, AssetRecord::new(root, "dummy".to_string()));
    let mut dependent_record = AssetRecord::new(dependent, "dummy".to_string());
    dependent_record.dependencies = vec![root];
    dependent_record.strong_ref_count = 1;
    store.records.insert(dependent, dependent_record);
    let mut events = AssetEventLog::default();
    let mut cursor = events.cursor();
    let mut reload = AssetReloadController::default();

    let report = apply_reload_roots(
        &mut store,
        &mut events,
        &mut reload,
        &manifest,
        vec![root],
        0,
    );

    assert_eq!(report.changed_roots, vec![root]);
    assert_eq!(report.impacted, vec![root, dependent]);
    assert_eq!(reload.last_report().impacted, vec![root, dependent]);
    assert_eq!(store.records[&root].state, AssetState::Loading);
    assert_eq!(store.records[&dependent].state, AssetState::Failed);

    let emitted = events.events_since(&mut cursor);
    assert_eq!(emitted.len(), 2);
    assert_eq!(emitted[0].id, root);
    assert_eq!(emitted[0].kind, AssetEventKind::ReloadQueued);
    assert_eq!(emitted[1].id, dependent);
    assert_eq!(emitted[1].kind, AssetEventKind::Failed);
}
