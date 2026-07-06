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
use super::watcher::{AssetFileWatcher, AssetWatchEvent};

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

pub(crate) struct AssetReloadService {
    watcher: AssetFileWatcher,
    controller: AssetReloadController,
}

impl AssetReloadService {
    pub(crate) fn new(config: &AssetConfig) -> Self {
        Self {
            watcher: AssetFileWatcher::new(config),
            controller: AssetReloadController::default(),
        }
    }

    pub(crate) fn set_frozen(&mut self, frozen: bool) {
        self.controller.set_frozen(frozen);
    }

    pub(crate) fn last_report(&self) -> AssetReloadReport {
        self.controller.last_report()
    }

    pub(crate) fn status(&self, config: &AssetConfig, now: Instant) -> AssetReloadStatus {
        self.controller.status(config, now)
    }

    pub(crate) fn controller(&self) -> &AssetReloadController {
        &self.controller
    }

    pub(crate) fn reload_manifest(
        &mut self,
        config: &AssetConfig,
        store: &mut AssetStore,
        events: &mut AssetEventLog,
        requests: &mut AssetRequests,
        provider: &dyn AssetProvider,
        registry_loader: &dyn AssetRegistryLoader,
        manifest: &mut LocalManifestRegistry,
        default_priority: i32,
        now: Instant,
    ) -> Result<(), AssetError> {
        reload_manifest_records(
            config,
            store,
            events,
            requests,
            provider,
            registry_loader,
            manifest,
            default_priority,
            now,
        )
    }

    pub(crate) fn reload_changed_with_report(
        &mut self,
        config: &AssetConfig,
        store: &mut AssetStore,
        events: &mut AssetEventLog,
        requests: &mut AssetRequests,
        provider: &dyn AssetProvider,
        registry_loader: &dyn AssetRegistryLoader,
        manifest: &mut LocalManifestRegistry,
        default_priority: i32,
        now: Instant,
    ) -> Result<AssetReloadReport, AssetError> {
        reload_changed_with_report(
            config,
            store,
            events,
            requests,
            &mut self.controller,
            provider,
            registry_loader,
            manifest,
            default_priority,
            now,
        )
    }

    pub(crate) fn force_reload(
        &mut self,
        config: &AssetConfig,
        store: &mut AssetStore,
        events: &mut AssetEventLog,
        requests: &mut AssetRequests,
        provider: &dyn AssetProvider,
        registry_loader: &dyn AssetRegistryLoader,
        manifest: &mut LocalManifestRegistry,
        id: AssetId,
        default_priority: i32,
        now: Instant,
    ) -> Result<AssetReloadReport, AssetError> {
        force_reload_root(
            config,
            store,
            events,
            requests,
            &mut self.controller,
            provider,
            registry_loader,
            manifest,
            id,
            default_priority,
            now,
        )
    }

    pub(crate) fn drive_auto_reload(
        &mut self,
        config: &AssetConfig,
        store: &mut AssetStore,
        events: &mut AssetEventLog,
        requests: &mut AssetRequests,
        provider: &dyn AssetProvider,
        registry_loader: &dyn AssetRegistryLoader,
        manifest: &mut LocalManifestRegistry,
        default_priority: i32,
        now: Instant,
    ) -> Result<(), AssetError> {
        let watcher_events = self.watcher.drain();
        drive_auto_reload(
            config,
            store,
            events,
            requests,
            &mut self.controller,
            provider,
            registry_loader,
            manifest,
            watcher_events,
            default_priority,
            now,
        )
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
