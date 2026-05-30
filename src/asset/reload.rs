use std::time::Instant;

use super::dependency;
use super::events::{self, AssetEventLog};
use super::failure;
use super::load::{hash_bytes, manifest_entry_fingerprint};
use super::provider::{self, AssetProvider, ResolvedAssetSource};
use super::registry::{
    refresh_manifest_records_or_fail, AssetRegistry, AssetRegistryLoader, LocalManifestRegistry,
};
use super::request::AssetRequests;
use super::store::{AssetReloadPrepareOutcome, AssetReloadScanRecord, AssetStore};
use super::types::{
    AssetConfig, AssetError, AssetFailurePhase, AssetId, AssetRegistryManifest, AssetReloadReport,
    AssetReloadSkipReason, AssetReloadSkipped, AssetReloadStatus,
};
use super::watcher::AssetWatchEvent;

#[derive(Default)]
pub(crate) struct AssetReloadController {
    last_auto_reload_check: Option<Instant>,
    pending_auto_reload_roots: Vec<AssetId>,
    pending_auto_reload_since: Option<Instant>,
    auto_reload_frozen: bool,
    last_reload_report: AssetReloadReport,
}

impl AssetReloadController {
    pub(crate) fn set_frozen(&mut self, frozen: bool) {
        self.auto_reload_frozen = frozen;
    }

    pub(crate) fn auto_reload_active(&self, config: &AssetConfig) -> bool {
        config.auto_reload && !self.auto_reload_frozen
    }

    pub(crate) fn status(&self, config: &AssetConfig, now: Instant) -> AssetReloadStatus {
        AssetReloadStatus {
            auto_reload_enabled: config.auto_reload,
            file_watcher_enabled: config.file_watcher,
            auto_reload_frozen: self.auto_reload_frozen,
            pending_roots: self.pending_auto_reload_roots.clone(),
            pending_age: self
                .pending_auto_reload_since
                .map(|since| now.saturating_duration_since(since)),
            last_report: self.last_reload_report.clone(),
        }
    }

    pub(crate) fn last_report(&self) -> AssetReloadReport {
        self.last_reload_report.clone()
    }

    pub(crate) fn record_report(&mut self, report: AssetReloadReport) {
        self.last_reload_report = report;
    }

    pub(crate) fn discard_pending_root(&mut self, id: AssetId) {
        self.pending_auto_reload_roots
            .retain(|pending| *pending != id);
        if self.pending_auto_reload_roots.is_empty() {
            self.pending_auto_reload_since = None;
        }
    }

    pub(crate) fn should_scan(
        &mut self,
        config: &AssetConfig,
        now: Instant,
        watcher_requested_scan: bool,
    ) -> bool {
        if !self.auto_reload_active(config) {
            return false;
        }

        let interval_elapsed = self.interval_scan_due(config, now);
        let should_scan = interval_elapsed || watcher_requested_scan;
        if should_scan {
            self.last_auto_reload_check = Some(now);
        }
        should_scan
    }

    pub(crate) fn interval_scan_due(&self, config: &AssetConfig, now: Instant) -> bool {
        !self
            .last_auto_reload_check
            .is_some_and(|last| now.saturating_duration_since(last) < config.auto_reload_interval)
    }

    pub(crate) fn merge_pending_roots(&mut self, changed_roots: Vec<AssetId>, now: Instant) {
        if changed_roots.is_empty() {
            return;
        }
        if self.pending_auto_reload_since.is_none() {
            self.pending_auto_reload_since = Some(now);
        }
        self.pending_auto_reload_roots.extend(changed_roots);
        dedupe_sort_asset_ids(&mut self.pending_auto_reload_roots);
    }

    pub(crate) fn take_due_pending(
        &mut self,
        config: &AssetConfig,
        now: Instant,
    ) -> Option<Vec<AssetId>> {
        let due = self.pending_auto_reload_since.is_some_and(|since| {
            now.saturating_duration_since(since) >= config.auto_reload_debounce
        });
        if !due {
            return None;
        }

        self.pending_auto_reload_since = None;
        Some(std::mem::take(&mut self.pending_auto_reload_roots))
    }
}

#[derive(Default)]
pub(crate) struct AssetWatchReloadRoots {
    pub(crate) changed_roots: Vec<AssetId>,
    pub(crate) requires_scan: bool,
}

impl AssetWatchReloadRoots {
    pub(crate) fn requested_reload(&self) -> bool {
        self.requires_scan || !self.changed_roots.is_empty()
    }
}

pub(crate) fn changed_roots_from_watch_events(
    config: &AssetConfig,
    manifest: &impl AssetRegistry,
    events: &[AssetWatchEvent],
) -> AssetWatchReloadRoots {
    let mut roots = AssetWatchReloadRoots::default();
    for event in events {
        match event {
            AssetWatchEvent::Rescan => roots.requires_scan = true,
            AssetWatchEvent::Changed(path) => {
                if let Some(id) = AssetRegistry::lookup_watch_asset(manifest, config, path) {
                    roots.changed_roots.push(id);
                } else {
                    roots.requires_scan = true;
                }
            }
        }
    }
    dedupe_sort_asset_ids(&mut roots.changed_roots);
    roots
}

#[derive(Debug, Default)]
pub(crate) struct AssetReloadScan {
    pub(crate) changed_roots: Vec<AssetId>,
    pub(crate) skipped: Vec<AssetReloadSkipped>,
}

pub(crate) fn detect_reload_scan<F>(
    store: &AssetStore,
    manifest: &impl AssetRegistry,
    mut resolve_source: F,
) -> Result<AssetReloadScan, AssetError>
where
    F: FnMut(AssetId) -> Result<ResolvedAssetSource, AssetError>,
{
    let mut scan = AssetReloadScan::default();

    for record in store.reload_scan_records() {
        let id = record.id();
        let AssetReloadScanRecord::Tracked {
            loaded_entry_fingerprint,
            loaded_cooked_hash,
            ..
        } = record
        else {
            scan.skipped.push(AssetReloadSkipped {
                asset_id: id,
                reason: AssetReloadSkipReason::UntrackedRecord,
            });
            continue;
        };

        let Some(entry) = manifest.entry(id) else {
            scan.skipped.push(AssetReloadSkipped {
                asset_id: id,
                reason: AssetReloadSkipReason::MissingManifestEntry,
            });
            continue;
        };

        let current_entry_fingerprint = manifest_entry_fingerprint(entry)?;
        let source = resolve_source(id)?;
        let current_cooked_hash = hash_bytes(&source.read_bytes(id)?);
        if loaded_entry_fingerprint != current_entry_fingerprint
            || loaded_cooked_hash != current_cooked_hash
        {
            scan.changed_roots.push(id);
        } else {
            scan.skipped.push(AssetReloadSkipped {
                asset_id: id,
                reason: AssetReloadSkipReason::Unchanged,
            });
        }
    }

    dedupe_sort_asset_ids(&mut scan.changed_roots);
    sort_reload_skips(&mut scan.skipped);
    Ok(scan)
}

pub(crate) fn detect_reload_scan_from_provider(
    config: &AssetConfig,
    store: &AssetStore,
    manifest: &impl AssetRegistry,
    provider: &dyn AssetProvider,
) -> Result<AssetReloadScan, AssetError> {
    detect_reload_scan(store, manifest, |id| {
        provider::resolve_record_source(config, provider, manifest, store, id)
    })
}

pub(crate) fn refresh_manifest_and_detect_scan(
    config: &AssetConfig,
    store: &mut AssetStore,
    events: &mut AssetEventLog,
    requests: &mut AssetRequests,
    provider: &dyn AssetProvider,
    registry_loader: &dyn AssetRegistryLoader,
    manifest: &mut LocalManifestRegistry,
    dependency_priority: i32,
    now: Instant,
) -> Result<AssetReloadScan, AssetError> {
    reload_manifest_records(
        config,
        store,
        events,
        requests,
        provider,
        registry_loader,
        manifest,
        dependency_priority,
        now,
    )?;
    detect_reload_scan_from_provider(config, store, manifest, provider)
}

pub(crate) fn reload_manifest_records(
    config: &AssetConfig,
    store: &mut AssetStore,
    events: &mut AssetEventLog,
    requests: &mut AssetRequests,
    provider: &dyn AssetProvider,
    registry_loader: &dyn AssetRegistryLoader,
    manifest: &mut LocalManifestRegistry,
    dependency_priority: i32,
    now: Instant,
) -> Result<(), AssetError> {
    let next_manifest = registry_loader.load(config)?;
    replace_manifest_records(
        store,
        events,
        requests,
        provider,
        manifest,
        next_manifest,
        dependency_priority,
        now,
    );
    Ok(())
}

pub(crate) fn replace_manifest_records(
    store: &mut AssetStore,
    events: &mut AssetEventLog,
    requests: &mut AssetRequests,
    provider: &dyn AssetProvider,
    manifest: &mut LocalManifestRegistry,
    next_manifest: AssetRegistryManifest,
    dependency_priority: i32,
    now: Instant,
) {
    provider.invalidate_all();
    refresh_manifest_records_or_fail(
        manifest,
        store,
        events,
        requests,
        next_manifest,
        dependency_priority,
        now,
    );
}

pub(crate) fn reload_changed_with_report(
    config: &AssetConfig,
    store: &mut AssetStore,
    events: &mut AssetEventLog,
    requests: &mut AssetRequests,
    reload: &mut AssetReloadController,
    provider: &dyn AssetProvider,
    registry_loader: &dyn AssetRegistryLoader,
    manifest: &mut LocalManifestRegistry,
    dependency_priority: i32,
    now: Instant,
) -> Result<AssetReloadReport, AssetError> {
    let scan = refresh_manifest_and_detect_scan(
        config,
        store,
        events,
        requests,
        provider,
        registry_loader,
        manifest,
        dependency_priority,
        now,
    )?;
    let mut report = apply_reload_roots(
        store,
        events,
        reload,
        manifest,
        scan.changed_roots,
        dependency_priority,
    );
    report.skipped = scan.skipped;
    reload.record_report(report.clone());
    Ok(report)
}

pub(crate) fn force_reload_root(
    config: &AssetConfig,
    store: &mut AssetStore,
    events: &mut AssetEventLog,
    requests: &mut AssetRequests,
    reload: &mut AssetReloadController,
    provider: &dyn AssetProvider,
    registry_loader: &dyn AssetRegistryLoader,
    manifest: &mut LocalManifestRegistry,
    id: AssetId,
    dependency_priority: i32,
    now: Instant,
) -> Result<AssetReloadReport, AssetError> {
    reload_manifest_records(
        config,
        store,
        events,
        requests,
        provider,
        registry_loader,
        manifest,
        dependency_priority,
        now,
    )?;

    if !store.contains_record(id) {
        return Err(AssetError::AssetNotFound { id });
    }

    reload.discard_pending_root(id);
    Ok(apply_reload_roots(
        store,
        events,
        reload,
        manifest,
        vec![id],
        dependency_priority,
    ))
}

pub(crate) fn drive_auto_reload(
    config: &AssetConfig,
    store: &mut AssetStore,
    events: &mut AssetEventLog,
    requests: &mut AssetRequests,
    reload: &mut AssetReloadController,
    provider: &dyn AssetProvider,
    registry_loader: &dyn AssetRegistryLoader,
    manifest: &mut LocalManifestRegistry,
    watcher_events: Vec<AssetWatchEvent>,
    dependency_priority: i32,
    now: Instant,
) -> Result<(), AssetError> {
    if !reload.auto_reload_active(config) {
        return Ok(());
    }

    provider::invalidate_from_watch_events(provider, &watcher_events);
    let watcher_roots = changed_roots_from_watch_events(config, manifest, &watcher_events);
    let interval_scan_due = reload.interval_scan_due(config, now);
    if reload.should_scan(config, now, watcher_roots.requested_reload()) {
        let changed_roots = if !interval_scan_due
            && !watcher_roots.requires_scan
            && !watcher_roots.changed_roots.is_empty()
        {
            watcher_roots.changed_roots
        } else {
            refresh_manifest_and_detect_scan(
                config,
                store,
                events,
                requests,
                provider,
                registry_loader,
                manifest,
                dependency_priority,
                now,
            )?
            .changed_roots
        };
        reload.merge_pending_roots(changed_roots, now);
    }

    if let Some(roots) = reload.take_due_pending(config, now) {
        let _ = apply_reload_roots(store, events, reload, manifest, roots, dependency_priority);
    }

    Ok(())
}

#[derive(Debug)]
pub(crate) struct AssetReloadPreparation {
    pub(crate) report: AssetReloadReport,
    pub(crate) queued: Vec<AssetId>,
    pub(crate) missing_manifest: Vec<AssetId>,
}

pub(crate) fn prepare_reload_roots<F>(
    store: &mut AssetStore,
    mut changed_roots: Vec<AssetId>,
    mut has_manifest_entry: F,
) -> AssetReloadPreparation
where
    F: FnMut(AssetId) -> bool,
{
    dedupe_sort_asset_ids(&mut changed_roots);
    let impacted = dependency::dependent_reload_closure(store, &changed_roots);

    for id in &impacted {
        store.queue_record_reload(*id);
    }

    let mut queued = Vec::new();
    let mut missing_manifest = Vec::new();
    for id in &impacted {
        match store.prepare_record_reload(*id, has_manifest_entry(*id)) {
            AssetReloadPrepareOutcome::Queued => queued.push(*id),
            AssetReloadPrepareOutcome::MissingManifest => missing_manifest.push(*id),
            AssetReloadPrepareOutcome::MissingRecord => {}
        }
    }

    AssetReloadPreparation {
        report: AssetReloadReport {
            changed_roots,
            impacted,
            skipped: Vec::new(),
        },
        queued,
        missing_manifest,
    }
}

pub(crate) fn apply_reload_roots(
    store: &mut AssetStore,
    events: &mut AssetEventLog,
    reload: &mut AssetReloadController,
    manifest: &impl AssetRegistry,
    changed_roots: Vec<AssetId>,
    default_priority: i32,
) -> AssetReloadReport {
    let preparation = prepare_reload_roots(store, changed_roots, |id| manifest.entry(id).is_some());
    events::push_reload_queued_events(events, store, &preparation.queued);
    for id in &preparation.missing_manifest {
        failure::fail_record(
            store,
            events,
            *id,
            AssetError::AssetNotFound { id: *id },
            AssetFailurePhase::Lookup,
            default_priority,
            |dependency| manifest.asset_type(dependency),
        );
    }

    let report = preparation.report;
    reload.record_report(report.clone());
    report
}

fn sort_asset_ids(ids: &mut [AssetId]) {
    ids.sort_by(|left, right| left.to_string().cmp(&right.to_string()));
}

fn dedupe_sort_asset_ids(ids: &mut Vec<AssetId>) {
    sort_asset_ids(ids);
    ids.dedup();
}

fn sort_reload_skips(skipped: &mut [AssetReloadSkipped]) {
    skipped.sort_by(|left, right| {
        left.asset_id
            .to_string()
            .cmp(&right.asset_id.to_string())
            .then_with(|| format!("{:?}", left.reason).cmp(&format!("{:?}", right.reason)))
    });
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    use std::time::{Duration, Instant};

    use super::*;
    use crate::asset::provider::{AssetSourceLocation, MemoryAssetProvider, ResolvedAssetSource};
    use crate::asset::registry::ManifestIndex;
    use crate::asset::store::{AssetRecord, AssetStore};
    use crate::asset::types::{
        AssetEventKind, AssetManifestEntry, AssetRegistryManifest, AssetReloadSkipReason,
        AssetState, ASSET_SYSTEM_VERSION,
    };

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

        let rescan =
            changed_roots_from_watch_events(&config, &manifest, &[AssetWatchEvent::Rescan]);
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
    fn reload_scan_from_provider_resolves_current_source() -> Result<(), Box<dyn std::error::Error>>
    {
        let id = AssetId::new();
        let entry = entry(id, "clip.dummyc");
        let manifest = manifest(vec![entry.clone()]);
        let store = loaded_store(id, &entry, b"old")?;
        let provider = MemoryAssetProvider::new().with_asset(id, b"new");

        let scan = detect_reload_scan_from_provider(
            &AssetConfig::default(),
            &store,
            &manifest,
            &provider,
        )?;

        assert_eq!(scan.changed_roots, vec![id]);
        assert!(scan.skipped.is_empty());
        Ok(())
    }

    #[test]
    fn reload_scan_ignores_unreferenced_or_unchanged_records(
    ) -> Result<(), Box<dyn std::error::Error>> {
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
    fn reload_scan_detects_manifest_fingerprint_changes() -> Result<(), Box<dyn std::error::Error>>
    {
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
}
