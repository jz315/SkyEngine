use std::time::Duration;

use crate::asset::{
    AssetDependencyBlockerReason, AssetDiagnosticsSnapshot, AssetFailurePhase,
    AssetFailureSnapshot, AssetId, AssetReloadReport, AssetReloadSkipReason, AssetReloadStatus,
    AssetRequestSnapshot, AssetRequestStatus, AssetState, AssetStateCounts, AssetStats, Assets,
};
use crate::diagnostics::{DiagnosticEvent, DiagnosticSeverity, Diagnostics};
use crate::ecs::World;

const SLOW_QUEUE_THRESHOLD: Duration = Duration::from_secs(2);

#[derive(Clone, Debug, Default)]
pub(crate) struct AssetDiagnosticsState {
    last_stats: Option<AssetStats>,
    last_reload_status: Option<AssetReloadStatusKey>,
    last_reload_report: AssetReloadReport,
    active_failures: Vec<AssetFailureKey>,
    reported_failed_requests: Vec<AssetFailedRequestKey>,
    reported_canceled_requests: Vec<AssetCanceledRequestKey>,
    slow_queue_reported: Option<AssetQueuedRequestKey>,
    slow_load_queue_reported: bool,
    slow_active_reported: Option<AssetActiveRequestKey>,
    slow_state_reported: Option<AssetSlowStateKey>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct AssetFailureKey {
    asset_id: AssetId,
    generation: u64,
    phase: Option<AssetFailurePhase>,
    error: String,
}

impl AssetFailureKey {
    fn new(failure: &AssetFailureSnapshot) -> Self {
        Self {
            asset_id: failure.asset_id,
            generation: failure.generation,
            phase: failure.phase,
            error: failure.error.to_string(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct AssetFailedRequestKey {
    request_id: u64,
    asset_id: AssetId,
    generation: u64,
    failure_phase: Option<AssetFailurePhase>,
    error: Option<String>,
}

impl AssetFailedRequestKey {
    fn new(request: &AssetRequestSnapshot) -> Self {
        Self {
            request_id: request.request_id,
            asset_id: request.asset_id,
            generation: request.generation,
            failure_phase: request.failure_phase,
            error: request.last_error.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct AssetCanceledRequestKey {
    request_id: u64,
    asset_id: AssetId,
    generation: u64,
}

impl AssetCanceledRequestKey {
    fn new(request: &AssetRequestSnapshot) -> Self {
        Self {
            request_id: request.request_id,
            asset_id: request.asset_id,
            generation: request.generation,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct AssetQueuedRequestKey {
    request_id: u64,
    asset_id: AssetId,
    generation: u64,
}

impl AssetQueuedRequestKey {
    fn new(request: &AssetRequestSnapshot) -> Self {
        Self {
            request_id: request.request_id,
            asset_id: request.asset_id,
            generation: request.generation,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct AssetActiveRequestKey {
    request_id: u64,
    asset_id: AssetId,
    generation: u64,
    status: AssetRequestStatus,
    failure_phase: Option<AssetFailurePhase>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AssetSlowStateKey {
    state: AssetState,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct AssetReloadStatusKey {
    auto_reload_enabled: bool,
    file_watcher_enabled: bool,
    auto_reload_frozen: bool,
    pending_roots: Vec<AssetId>,
}

impl AssetReloadStatusKey {
    fn new(status: &AssetReloadStatus) -> Self {
        Self {
            auto_reload_enabled: status.auto_reload_enabled,
            file_watcher_enabled: status.file_watcher_enabled,
            auto_reload_frozen: status.auto_reload_frozen,
            pending_roots: status.pending_roots.clone(),
        }
    }
}

impl AssetActiveRequestKey {
    fn new(request: &AssetRequestSnapshot) -> Self {
        Self {
            request_id: request.request_id,
            asset_id: request.asset_id,
            generation: request.generation,
            status: request.status,
            failure_phase: request.failure_phase,
        }
    }
}

pub(crate) fn publish_asset_diagnostics(world: &mut World, assets: &Assets) {
    let diagnostics = ensure_diagnostics(world);
    let frame = Some(world.time.frame_count);
    let snapshot = assets.diagnostics_snapshot();

    let mut state = world
        .remove_resource::<AssetDiagnosticsState>()
        .unwrap_or_default();
    state.publish_snapshot(&diagnostics, frame, snapshot);
    world.insert_resource(state);
}

fn ensure_diagnostics(world: &mut World) -> Diagnostics {
    if let Some(diagnostics) = world.get_resource::<Diagnostics>() {
        return diagnostics.clone();
    }
    let diagnostics = Diagnostics::new();
    world.insert_resource(diagnostics.clone());
    diagnostics
}

fn stats_ref(queued: &[AssetRequestSnapshot]) -> Option<&AssetRequestSnapshot> {
    queued.iter().max_by_key(|request| request.queued_age)
}

fn slowest_active_ref(active: &[AssetRequestSnapshot]) -> Option<&AssetRequestSnapshot> {
    active
        .iter()
        .filter(|request| request.active_age.is_some())
        .max_by_key(|request| request.active_age.unwrap_or_default())
}

fn slowest_state_age(stats: &AssetStats) -> Option<(AssetState, Duration)> {
    let mut slowest = None;
    for (state, age) in [
        (AssetState::Loading, stats.active_state_ages.loading),
        (AssetState::Loaded, stats.active_state_ages.loaded),
        (
            AssetState::WaitingDependencies,
            stats.active_state_ages.waiting_dependencies,
        ),
        (AssetState::Installing, stats.active_state_ages.installing),
        (
            AssetState::Uninstalling,
            stats.active_state_ages.uninstalling,
        ),
        (AssetState::Unloading, stats.active_state_ages.unloading),
    ] {
        let Some(age) = age else {
            continue;
        };
        if slowest.map_or(true, |(_, current)| age > current) {
            slowest = Some((state, age));
        }
    }
    slowest
}

fn state_count(counts: &AssetStateCounts, state: AssetState) -> usize {
    match state {
        AssetState::Unloaded => counts.unloaded,
        AssetState::Loading => counts.loading,
        AssetState::Loaded => counts.loaded,
        AssetState::WaitingDependencies => counts.waiting_dependencies,
        AssetState::Installing => counts.installing,
        AssetState::Installed => counts.installed,
        AssetState::Uninstalling => counts.uninstalling,
        AssetState::Unloading => counts.unloading,
        AssetState::Failed => counts.failed,
    }
}

fn optional_duration_ms(duration: Option<Duration>) -> String {
    duration
        .map(|duration| duration.as_millis().to_string())
        .unwrap_or_else(|| "none".to_string())
}

fn asset_id_list(ids: &[AssetId]) -> String {
    if ids.is_empty() {
        return "none".to_string();
    }

    ids.iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

#[derive(Clone, Copy, Debug, Default)]
struct ReloadSkipSummary {
    unchanged: usize,
    missing_manifest: usize,
    untracked: usize,
}

impl ReloadSkipSummary {
    fn new(report: &AssetReloadReport) -> Self {
        let mut summary = Self::default();
        for skipped in &report.skipped {
            match skipped.reason {
                AssetReloadSkipReason::Unchanged => summary.unchanged += 1,
                AssetReloadSkipReason::MissingManifestEntry => summary.missing_manifest += 1,
                AssetReloadSkipReason::UntrackedRecord => summary.untracked += 1,
            }
        }
        summary
    }
}

fn reload_skip_details(report: &AssetReloadReport) -> String {
    if report.skipped.is_empty() {
        return "none".to_string();
    }

    report
        .skipped
        .iter()
        .map(|skipped| format!("{}:{:?}", skipped.asset_id, skipped.reason))
        .collect::<Vec<_>>()
        .join(",")
}

#[derive(Clone, Copy, Debug, Default)]
struct DependencyBlockerSummary {
    missing: usize,
    failed: usize,
    waiting: usize,
}

impl DependencyBlockerSummary {
    fn new(request: &AssetRequestSnapshot) -> Self {
        let mut summary = Self::default();
        for blocker in &request.dependency_blocker_details {
            match blocker.reason {
                AssetDependencyBlockerReason::Missing => summary.missing += 1,
                AssetDependencyBlockerReason::Failed => summary.failed += 1,
                AssetDependencyBlockerReason::Waiting => summary.waiting += 1,
            }
        }
        summary
    }
}

impl AssetDiagnosticsState {
    fn publish_snapshot(
        &mut self,
        diagnostics: &Diagnostics,
        frame: Option<u64>,
        snapshot: AssetDiagnosticsSnapshot,
    ) {
        self.publish_stats(diagnostics, frame, snapshot.stats);
        self.publish_reload_status(diagnostics, frame, &snapshot.reload_status);
        self.publish_reload_report(diagnostics, frame, snapshot.last_reload_report);
        self.publish_failures(diagnostics, frame, &snapshot.failures);
        self.publish_canceled_requests(diagnostics, frame, &snapshot.canceled_requests);
        self.publish_failed_requests(diagnostics, frame, &snapshot.failed_requests);
        self.publish_slow_queue(diagnostics, frame, stats_ref(&snapshot.queued_requests));
        self.publish_slow_active_request(
            diagnostics,
            frame,
            slowest_active_ref(&snapshot.active_requests),
        );
    }

    fn publish_stats(&mut self, diagnostics: &Diagnostics, frame: Option<u64>, stats: AssetStats) {
        if self.last_stats.as_ref() == Some(&stats) {
            return;
        }
        let previous_deferred_loads = self
            .last_stats
            .as_ref()
            .map(|stats| stats.deferred_load_submissions)
            .unwrap_or(0);
        let previous_provider_resolve_errors = self
            .last_stats
            .as_ref()
            .map(|stats| stats.provider.resolve_errors)
            .unwrap_or(0);
        let previous_provider_cache_invalidations = self
            .last_stats
            .as_ref()
            .map(|stats| stats.provider.cache_invalidations)
            .unwrap_or(0);
        let previous_provider_full_cache_invalidations = self
            .last_stats
            .as_ref()
            .map(|stats| stats.provider.full_cache_invalidations)
            .unwrap_or(0);
        let deferred_delta = stats
            .deferred_load_submissions
            .saturating_sub(previous_deferred_loads);
        let provider_resolve_error_delta = stats
            .provider
            .resolve_errors
            .saturating_sub(previous_provider_resolve_errors);
        let provider_cache_invalidation_delta = stats
            .provider
            .cache_invalidations
            .saturating_sub(previous_provider_cache_invalidations);
        let provider_full_cache_invalidation_delta = stats
            .provider
            .full_cache_invalidations
            .saturating_sub(previous_provider_full_cache_invalidations);

        diagnostics.push(
            DiagnosticEvent::new(
                "asset",
                "asset.stats",
                DiagnosticSeverity::Info,
                "Asset runtime stats changed",
            )
            .with_frame(frame)
            .with_field("records", stats.records)
            .with_field("queued_requests", stats.queued_requests)
            .with_field("active_requests", stats.active_requests)
            .with_field("inflight_loads", stats.inflight_loads)
            .with_field("load_worker_threads", stats.load_worker_threads)
            .with_field("running_load_jobs", stats.running_load_jobs)
            .with_field("queued_load_jobs", stats.queued_load_jobs)
            .with_field("load_queue_capacity", stats.load_queue_capacity)
            .with_field(
                "oldest_queued_load_job_ms",
                optional_duration_ms(stats.oldest_queued_load_job_age),
            )
            .with_field("source_loads_queued", stats.source_load_phases.queued)
            .with_field("source_loads_reading", stats.source_load_phases.reading)
            .with_field("source_loads_decoding", stats.source_load_phases.decoding)
            .with_field("deferred_load_submissions", stats.deferred_load_submissions)
            .with_field("provider_package_roots", stats.provider.package_roots)
            .with_field("provider_package_files", stats.provider.package_files)
            .with_field(
                "provider_cached_bundle_indexes",
                stats.provider.cached_bundle_indexes,
            )
            .with_field(
                "provider_cached_bundle_index_entries",
                stats.provider.cached_bundle_index_entries,
            )
            .with_field(
                "provider_resolved_raw_sources",
                stats.provider.resolved_raw_sources,
            )
            .with_field(
                "provider_resolved_cooked_sources",
                stats.provider.resolved_cooked_sources,
            )
            .with_field(
                "provider_resolved_package_sources",
                stats.provider.resolved_package_sources,
            )
            .with_field(
                "provider_resolved_bundle_sources",
                stats.provider.resolved_bundle_sources,
            )
            .with_field("provider_resolve_errors", stats.provider.resolve_errors)
            .with_field(
                "provider_cache_invalidations",
                stats.provider.cache_invalidations,
            )
            .with_field(
                "provider_full_cache_invalidations",
                stats.provider.full_cache_invalidations,
            )
            .with_field("submitted_requests", stats.submitted_requests)
            .with_field("activated_requests", stats.activated_requests)
            .with_field("canceled_requests", stats.canceled_requests)
            .with_field("failed_requests", stats.failed_requests)
            .with_field(
                "completed_requests",
                stats.request_timings.completed_requests,
            )
            .with_field(
                "completed_source_loads",
                stats.load_timings.completed_source_loads,
            )
            .with_field(
                "failed_source_loads",
                stats.load_timings.failed_source_loads,
            )
            .with_field("retained_events", stats.retained_events)
            .with_field("strong_references", stats.strong_references)
            .with_field("dependency_references", stats.dependency_references)
            .with_field(
                "oldest_queued_request_ms",
                stats
                    .oldest_queued_request_age
                    .map(|age| age.as_millis().to_string())
                    .unwrap_or_else(|| "none".to_string()),
            )
            .with_field(
                "oldest_active_request_ms",
                optional_duration_ms(stats.oldest_active_request_age),
            )
            .with_field(
                "oldest_state_loading_ms",
                optional_duration_ms(stats.active_state_ages.loading),
            )
            .with_field(
                "oldest_state_loaded_ms",
                optional_duration_ms(stats.active_state_ages.loaded),
            )
            .with_field(
                "oldest_state_waiting_dependencies_ms",
                optional_duration_ms(stats.active_state_ages.waiting_dependencies),
            )
            .with_field(
                "oldest_state_installing_ms",
                optional_duration_ms(stats.active_state_ages.installing),
            )
            .with_field(
                "oldest_state_uninstalling_ms",
                optional_duration_ms(stats.active_state_ages.uninstalling),
            )
            .with_field(
                "oldest_state_unloading_ms",
                optional_duration_ms(stats.active_state_ages.unloading),
            )
            .with_field(
                "average_request_queue_wait_ms",
                optional_duration_ms(stats.request_timings.average_queue_wait),
            )
            .with_field(
                "average_canceled_request_queue_wait_ms",
                optional_duration_ms(stats.request_timings.average_canceled_queue_wait),
            )
            .with_field(
                "average_request_active_ms",
                optional_duration_ms(stats.request_timings.average_active_time),
            )
            .with_field(
                "average_request_total_ms",
                optional_duration_ms(stats.request_timings.average_total_time),
            )
            .with_field(
                "average_load_read_ms",
                optional_duration_ms(stats.load_timings.average_read_time),
            )
            .with_field(
                "average_load_decode_ms",
                optional_duration_ms(stats.load_timings.average_decode_time),
            )
            .with_field(
                "average_load_total_ms",
                optional_duration_ms(stats.load_timings.average_total_time),
            )
            .with_field(
                "average_request_loading_ms",
                optional_duration_ms(stats.request_timings.loading.average),
            )
            .with_field(
                "average_request_decoding_ms",
                optional_duration_ms(stats.request_timings.decoding.average),
            )
            .with_field(
                "average_request_waiting_dependencies_ms",
                optional_duration_ms(stats.request_timings.waiting_dependencies.average),
            )
            .with_field(
                "average_request_ready_to_install_ms",
                optional_duration_ms(stats.request_timings.ready_to_install.average),
            )
            .with_field(
                "average_request_installing_ms",
                optional_duration_ms(stats.request_timings.installing.average),
            )
            .with_field(
                "average_request_unloading_ms",
                optional_duration_ms(stats.request_timings.unloading.average),
            )
            .with_field(
                "request_loading_samples",
                stats.request_timings.loading.samples,
            )
            .with_field(
                "request_decoding_samples",
                stats.request_timings.decoding.samples,
            )
            .with_field(
                "request_waiting_dependencies_samples",
                stats.request_timings.waiting_dependencies.samples,
            )
            .with_field(
                "request_ready_to_install_samples",
                stats.request_timings.ready_to_install.samples,
            )
            .with_field(
                "request_installing_samples",
                stats.request_timings.installing.samples,
            )
            .with_field(
                "request_unloading_samples",
                stats.request_timings.unloading.samples,
            )
            .with_field("state_unloaded", stats.states.unloaded)
            .with_field("state_loading", stats.states.loading)
            .with_field("state_loaded", stats.states.loaded)
            .with_field(
                "state_waiting_dependencies",
                stats.states.waiting_dependencies,
            )
            .with_field("state_installing", stats.states.installing)
            .with_field("state_installed", stats.states.installed)
            .with_field("state_uninstalling", stats.states.uninstalling)
            .with_field("state_unloading", stats.states.unloading)
            .with_field("state_failed", stats.states.failed),
        );
        log::debug!(
            target: "sky_engine::asset",
            "asset stats records={} queued={} active={} inflight={} running_loads={}/{} load_queue={}/{} deferred_loads={} provider_bundle_indexes={} provider_bundle_entries={} oldest_loading_ms={} installed={} failed={}",
            stats.records,
            stats.queued_requests,
            stats.active_requests,
            stats.inflight_loads,
            stats.running_load_jobs,
            stats.load_worker_threads,
            stats.queued_load_jobs,
            stats.load_queue_capacity,
            stats.deferred_load_submissions,
            stats.provider.cached_bundle_indexes,
            stats.provider.cached_bundle_index_entries,
            optional_duration_ms(stats.active_state_ages.loading),
            stats.states.installed,
            stats.states.failed
        );
        if deferred_delta > 0 {
            diagnostics.push(
                DiagnosticEvent::new(
                    "asset",
                    "asset.load.backpressure",
                    DiagnosticSeverity::Warning,
                    "Asset load submissions were deferred because the bounded I/O queue was full",
                )
                .with_frame(frame)
                .with_field("deferred_delta", deferred_delta)
                .with_field("deferred_load_submissions", stats.deferred_load_submissions)
                .with_field("load_worker_threads", stats.load_worker_threads)
                .with_field("running_load_jobs", stats.running_load_jobs)
                .with_field("queued_load_jobs", stats.queued_load_jobs)
                .with_field("load_queue_capacity", stats.load_queue_capacity)
                .with_field("inflight_loads", stats.inflight_loads),
            );
            log::warn!(
                target: "sky_engine::asset",
                "asset load back-pressure deferred_delta={} total_deferred={} running_loads={}/{} queue={}/{} inflight={}",
                deferred_delta,
                stats.deferred_load_submissions,
                stats.running_load_jobs,
                stats.load_worker_threads,
                stats.queued_load_jobs,
                stats.load_queue_capacity,
                stats.inflight_loads
            );
        }
        if provider_resolve_error_delta > 0 {
            diagnostics.push(
                DiagnosticEvent::new(
                    "asset",
                    "asset.provider.resolve.failed",
                    DiagnosticSeverity::Warning,
                    "Asset provider source resolution failures increased",
                )
                .with_frame(frame)
                .with_field("resolve_error_delta", provider_resolve_error_delta)
                .with_field("provider_resolve_errors", stats.provider.resolve_errors)
                .with_field(
                    "provider_resolved_raw_sources",
                    stats.provider.resolved_raw_sources,
                )
                .with_field(
                    "provider_resolved_cooked_sources",
                    stats.provider.resolved_cooked_sources,
                )
                .with_field(
                    "provider_resolved_package_sources",
                    stats.provider.resolved_package_sources,
                )
                .with_field(
                    "provider_resolved_bundle_sources",
                    stats.provider.resolved_bundle_sources,
                )
                .with_field("provider_package_roots", stats.provider.package_roots)
                .with_field("provider_package_files", stats.provider.package_files)
                .with_field(
                    "provider_cached_bundle_indexes",
                    stats.provider.cached_bundle_indexes,
                )
                .with_field(
                    "provider_cached_bundle_index_entries",
                    stats.provider.cached_bundle_index_entries,
                ),
            );
            log::warn!(
                target: "sky_engine::asset",
                "asset provider source resolution failures increased by {} (total={}) raw={} cooked={} package={} bundle={}",
                provider_resolve_error_delta,
                stats.provider.resolve_errors,
                stats.provider.resolved_raw_sources,
                stats.provider.resolved_cooked_sources,
                stats.provider.resolved_package_sources,
                stats.provider.resolved_bundle_sources
            );
        }
        if provider_cache_invalidation_delta > 0 || provider_full_cache_invalidation_delta > 0 {
            diagnostics.push(
                DiagnosticEvent::new(
                    "asset",
                    "asset.provider.cache.invalidated",
                    DiagnosticSeverity::Info,
                    "Asset provider cache invalidations increased",
                )
                .with_frame(frame)
                .with_field(
                    "cache_invalidation_delta",
                    provider_cache_invalidation_delta,
                )
                .with_field(
                    "full_cache_invalidation_delta",
                    provider_full_cache_invalidation_delta,
                )
                .with_field(
                    "provider_cache_invalidations",
                    stats.provider.cache_invalidations,
                )
                .with_field(
                    "provider_full_cache_invalidations",
                    stats.provider.full_cache_invalidations,
                )
                .with_field("provider_package_roots", stats.provider.package_roots)
                .with_field("provider_package_files", stats.provider.package_files)
                .with_field(
                    "provider_cached_bundle_indexes",
                    stats.provider.cached_bundle_indexes,
                )
                .with_field(
                    "provider_cached_bundle_index_entries",
                    stats.provider.cached_bundle_index_entries,
                ),
            );
            log::info!(
                target: "sky_engine::asset",
                "asset provider cache invalidations increased by {} (full_delta={}, total={}, full_total={}, cached_bundle_indexes={}, cached_bundle_entries={})",
                provider_cache_invalidation_delta,
                provider_full_cache_invalidation_delta,
                stats.provider.cache_invalidations,
                stats.provider.full_cache_invalidations,
                stats.provider.cached_bundle_indexes,
                stats.provider.cached_bundle_index_entries
            );
        }
        self.publish_slow_load_queue(diagnostics, frame, &stats);
        self.publish_slow_state(diagnostics, frame, &stats);
        self.last_stats = Some(stats);
    }

    fn publish_slow_load_queue(
        &mut self,
        diagnostics: &Diagnostics,
        frame: Option<u64>,
        stats: &AssetStats,
    ) {
        let Some(oldest_age) = stats.oldest_queued_load_job_age else {
            self.slow_load_queue_reported = false;
            return;
        };
        if oldest_age < SLOW_QUEUE_THRESHOLD {
            self.slow_load_queue_reported = false;
            return;
        }
        if self.slow_load_queue_reported {
            return;
        }

        diagnostics.push(
            DiagnosticEvent::new(
                "asset",
                "asset.load.queue.slow",
                DiagnosticSeverity::Warning,
                "Asset source-load worker job has been queued for longer than the app diagnostic threshold",
            )
            .with_frame(frame)
            .with_field("oldest_queued_load_job_ms", oldest_age.as_millis())
            .with_field("load_worker_threads", stats.load_worker_threads)
            .with_field("running_load_jobs", stats.running_load_jobs)
            .with_field("queued_load_jobs", stats.queued_load_jobs)
            .with_field("load_queue_capacity", stats.load_queue_capacity)
            .with_field("inflight_loads", stats.inflight_loads)
            .with_field("source_loads_queued", stats.source_load_phases.queued)
            .with_field("source_loads_reading", stats.source_load_phases.reading)
            .with_field("source_loads_decoding", stats.source_load_phases.decoding),
        );
        log::warn!(
            target: "sky_engine::asset",
            "asset source-load worker queue oldest job has waited {}ms queue={}/{} running={}/{} inflight={}",
            oldest_age.as_millis(),
            stats.queued_load_jobs,
            stats.load_queue_capacity,
            stats.running_load_jobs,
            stats.load_worker_threads,
            stats.inflight_loads
        );
        self.slow_load_queue_reported = true;
    }

    fn publish_slow_state(
        &mut self,
        diagnostics: &Diagnostics,
        frame: Option<u64>,
        stats: &AssetStats,
    ) {
        let Some((state, age)) = slowest_state_age(stats) else {
            self.slow_state_reported = None;
            return;
        };
        if age < SLOW_QUEUE_THRESHOLD {
            self.slow_state_reported = None;
            return;
        }

        let key = AssetSlowStateKey { state };
        if self.slow_state_reported.as_ref() == Some(&key) {
            return;
        }

        diagnostics.push(
            DiagnosticEvent::new(
                "asset",
                "asset.state.slow",
                DiagnosticSeverity::Warning,
                "Asset record lifecycle state has been active for longer than the app diagnostic threshold",
            )
            .with_frame(frame)
            .with_field("state", format!("{state:?}"))
            .with_field("state_age_ms", age.as_millis())
            .with_field("state_records", state_count(&stats.states, state))
            .with_field("queued_requests", stats.queued_requests)
            .with_field("active_requests", stats.active_requests)
            .with_field("inflight_loads", stats.inflight_loads)
            .with_field(
                "oldest_queued_request_ms",
                optional_duration_ms(stats.oldest_queued_request_age),
            )
            .with_field(
                "oldest_active_request_ms",
                optional_duration_ms(stats.oldest_active_request_age),
            ),
        );
        log::warn!(
            target: "sky_engine::asset",
            "asset records in {:?} have waited {}ms count={} queued={} active={} inflight={}",
            state,
            age.as_millis(),
            state_count(&stats.states, state),
            stats.queued_requests,
            stats.active_requests,
            stats.inflight_loads
        );
        self.slow_state_reported = Some(key);
    }

    fn publish_reload_status(
        &mut self,
        diagnostics: &Diagnostics,
        frame: Option<u64>,
        status: &AssetReloadStatus,
    ) {
        let key = AssetReloadStatusKey::new(status);
        if self.last_reload_status.as_ref() == Some(&key) {
            return;
        }

        diagnostics.push(
            DiagnosticEvent::new(
                "asset",
                "asset.reload.status",
                DiagnosticSeverity::Info,
                "Asset reload status changed",
            )
            .with_frame(frame)
            .with_field("auto_reload_enabled", status.auto_reload_enabled)
            .with_field("file_watcher_enabled", status.file_watcher_enabled)
            .with_field("auto_reload_frozen", status.auto_reload_frozen)
            .with_field("pending_roots", status.pending_roots.len())
            .with_field("pending_root_ids", asset_id_list(&status.pending_roots))
            .with_field("pending_age_ms", optional_duration_ms(status.pending_age))
            .with_field("last_changed_roots", status.last_report.changed_roots.len())
            .with_field("last_impacted", status.last_report.impacted.len())
            .with_field("last_skipped", status.last_report.skipped.len()),
        );
        log::debug!(
            target: "sky_engine::asset",
            "asset reload status auto={} watcher={} frozen={} pending_roots={}",
            status.auto_reload_enabled,
            status.file_watcher_enabled,
            status.auto_reload_frozen,
            status.pending_roots.len()
        );
        self.last_reload_status = Some(key);
    }

    fn publish_reload_report(
        &mut self,
        diagnostics: &Diagnostics,
        frame: Option<u64>,
        report: AssetReloadReport,
    ) {
        if report.is_empty() || self.last_reload_report == report {
            return;
        }
        let skip_summary = ReloadSkipSummary::new(&report);

        diagnostics.push(
            DiagnosticEvent::new(
                "asset",
                "asset.reload",
                DiagnosticSeverity::Info,
                "Asset reload report updated",
            )
            .with_frame(frame)
            .with_field("changed_roots", report.changed_roots.len())
            .with_field("impacted", report.impacted.len())
            .with_field("skipped", report.skipped.len())
            .with_field("changed_root_ids", asset_id_list(&report.changed_roots))
            .with_field("impacted_ids", asset_id_list(&report.impacted))
            .with_field("skipped_unchanged", skip_summary.unchanged)
            .with_field("skipped_missing_manifest", skip_summary.missing_manifest)
            .with_field("skipped_untracked", skip_summary.untracked)
            .with_field("skipped_details", reload_skip_details(&report)),
        );
        log::info!(
            target: "sky_engine::asset",
            "asset reload changed_roots={} impacted={} skipped={} unchanged={} missing_manifest={} untracked={}",
            report.changed_roots.len(),
            report.impacted.len(),
            report.skipped.len(),
            skip_summary.unchanged,
            skip_summary.missing_manifest,
            skip_summary.untracked
        );
        self.last_reload_report = report;
    }

    fn publish_failures(
        &mut self,
        diagnostics: &Diagnostics,
        frame: Option<u64>,
        failures: &[AssetFailureSnapshot],
    ) {
        let current: Vec<_> = failures.iter().map(AssetFailureKey::new).collect();
        self.active_failures
            .retain(|known| current.iter().any(|failure| failure == known));

        for failure in failures {
            let key = AssetFailureKey::new(failure);
            if self.active_failures.iter().any(|known| known == &key) {
                continue;
            }

            diagnostics.push(
                DiagnosticEvent::new(
                    "asset",
                    "asset.failure",
                    DiagnosticSeverity::Error,
                    failure.error.to_string(),
                )
                .with_frame(frame)
                .with_field("asset_id", failure.asset_id)
                .with_field("asset_type", &failure.asset_type)
                .with_field("state", format!("{:?}", failure.state))
                .with_field("generation", failure.generation)
                .with_field(
                    "phase",
                    failure
                        .phase
                        .map(|phase| format!("{phase:?}"))
                        .unwrap_or_else(|| "unknown".to_string()),
                )
                .with_field("reload_pending", failure.reload_pending),
            );
            log::warn!(
                target: "sky_engine::asset",
                "asset failure id={} type={} phase={:?}: {}",
                failure.asset_id,
                failure.asset_type,
                failure.phase,
                failure.error
            );
            self.active_failures.push(key);
        }
    }

    fn publish_failed_requests(
        &mut self,
        diagnostics: &Diagnostics,
        frame: Option<u64>,
        requests: &[AssetRequestSnapshot],
    ) {
        let current: Vec<_> = requests.iter().map(AssetFailedRequestKey::new).collect();
        self.reported_failed_requests
            .retain(|known| current.iter().any(|request| request == known));

        for request in requests {
            let key = AssetFailedRequestKey::new(request);
            if self
                .reported_failed_requests
                .iter()
                .any(|known| known == &key)
            {
                continue;
            }
            let blockers = DependencyBlockerSummary::new(request);

            diagnostics.push(
                DiagnosticEvent::new(
                    "asset",
                    "asset.request.failed",
                    DiagnosticSeverity::Error,
                    request
                        .last_error
                        .clone()
                        .unwrap_or_else(|| "Asset request failed".to_string()),
                )
                .with_frame(frame)
                .with_field("request_id", request.request_id)
                .with_field("asset_id", request.asset_id)
                .with_field("generation", request.generation)
                .with_field("priority", request.priority)
                .with_field("status", format!("{:?}", request.status))
                .with_field(
                    "failure_phase",
                    request
                        .failure_phase
                        .map(|phase| format!("{phase:?}"))
                        .unwrap_or_else(|| "unknown".to_string()),
                )
                .with_field("progress_label", &request.progress.label)
                .with_field("progress_percent", request.progress.percent())
                .with_field("queued_ms", request.queued_age.as_millis())
                .with_field("active_ms", optional_duration_ms(request.active_age))
                .with_field("phase_ms", request.phase_age.as_millis())
                .with_field("dependency_blockers", request.dependency_blockers.len())
                .with_field("dependency_missing_blockers", blockers.missing)
                .with_field("dependency_failed_blockers", blockers.failed)
                .with_field("dependency_waiting_blockers", blockers.waiting)
                .with_field("dependency_cycle_length", request.dependency_cycle.len())
                .with_field(
                    "last_error",
                    request.last_error.as_deref().unwrap_or("none"),
                ),
            );
            log::warn!(
                target: "sky_engine::asset",
                "asset request {} for {} failed: {}",
                request.request_id,
                request.asset_id,
                request.last_error.as_deref().unwrap_or("unknown")
            );
            self.reported_failed_requests.push(key);
        }
    }

    fn publish_canceled_requests(
        &mut self,
        diagnostics: &Diagnostics,
        frame: Option<u64>,
        requests: &[AssetRequestSnapshot],
    ) {
        let current: Vec<_> = requests.iter().map(AssetCanceledRequestKey::new).collect();
        self.reported_canceled_requests
            .retain(|known| current.iter().any(|request| request == known));

        for request in requests {
            let key = AssetCanceledRequestKey::new(request);
            if self
                .reported_canceled_requests
                .iter()
                .any(|known| known == &key)
            {
                continue;
            }

            diagnostics.push(
                DiagnosticEvent::new(
                    "asset",
                    "asset.request.canceled",
                    DiagnosticSeverity::Info,
                    "Asset request was canceled before activation",
                )
                .with_frame(frame)
                .with_field("request_id", request.request_id)
                .with_field("asset_id", request.asset_id)
                .with_field("generation", request.generation)
                .with_field("priority", request.priority)
                .with_field("status", format!("{:?}", request.status))
                .with_field("progress_label", &request.progress.label)
                .with_field("progress_percent", request.progress.percent())
                .with_field("queued_ms", request.queued_age.as_millis()),
            );
            log::debug!(
                target: "sky_engine::asset",
                "asset request {} for {} canceled before activation",
                request.request_id,
                request.asset_id
            );
            self.reported_canceled_requests.push(key);
        }
    }

    fn publish_slow_queue(
        &mut self,
        diagnostics: &Diagnostics,
        frame: Option<u64>,
        oldest: Option<&AssetRequestSnapshot>,
    ) {
        let Some(oldest) = oldest else {
            self.slow_queue_reported = None;
            return;
        };
        if oldest.queued_age < SLOW_QUEUE_THRESHOLD {
            self.slow_queue_reported = None;
            return;
        }

        let key = AssetQueuedRequestKey::new(oldest);
        if self.slow_queue_reported.as_ref() == Some(&key) {
            return;
        }

        diagnostics.push(
            DiagnosticEvent::new(
                "asset",
                "asset.queue.slow",
                DiagnosticSeverity::Warning,
                "Asset request has been queued for longer than the app diagnostic threshold",
            )
            .with_frame(frame)
            .with_field("request_id", oldest.request_id)
            .with_field("asset_id", oldest.asset_id)
            .with_field("generation", oldest.generation)
            .with_field("priority", oldest.priority)
            .with_field("status", format!("{:?}", oldest.status))
            .with_field("progress_label", &oldest.progress.label)
            .with_field("progress_percent", oldest.progress.percent())
            .with_field("queued_ms", oldest.queued_age.as_millis()),
        );
        log::warn!(
            target: "sky_engine::asset",
            "asset request {} for {} has been queued for {}ms",
            oldest.request_id,
            oldest.asset_id,
            oldest.queued_age.as_millis()
        );
        self.slow_queue_reported = Some(key);
    }

    fn publish_slow_active_request(
        &mut self,
        diagnostics: &Diagnostics,
        frame: Option<u64>,
        slowest: Option<&AssetRequestSnapshot>,
    ) {
        let Some(slowest) = slowest else {
            self.slow_active_reported = None;
            return;
        };
        let active_age = slowest.active_age.unwrap_or_default();
        if active_age < SLOW_QUEUE_THRESHOLD {
            self.slow_active_reported = None;
            return;
        }

        let key = AssetActiveRequestKey::new(slowest);
        if self.slow_active_reported.as_ref() == Some(&key) {
            return;
        }
        let blockers = DependencyBlockerSummary::new(slowest);

        diagnostics.push(
            DiagnosticEvent::new(
                "asset",
                "asset.request.slow",
                DiagnosticSeverity::Warning,
                "Asset request has been active for longer than the app diagnostic threshold",
            )
            .with_frame(frame)
            .with_field("request_id", slowest.request_id)
            .with_field("asset_id", slowest.asset_id)
            .with_field("generation", slowest.generation)
            .with_field("priority", slowest.priority)
            .with_field("status", format!("{:?}", slowest.status))
            .with_field(
                "failure_phase",
                slowest
                    .failure_phase
                    .map(|phase| format!("{phase:?}"))
                    .unwrap_or_else(|| "unknown".to_string()),
            )
            .with_field("progress_label", &slowest.progress.label)
            .with_field("progress_percent", slowest.progress.percent())
            .with_field("active_ms", active_age.as_millis())
            .with_field("phase_ms", slowest.phase_age.as_millis())
            .with_field("dependency_blockers", slowest.dependency_blockers.len())
            .with_field("dependency_missing_blockers", blockers.missing)
            .with_field("dependency_failed_blockers", blockers.failed)
            .with_field("dependency_waiting_blockers", blockers.waiting)
            .with_field("dependency_cycle_length", slowest.dependency_cycle.len())
            .with_field(
                "last_error",
                slowest.last_error.as_deref().unwrap_or("none"),
            ),
        );
        log::warn!(
            target: "sky_engine::asset",
            "asset request {} for {} has been active for {}ms in {:?}",
            slowest.request_id,
            slowest.asset_id,
            active_age.as_millis(),
            slowest.status
        );
        self.slow_active_reported = Some(key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset::{
        AssetActiveStateAgeStats, AssetConfig, AssetDependencyBlocker,
        AssetDependencyBlockerReason, AssetProviderStats, AssetReloadSkipped, AssetRequestProgress,
        AssetSourceLoadPhaseCounts, AssetStateCounts, TextureAsset,
    };

    #[test]
    fn publish_asset_diagnostics_inserts_structured_stats_event() {
        let mut world = World::new();
        let assets = Assets::with_empty_manifest(AssetConfig::default());
        let _handle = assets.insert_runtime(TextureAsset::white_pixel());
        world.insert_resource(assets.clone());

        publish_asset_diagnostics(&mut world, &assets);

        let diagnostics = world.get_resource::<Diagnostics>().unwrap();
        let events = diagnostics.events();
        let stats = events
            .iter()
            .find(|event| event.code == "asset.stats")
            .expect("asset stats event");
        assert!(stats
            .fields
            .iter()
            .any(|field| field.key == "failed_requests"));
        assert!(stats
            .fields
            .iter()
            .any(|field| field.key == "oldest_active_request_ms"));
        assert!(stats
            .fields
            .iter()
            .any(|field| field.key == "completed_source_loads"));
        assert!(stats
            .fields
            .iter()
            .any(|field| field.key == "load_worker_threads"));
        assert!(stats
            .fields
            .iter()
            .any(|field| field.key == "running_load_jobs"));
        assert!(stats
            .fields
            .iter()
            .any(|field| field.key == "queued_load_jobs"));
        assert!(stats
            .fields
            .iter()
            .any(|field| field.key == "load_queue_capacity"));
        assert!(stats
            .fields
            .iter()
            .any(|field| field.key == "oldest_queued_load_job_ms"));
        assert!(stats
            .fields
            .iter()
            .any(|field| field.key == "source_loads_queued"));
        assert!(stats
            .fields
            .iter()
            .any(|field| field.key == "source_loads_reading"));
        assert!(stats
            .fields
            .iter()
            .any(|field| field.key == "source_loads_decoding"));
        assert!(stats
            .fields
            .iter()
            .any(|field| field.key == "deferred_load_submissions"));
        assert!(stats
            .fields
            .iter()
            .any(|field| field.key == "oldest_state_loading_ms"));
        assert!(stats
            .fields
            .iter()
            .any(|field| field.key == "oldest_state_installing_ms"));
        assert!(stats
            .fields
            .iter()
            .any(|field| field.key == "provider_package_roots"));
        assert!(stats
            .fields
            .iter()
            .any(|field| field.key == "provider_package_files"));
        assert!(stats
            .fields
            .iter()
            .any(|field| field.key == "provider_cached_bundle_indexes"));
        assert!(stats
            .fields
            .iter()
            .any(|field| field.key == "provider_cached_bundle_index_entries"));
        assert!(stats
            .fields
            .iter()
            .any(|field| field.key == "provider_resolved_raw_sources"));
        assert!(stats
            .fields
            .iter()
            .any(|field| field.key == "provider_resolved_cooked_sources"));
        assert!(stats
            .fields
            .iter()
            .any(|field| field.key == "provider_resolved_package_sources"));
        assert!(stats
            .fields
            .iter()
            .any(|field| field.key == "provider_resolved_bundle_sources"));
        assert!(stats
            .fields
            .iter()
            .any(|field| field.key == "provider_resolve_errors"));
        assert!(stats
            .fields
            .iter()
            .any(|field| field.key == "provider_cache_invalidations"));
        assert!(stats
            .fields
            .iter()
            .any(|field| field.key == "provider_full_cache_invalidations"));
        assert!(stats
            .fields
            .iter()
            .any(|field| field.key == "average_canceled_request_queue_wait_ms"));
        assert!(stats
            .fields
            .iter()
            .any(|field| field.key == "average_load_total_ms"));
        assert!(stats
            .fields
            .iter()
            .any(|field| field.key == "average_request_decoding_ms"));
        assert!(stats
            .fields
            .iter()
            .any(|field| field.key == "average_request_unloading_ms"));
        assert!(stats
            .fields
            .iter()
            .any(|field| field.key == "request_decoding_samples"));
        assert!(stats
            .fields
            .iter()
            .any(|field| field.key == "request_unloading_samples"));
        assert!(stats
            .fields
            .iter()
            .any(|field| field.key == "state_uninstalling"));
        assert!(stats
            .fields
            .iter()
            .any(|field| field.key == "state_unloading"));
    }

    #[test]
    fn publish_snapshot_reports_load_backpressure_when_deferred_count_increases() {
        let diagnostics = Diagnostics::new();
        let mut state = AssetDiagnosticsState::default();
        let mut snapshot = AssetDiagnosticsSnapshot::default();
        snapshot.stats.load_worker_threads = 2;
        snapshot.stats.running_load_jobs = 2;
        snapshot.stats.queued_load_jobs = 4;
        snapshot.stats.load_queue_capacity = 4;
        snapshot.stats.inflight_loads = 6;
        snapshot.stats.deferred_load_submissions = 3;

        state.publish_snapshot(&diagnostics, Some(12), snapshot.clone());
        state.publish_snapshot(&diagnostics, Some(13), snapshot.clone());
        snapshot.stats.deferred_load_submissions = 5;
        state.publish_snapshot(&diagnostics, Some(14), snapshot);

        let events = diagnostics.events();
        let backpressure = events
            .iter()
            .filter(|event| event.code == "asset.load.backpressure")
            .collect::<Vec<_>>();
        assert_eq!(backpressure.len(), 2);
        assert_eq!(backpressure[0].frame, Some(12));
        assert!(backpressure[0]
            .fields
            .iter()
            .any(|field| field.key == "deferred_delta" && field.value == "3"));
        assert!(backpressure[0]
            .fields
            .iter()
            .any(|field| field.key == "running_load_jobs" && field.value == "2"));
        assert!(backpressure[0]
            .fields
            .iter()
            .any(|field| field.key == "load_queue_capacity" && field.value == "4"));
        assert_eq!(backpressure[1].frame, Some(14));
        assert!(backpressure[1]
            .fields
            .iter()
            .any(|field| field.key == "deferred_delta" && field.value == "2"));
    }

    #[test]
    fn publish_snapshot_reports_provider_resolve_failures_when_error_count_increases() {
        let diagnostics = Diagnostics::new();
        let mut state = AssetDiagnosticsState::default();
        let mut snapshot = AssetDiagnosticsSnapshot {
            stats: AssetStats {
                provider: AssetProviderStats {
                    package_roots: 1,
                    package_files: 2,
                    cached_bundle_indexes: 3,
                    cached_bundle_index_entries: 5,
                    resolved_raw_sources: 7,
                    resolved_cooked_sources: 11,
                    resolved_package_sources: 13,
                    resolved_bundle_sources: 17,
                    resolve_errors: 2,
                    ..AssetProviderStats::default()
                },
                ..AssetStats::default()
            },
            ..AssetDiagnosticsSnapshot::default()
        };

        state.publish_snapshot(&diagnostics, Some(20), snapshot.clone());
        state.publish_snapshot(&diagnostics, Some(21), snapshot.clone());
        snapshot.stats.provider.resolve_errors = 5;
        snapshot.stats.provider.resolved_package_sources = 19;
        state.publish_snapshot(&diagnostics, Some(22), snapshot);

        let events = diagnostics.events();
        let failures = events
            .iter()
            .filter(|event| event.code == "asset.provider.resolve.failed")
            .collect::<Vec<_>>();
        assert_eq!(failures.len(), 2);
        assert_eq!(failures[0].frame, Some(20));
        assert!(failures[0]
            .fields
            .iter()
            .any(|field| field.key == "resolve_error_delta" && field.value == "2"));
        assert!(failures[0]
            .fields
            .iter()
            .any(|field| field.key == "provider_cached_bundle_indexes" && field.value == "3"));
        assert!(failures[0]
            .fields
            .iter()
            .any(|field| field.key == "provider_resolved_bundle_sources" && field.value == "17"));
        assert_eq!(failures[1].frame, Some(22));
        assert!(failures[1]
            .fields
            .iter()
            .any(|field| field.key == "resolve_error_delta" && field.value == "3"));
        assert!(failures[1]
            .fields
            .iter()
            .any(|field| field.key == "provider_resolved_package_sources" && field.value == "19"));
    }

    #[test]
    fn publish_snapshot_reports_provider_cache_invalidations_when_counts_increase() {
        let diagnostics = Diagnostics::new();
        let mut state = AssetDiagnosticsState::default();
        let mut snapshot = AssetDiagnosticsSnapshot {
            stats: AssetStats {
                provider: AssetProviderStats {
                    package_roots: 1,
                    package_files: 2,
                    cached_bundle_indexes: 3,
                    cached_bundle_index_entries: 5,
                    cache_invalidations: 1,
                    ..AssetProviderStats::default()
                },
                ..AssetStats::default()
            },
            ..AssetDiagnosticsSnapshot::default()
        };

        state.publish_snapshot(&diagnostics, Some(30), snapshot.clone());
        state.publish_snapshot(&diagnostics, Some(31), snapshot.clone());
        snapshot.stats.provider.cache_invalidations = 3;
        snapshot.stats.provider.full_cache_invalidations = 1;
        state.publish_snapshot(&diagnostics, Some(32), snapshot);

        let events = diagnostics.events();
        let invalidations = events
            .iter()
            .filter(|event| event.code == "asset.provider.cache.invalidated")
            .collect::<Vec<_>>();
        assert_eq!(invalidations.len(), 2);
        assert_eq!(invalidations[0].frame, Some(30));
        assert!(invalidations[0]
            .fields
            .iter()
            .any(|field| field.key == "cache_invalidation_delta" && field.value == "1"));
        assert!(invalidations[0]
            .fields
            .iter()
            .any(|field| field.key == "full_cache_invalidation_delta" && field.value == "0"));
        assert!(
            invalidations[0]
                .fields
                .iter()
                .any(|field| field.key == "provider_cached_bundle_index_entries"
                    && field.value == "5")
        );
        assert_eq!(invalidations[1].frame, Some(32));
        assert!(invalidations[1]
            .fields
            .iter()
            .any(|field| field.key == "cache_invalidation_delta" && field.value == "2"));
        assert!(invalidations[1]
            .fields
            .iter()
            .any(|field| field.key == "full_cache_invalidation_delta" && field.value == "1"));
    }

    #[test]
    fn publish_snapshot_reports_slow_queued_load_job() {
        let diagnostics = Diagnostics::new();
        let mut state = AssetDiagnosticsState::default();
        let snapshot = AssetDiagnosticsSnapshot {
            stats: AssetStats {
                load_worker_threads: 1,
                running_load_jobs: 1,
                queued_load_jobs: 2,
                load_queue_capacity: 2,
                inflight_loads: 3,
                oldest_queued_load_job_age: Some(Duration::from_secs(3)),
                source_load_phases: AssetSourceLoadPhaseCounts {
                    queued: 2,
                    reading: 1,
                    decoding: 0,
                },
                ..AssetStats::default()
            },
            ..AssetDiagnosticsSnapshot::default()
        };

        state.publish_snapshot(&diagnostics, Some(20), snapshot.clone());
        state.publish_snapshot(&diagnostics, Some(21), snapshot);

        let events = diagnostics.events();
        let slow = events
            .iter()
            .filter(|event| event.code == "asset.load.queue.slow")
            .collect::<Vec<_>>();
        assert_eq!(slow.len(), 1);
        assert_eq!(slow[0].frame, Some(20));
        assert!(slow[0]
            .fields
            .iter()
            .any(|field| field.key == "oldest_queued_load_job_ms" && field.value == "3000"));
        assert!(slow[0]
            .fields
            .iter()
            .any(|field| field.key == "source_loads_queued" && field.value == "2"));
        assert!(slow[0]
            .fields
            .iter()
            .any(|field| field.key == "source_loads_reading" && field.value == "1"));
    }

    #[test]
    fn publish_snapshot_reports_slow_lifecycle_state() {
        let diagnostics = Diagnostics::new();
        let mut state = AssetDiagnosticsState::default();
        let snapshot = AssetDiagnosticsSnapshot {
            stats: AssetStats {
                active_requests: 1,
                inflight_loads: 1,
                oldest_active_request_age: Some(Duration::from_secs(2)),
                states: AssetStateCounts {
                    waiting_dependencies: 2,
                    installing: 1,
                    ..AssetStateCounts::default()
                },
                active_state_ages: AssetActiveStateAgeStats {
                    waiting_dependencies: Some(Duration::from_secs(4)),
                    installing: Some(Duration::from_secs(3)),
                    ..AssetActiveStateAgeStats::default()
                },
                ..AssetStats::default()
            },
            ..AssetDiagnosticsSnapshot::default()
        };

        state.publish_snapshot(&diagnostics, Some(30), snapshot.clone());
        state.publish_snapshot(&diagnostics, Some(31), snapshot);

        let events = diagnostics.events();
        let slow = events
            .iter()
            .filter(|event| event.code == "asset.state.slow")
            .collect::<Vec<_>>();
        assert_eq!(slow.len(), 1);
        assert_eq!(slow[0].frame, Some(30));
        assert!(slow[0]
            .fields
            .iter()
            .any(|field| field.key == "state" && field.value == "WaitingDependencies"));
        assert!(slow[0]
            .fields
            .iter()
            .any(|field| field.key == "state_age_ms" && field.value == "4000"));
        assert!(slow[0]
            .fields
            .iter()
            .any(|field| field.key == "state_records" && field.value == "2"));
        assert!(slow[0]
            .fields
            .iter()
            .any(|field| field.key == "active_requests" && field.value == "1"));
    }

    #[test]
    fn publish_snapshot_reports_distinct_slow_queued_requests() {
        let diagnostics = Diagnostics::new();
        let first = AssetId::new();
        let second = AssetId::new();
        let mut state = AssetDiagnosticsState::default();
        let mut snapshot = AssetDiagnosticsSnapshot {
            queued_requests: vec![AssetRequestSnapshot {
                request_id: 10,
                asset_id: first,
                generation: 1,
                priority: 3,
                status: AssetRequestStatus::Queued,
                failure_phase: None,
                progress: AssetRequestProgress::new(0, 6, "queued"),
                queued_age: Duration::from_secs(3),
                active_age: None,
                phase_age: Duration::from_secs(3),
                dependency_blockers: Vec::new(),
                dependency_blocker_details: Vec::new(),
                dependency_cycle: Vec::new(),
                last_error: None,
            }],
            ..AssetDiagnosticsSnapshot::default()
        };

        state.publish_snapshot(&diagnostics, Some(40), snapshot.clone());
        state.publish_snapshot(&diagnostics, Some(41), snapshot.clone());
        snapshot.queued_requests[0].request_id = 11;
        snapshot.queued_requests[0].asset_id = second;
        snapshot.queued_requests[0].generation = 2;
        state.publish_snapshot(&diagnostics, Some(42), snapshot);

        let events = diagnostics.events();
        let slow = events
            .iter()
            .filter(|event| event.code == "asset.queue.slow")
            .collect::<Vec<_>>();
        assert_eq!(slow.len(), 2);
        assert_eq!(slow[0].frame, Some(40));
        assert_eq!(slow[1].frame, Some(42));
        assert!(slow[0]
            .fields
            .iter()
            .any(|field| field.key == "request_id" && field.value == "10"));
        assert!(slow[1]
            .fields
            .iter()
            .any(|field| field.key == "request_id" && field.value == "11"));
        assert!(slow[1]
            .fields
            .iter()
            .any(|field| field.key == "generation" && field.value == "2"));
    }

    #[test]
    fn publish_snapshot_reports_reload_explanation_fields() {
        let diagnostics = Diagnostics::new();
        let changed = AssetId::new();
        let dependent = AssetId::new();
        let unchanged = AssetId::new();
        let missing = AssetId::new();
        let untracked = AssetId::new();
        let mut state = AssetDiagnosticsState::default();
        let snapshot = AssetDiagnosticsSnapshot {
            last_reload_report: AssetReloadReport {
                changed_roots: vec![changed],
                impacted: vec![changed, dependent],
                skipped: vec![
                    AssetReloadSkipped {
                        asset_id: unchanged,
                        reason: AssetReloadSkipReason::Unchanged,
                    },
                    AssetReloadSkipped {
                        asset_id: missing,
                        reason: AssetReloadSkipReason::MissingManifestEntry,
                    },
                    AssetReloadSkipped {
                        asset_id: untracked,
                        reason: AssetReloadSkipReason::UntrackedRecord,
                    },
                ],
            },
            ..AssetDiagnosticsSnapshot::default()
        };

        state.publish_snapshot(&diagnostics, Some(17), snapshot);

        let events = diagnostics.events();
        let reload = events
            .iter()
            .find(|event| event.code == "asset.reload")
            .expect("reload event");
        assert_eq!(reload.frame, Some(17));
        assert!(reload
            .fields
            .iter()
            .any(|field| field.key == "changed_roots" && field.value == "1"));
        assert!(reload
            .fields
            .iter()
            .any(|field| field.key == "impacted" && field.value == "2"));
        assert!(reload
            .fields
            .iter()
            .any(|field| field.key == "skipped_unchanged" && field.value == "1"));
        assert!(reload
            .fields
            .iter()
            .any(|field| field.key == "skipped_missing_manifest" && field.value == "1"));
        assert!(reload
            .fields
            .iter()
            .any(|field| field.key == "skipped_untracked" && field.value == "1"));
        let skipped_details = reload
            .fields
            .iter()
            .find(|field| field.key == "skipped_details")
            .expect("skipped details field");
        assert!(skipped_details.value.contains("Unchanged"));
        assert!(skipped_details.value.contains("MissingManifestEntry"));
        assert!(skipped_details.value.contains("UntrackedRecord"));
    }

    #[test]
    fn publish_snapshot_reports_reload_status_changes() {
        let diagnostics = Diagnostics::new();
        let pending = AssetId::new();
        let changed = AssetId::new();
        let mut state = AssetDiagnosticsState::default();
        let mut snapshot = AssetDiagnosticsSnapshot {
            reload_status: AssetReloadStatus {
                auto_reload_enabled: true,
                file_watcher_enabled: true,
                auto_reload_frozen: true,
                pending_roots: vec![pending],
                pending_age: Some(Duration::from_millis(42)),
                last_report: AssetReloadReport {
                    changed_roots: vec![changed],
                    impacted: vec![changed],
                    skipped: Vec::new(),
                },
            },
            ..AssetDiagnosticsSnapshot::default()
        };

        state.publish_snapshot(&diagnostics, Some(21), snapshot.clone());
        snapshot.reload_status.pending_age = Some(Duration::from_millis(99));
        state.publish_snapshot(&diagnostics, Some(22), snapshot.clone());
        snapshot.reload_status.pending_roots.push(changed);
        state.publish_snapshot(&diagnostics, Some(23), snapshot);

        let events = diagnostics.events();
        let statuses = events
            .iter()
            .filter(|event| event.code == "asset.reload.status")
            .collect::<Vec<_>>();
        assert_eq!(statuses.len(), 2);
        assert_eq!(statuses[0].frame, Some(21));
        assert!(statuses[0]
            .fields
            .iter()
            .any(|field| field.key == "auto_reload_enabled" && field.value == "true"));
        assert!(statuses[0]
            .fields
            .iter()
            .any(|field| field.key == "file_watcher_enabled" && field.value == "true"));
        assert!(statuses[0]
            .fields
            .iter()
            .any(|field| field.key == "auto_reload_frozen" && field.value == "true"));
        assert!(statuses[0]
            .fields
            .iter()
            .any(|field| field.key == "pending_roots" && field.value == "1"));
        assert!(statuses[0]
            .fields
            .iter()
            .any(|field| field.key == "pending_age_ms" && field.value == "42"));
        assert!(statuses[0]
            .fields
            .iter()
            .any(|field| field.key == "last_changed_roots" && field.value == "1"));
        assert_eq!(statuses[1].frame, Some(23));
        assert!(statuses[1]
            .fields
            .iter()
            .any(|field| field.key == "pending_roots" && field.value == "2"));
    }

    #[test]
    fn publish_snapshot_reports_slow_active_request_progress() {
        let diagnostics = Diagnostics::new();
        let asset_id = AssetId::new();
        let dependency = AssetId::new();
        let mut state = AssetDiagnosticsState::default();
        let snapshot = AssetDiagnosticsSnapshot {
            active_requests: vec![AssetRequestSnapshot {
                request_id: 42,
                asset_id,
                generation: 3,
                priority: 8,
                status: AssetRequestStatus::WaitingDependencies,
                failure_phase: Some(AssetFailurePhase::Dependency),
                progress: AssetRequestProgress::new(2, 5, "waiting dependencies"),
                queued_age: Duration::from_secs(4),
                active_age: Some(Duration::from_secs(3)),
                phase_age: Duration::from_secs(2),
                dependency_blockers: vec![dependency],
                dependency_blocker_details: vec![AssetDependencyBlocker {
                    asset_id: dependency,
                    reason: AssetDependencyBlockerReason::Waiting,
                    state: Some(AssetState::Loading),
                }],
                dependency_cycle: Vec::new(),
                last_error: Some("dependency still loading".to_string()),
            }],
            ..AssetDiagnosticsSnapshot::default()
        };

        state.publish_snapshot(&diagnostics, Some(9), snapshot);

        let events = diagnostics.events();
        let slow = events
            .iter()
            .find(|event| event.code == "asset.request.slow")
            .expect("slow active request event");
        assert_eq!(slow.frame, Some(9));
        assert!(slow
            .fields
            .iter()
            .any(|field| field.key == "progress_label" && field.value == "waiting dependencies"));
        assert!(slow
            .fields
            .iter()
            .any(|field| field.key == "failure_phase" && field.value == "Dependency"));
        assert!(slow
            .fields
            .iter()
            .any(|field| field.key == "progress_percent" && field.value == "40"));
        assert!(slow
            .fields
            .iter()
            .any(|field| field.key == "phase_ms" && field.value == "2000"));
        assert!(slow
            .fields
            .iter()
            .any(|field| field.key == "dependency_blockers" && field.value == "1"));
        assert!(slow
            .fields
            .iter()
            .any(|field| field.key == "dependency_waiting_blockers" && field.value == "1"));
    }

    #[test]
    fn publish_snapshot_reports_recent_failed_request() {
        let diagnostics = Diagnostics::new();
        let asset_id = AssetId::new();
        let missing = AssetId::new();
        let failed_dependency = AssetId::new();
        let mut state = AssetDiagnosticsState::default();
        let snapshot = AssetDiagnosticsSnapshot {
            failed_requests: vec![AssetRequestSnapshot {
                request_id: 77,
                asset_id,
                generation: 5,
                priority: -2,
                status: AssetRequestStatus::Failed,
                failure_phase: Some(AssetFailurePhase::Decode),
                progress: AssetRequestProgress::new(0, 5, "failed"),
                queued_age: Duration::from_millis(12),
                active_age: Some(Duration::from_millis(9)),
                phase_age: Duration::from_millis(4),
                dependency_blockers: vec![missing, failed_dependency],
                dependency_blocker_details: vec![
                    AssetDependencyBlocker {
                        asset_id: missing,
                        reason: AssetDependencyBlockerReason::Missing,
                        state: None,
                    },
                    AssetDependencyBlocker {
                        asset_id: failed_dependency,
                        reason: AssetDependencyBlockerReason::Failed,
                        state: Some(AssetState::Failed),
                    },
                ],
                dependency_cycle: vec![asset_id, failed_dependency, asset_id],
                last_error: Some("decode exploded".to_string()),
            }],
            ..AssetDiagnosticsSnapshot::default()
        };

        state.publish_snapshot(&diagnostics, Some(11), snapshot);

        let events = diagnostics.events();
        let failed = events
            .iter()
            .find(|event| event.code == "asset.request.failed")
            .expect("failed request event");
        assert_eq!(failed.frame, Some(11));
        assert!(failed
            .fields
            .iter()
            .any(|field| field.key == "request_id" && field.value == "77"));
        assert!(failed
            .fields
            .iter()
            .any(|field| field.key == "failure_phase" && field.value == "Decode"));
        assert!(failed
            .fields
            .iter()
            .any(|field| field.key == "dependency_blockers" && field.value == "2"));
        assert!(failed
            .fields
            .iter()
            .any(|field| field.key == "dependency_missing_blockers" && field.value == "1"));
        assert!(failed
            .fields
            .iter()
            .any(|field| field.key == "dependency_failed_blockers" && field.value == "1"));
        assert!(failed
            .fields
            .iter()
            .any(|field| field.key == "dependency_cycle_length" && field.value == "3"));
        assert!(failed
            .fields
            .iter()
            .any(|field| field.key == "last_error" && field.value == "decode exploded"));
    }

    #[test]
    fn publish_snapshot_reports_recent_canceled_request() {
        let diagnostics = Diagnostics::new();
        let asset_id = AssetId::new();
        let mut state = AssetDiagnosticsState::default();
        let snapshot = AssetDiagnosticsSnapshot {
            canceled_requests: vec![AssetRequestSnapshot {
                request_id: 88,
                asset_id,
                generation: 2,
                priority: 12,
                status: AssetRequestStatus::Canceled,
                failure_phase: None,
                progress: AssetRequestProgress::new(0, 6, "canceled"),
                queued_age: Duration::from_millis(17),
                active_age: None,
                phase_age: Duration::from_millis(17),
                dependency_blockers: Vec::new(),
                dependency_blocker_details: Vec::new(),
                dependency_cycle: Vec::new(),
                last_error: None,
            }],
            ..AssetDiagnosticsSnapshot::default()
        };

        state.publish_snapshot(&diagnostics, Some(19), snapshot.clone());
        state.publish_snapshot(&diagnostics, Some(20), snapshot);

        let events = diagnostics.events();
        let canceled = events
            .iter()
            .filter(|event| event.code == "asset.request.canceled")
            .collect::<Vec<_>>();
        assert_eq!(canceled.len(), 1);
        assert_eq!(canceled[0].frame, Some(19));
        assert!(canceled[0]
            .fields
            .iter()
            .any(|field| field.key == "request_id" && field.value == "88"));
        assert!(canceled[0]
            .fields
            .iter()
            .any(|field| field.key == "status" && field.value == "Canceled"));
        assert!(canceled[0]
            .fields
            .iter()
            .any(|field| field.key == "queued_ms" && field.value == "17"));
    }
}
