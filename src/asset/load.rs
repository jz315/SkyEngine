use std::any::Any;
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU8, Ordering as AtomicOrdering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::events::{self, AssetEventLog};
use super::failure;
use super::font::FontAsset;
use super::io::{AssetIoCancelToken, AssetIoPriority, AssetIoService, AssetIoSubmitError};
use super::provider::{self, AssetProvider, AssetSourceLocation, ResolvedAssetSource};
use super::registry::{AssetFactories, AssetRegistry, ErasedAssetFactory};
use super::request::AssetRequests;
use super::store::{AssetDependencyLeaseUpdate, AssetStore};
use super::texture::{decode_texture_source_bytes, TextureAsset, TextureColorSpace};
use super::types::{
    Asset, AssetConfig, AssetError, AssetFailurePhase, AssetId, AssetLoadContext,
    AssetLoadTimingStats, AssetManifestEntry, AssetSourceLoadPhaseCounts, LoadedAsset,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AssetSourceLoadPhase {
    Queued,
    Reading,
    Decoding,
}

#[derive(Clone, Debug)]
pub(crate) struct AssetLoadPhaseTracker {
    phase: Arc<AtomicU8>,
}

impl AssetLoadPhaseTracker {
    const QUEUED: u8 = 0;
    const READING: u8 = 1;
    const DECODING: u8 = 2;

    fn new() -> Self {
        Self {
            phase: Arc::new(AtomicU8::new(Self::QUEUED)),
        }
    }

    pub(crate) fn set(&self, phase: AssetSourceLoadPhase) {
        self.phase.store(phase.as_u8(), AtomicOrdering::Release);
    }

    pub(crate) fn phase(&self) -> AssetSourceLoadPhase {
        AssetSourceLoadPhase::from_u8(self.phase.load(AtomicOrdering::Acquire))
    }
}

impl AssetSourceLoadPhase {
    fn as_u8(self) -> u8 {
        match self {
            Self::Queued => AssetLoadPhaseTracker::QUEUED,
            Self::Reading => AssetLoadPhaseTracker::READING,
            Self::Decoding => AssetLoadPhaseTracker::DECODING,
        }
    }

    fn from_u8(value: u8) -> Self {
        match value {
            AssetLoadPhaseTracker::READING => Self::Reading,
            AssetLoadPhaseTracker::DECODING => Self::Decoding,
            _ => Self::Queued,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct AssetLoadTimingSample {
    sampled: bool,
    pub(crate) read_time: Duration,
    pub(crate) decode_time: Duration,
    pub(crate) total_time: Duration,
}

#[derive(Debug)]
pub(crate) struct TimedAssetLoadError {
    pub(crate) error: AssetError,
    pub(crate) timings: AssetLoadTimingSample,
}

impl From<AssetError> for TimedAssetLoadError {
    fn from(error: AssetError) -> Self {
        Self {
            error,
            timings: AssetLoadTimingSample::default(),
        }
    }
}

pub(crate) struct LoadedSourceAsset {
    pub(crate) loaded: LoadedAsset<Arc<dyn Any + Send + Sync>>,
    pub(crate) content_hash: String,
    pub(crate) timings: AssetLoadTimingSample,
}

pub(crate) struct CompletedLoad {
    pub(crate) id: AssetId,
    pub(crate) generation: u64,
    pub(crate) entry: AssetManifestEntry,
    pub(crate) cooked_hash: Option<String>,
    pub(crate) timings: AssetLoadTimingSample,
    pub(crate) result: Result<LoadedAsset<Arc<dyn Any + Send + Sync>>, AssetError>,
}

#[derive(Debug)]
pub(crate) struct CompletedLoadFailure {
    pub(crate) id: AssetId,
    pub(crate) error: AssetError,
    pub(crate) phase: AssetFailurePhase,
}

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
                result: Err(failure.error),
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
    .map_err(|error| TimedAssetLoadError { error, timings })?;
    let dependency_update = store
        .finish_loaded_record(
            id,
            prepared.loaded,
            prepared.dependencies,
            prepared.entry_fingerprint,
            prepared.content_hash,
        )
        .ok_or(AssetError::AssetNotFound { id })
        .map_err(|error| TimedAssetLoadError { error, timings })?;
    Ok(LoadedRecordNow {
        dependency_update,
        timings,
    })
}

pub(crate) fn finish_completed_load(
    store: &mut AssetStore,
    completion: CompletedLoad,
) -> Result<Option<AssetDependencyLeaseUpdate>, CompletedLoadFailure> {
    if !store.accepts_load_completion(completion.id, completion.generation) {
        return Ok(None);
    }

    match completion.result {
        Ok(loaded) => {
            let prepared = prepare_loaded_asset(&completion.entry, loaded, completion.cooked_hash)
                .map_err(|error| CompletedLoadFailure {
                    id: completion.id,
                    phase: AssetFailurePhase::from_error(&error),
                    error,
                })?;
            let update = store
                .finish_loaded_record(
                    completion.id,
                    prepared.loaded,
                    prepared.dependencies,
                    prepared.entry_fingerprint,
                    prepared.content_hash,
                )
                .expect("record should exist after stale check");
            Ok(Some(update))
        }
        Err(error) => Err(CompletedLoadFailure {
            id: completion.id,
            phase: AssetFailurePhase::from_error(&error),
            error,
        }),
    }
}

pub(crate) fn apply_completed_load(
    store: &mut AssetStore,
    events: &mut AssetEventLog,
    load_queue: &mut AssetLoadQueue,
    manifest: &impl AssetRegistry,
    completion: CompletedLoad,
    dependency_priority: i32,
) -> Result<bool, CompletedLoadFailure> {
    let id = completion.id;
    match finish_completed_load(store, completion) {
        Ok(Some(update)) => {
            apply_loaded_dependency_update(
                store,
                events,
                load_queue,
                manifest,
                update,
                dependency_priority,
            );
            events::push_loaded_event(events, store, id);
            Ok(true)
        }
        Ok(None) => Ok(false),
        Err(failure) => Err(failure),
    }
}

pub(crate) fn drain_completed_loads(
    store: &mut AssetStore,
    events: &mut AssetEventLog,
    load_queue: &mut AssetLoadQueue,
    manifest: &impl AssetRegistry,
    dependency_priority: i32,
) -> Result<usize, CompletedLoadFailure> {
    let mut completed = 0usize;
    for completion in load_queue.drain_ready() {
        if apply_completed_load(
            store,
            events,
            load_queue,
            manifest,
            completion,
            dependency_priority,
        )? {
            completed += 1;
        }
    }
    Ok(completed)
}

pub(crate) fn drain_completed_loads_or_fail(
    store: &mut AssetStore,
    events: &mut AssetEventLog,
    requests: &mut AssetRequests,
    load_queue: &mut AssetLoadQueue,
    manifest: &impl AssetRegistry,
    dependency_priority: i32,
    failed_at: Instant,
) -> Result<usize, AssetError> {
    match drain_completed_loads(store, events, load_queue, manifest, dependency_priority) {
        Ok(completed) => Ok(completed),
        Err(load_failure) => {
            failure::fail_record_and_request(
                store,
                events,
                requests,
                load_failure.id,
                load_failure.error.clone(),
                load_failure.phase,
                dependency_priority,
                failed_at,
                |dependency| manifest.asset_type(dependency),
            );
            Err(load_failure.error)
        }
    }
}

pub(crate) fn load_record_now_and_apply(
    config: &AssetConfig,
    provider: &dyn AssetProvider,
    manifest: &impl AssetRegistry,
    factories: &AssetFactories,
    store: &mut AssetStore,
    events: &mut AssetEventLog,
    load_queue: &mut AssetLoadQueue,
    id: AssetId,
) -> Result<(), TimedAssetLoadError> {
    match load_record_now(config, provider, manifest, factories, store, id) {
        Ok(loaded) => {
            load_queue.record_timing(loaded.timings, true);
            apply_loaded_dependency_update(
                store,
                events,
                load_queue,
                manifest,
                loaded.dependency_update,
                config.io_default_priority,
            );
            events::push_loaded_event(events, store, id);
            Ok(())
        }
        Err(failure) => {
            load_queue.record_timing(failure.timings, false);
            Err(failure)
        }
    }
}

pub(crate) fn load_record_now_and_apply_or_fail(
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
) -> Result<(), AssetError> {
    match load_record_now_and_apply(
        config, provider, manifest, factories, store, events, load_queue, id,
    ) {
        Ok(()) => Ok(()),
        Err(load_failure) => {
            let error = load_failure.error;
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

fn apply_loaded_dependency_update(
    store: &mut AssetStore,
    events: &mut AssetEventLog,
    load_queue: &mut AssetLoadQueue,
    manifest: &impl AssetRegistry,
    update: AssetDependencyLeaseUpdate,
    dependency_priority: i32,
) {
    let release = store.apply_dependency_lease_update(update, dependency_priority, |dependency| {
        manifest.asset_type(dependency)
    });
    load_queue.cancel_non_loading(store);
    events::push_release_events(events, store, release);
}

pub(crate) fn load_resolved_source_asset(
    id: AssetId,
    source: &ResolvedAssetSource,
    factory: &dyn ErasedAssetFactory,
    asset_root: &Path,
    cooked_root: &Path,
) -> Result<LoadedSourceAsset, TimedAssetLoadError> {
    load_resolved_source_asset_inner(id, source, factory, asset_root, cooked_root, None)
}

fn load_resolved_source_asset_with_phase(
    id: AssetId,
    source: &ResolvedAssetSource,
    factory: &dyn ErasedAssetFactory,
    asset_root: &Path,
    cooked_root: &Path,
    phase: &AssetLoadPhaseTracker,
) -> Result<LoadedSourceAsset, TimedAssetLoadError> {
    load_resolved_source_asset_inner(id, source, factory, asset_root, cooked_root, Some(phase))
}

fn load_resolved_source_asset_inner(
    id: AssetId,
    source: &ResolvedAssetSource,
    factory: &dyn ErasedAssetFactory,
    asset_root: &Path,
    cooked_root: &Path,
    phase: Option<&AssetLoadPhaseTracker>,
) -> Result<LoadedSourceAsset, TimedAssetLoadError> {
    validate_runtime_cooked_schema(factory, source.entry(), source.location())?;

    let total_start = Instant::now();
    let read_start = Instant::now();
    if let Some(phase) = phase {
        phase.set(AssetSourceLoadPhase::Reading);
    }
    let bytes = source.read_bytes(id).map_err(|error| TimedAssetLoadError {
        error,
        timings: AssetLoadTimingSample {
            sampled: true,
            read_time: read_start.elapsed(),
            decode_time: Duration::ZERO,
            total_time: total_start.elapsed(),
        },
    })?;
    let read_time = read_start.elapsed();
    let content_hash = hash_bytes(&bytes);
    let decode_start = Instant::now();
    if let Some(phase) = phase {
        phase.set(AssetSourceLoadPhase::Decoding);
    }
    let loaded = match source.location() {
        AssetSourceLocation::Raw(path) => load_raw_source_asset(source.entry(), path, &bytes),
        AssetSourceLocation::Cooked(_)
        | AssetSourceLocation::Package(_)
        | AssetSourceLocation::Bundle { .. } => factory.load(AssetLoadContext {
            asset_id: id,
            entry: source.entry(),
            bytes: &bytes,
            asset_root,
            cooked_root,
        }),
        #[cfg(test)]
        AssetSourceLocation::Memory { .. } => factory.load(AssetLoadContext {
            asset_id: id,
            entry: source.entry(),
            bytes: &bytes,
            asset_root,
            cooked_root,
        }),
    };
    let timings = AssetLoadTimingSample {
        sampled: true,
        read_time,
        decode_time: decode_start.elapsed(),
        total_time: total_start.elapsed(),
    };
    match loaded {
        Ok(loaded) => Ok(LoadedSourceAsset {
            loaded,
            content_hash,
            timings,
        }),
        Err(error) => Err(TimedAssetLoadError { error, timings }),
    }
}

fn validate_runtime_cooked_schema(
    factory: &dyn ErasedAssetFactory,
    entry: &AssetManifestEntry,
    location: &AssetSourceLocation,
) -> Result<(), AssetError> {
    if matches!(location, AssetSourceLocation::Raw(_)) {
        return Ok(());
    }

    let Some(schema) = factory.cooked_schema() else {
        return Ok(());
    };

    if entry.cooker == schema.cooker && entry.version == schema.version {
        return Ok(());
    }

    Err(AssetError::CookedSchemaMismatch {
        id: entry.asset_id,
        expected_cooker: schema.cooker.to_string(),
        expected_version: schema.version,
        actual_cooker: entry.cooker.clone(),
        actual_version: entry.version,
    })
}

pub(crate) fn manifest_entry_fingerprint(entry: &AssetManifestEntry) -> Result<String, AssetError> {
    let fingerprint = serde_json::json!({
        "asset_id": entry.asset_id.to_string(),
        "asset_type": entry.asset_type,
        "importer": entry.importer,
        "cooker": entry.cooker,
        "version": entry.version,
        "source_path": entry.source_path,
        "cooked_path": entry.cooked_path,
        "dependencies": entry
            .dependencies
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        "import_settings": entry.import_settings,
    });
    let bytes = serde_json::to_vec(&fingerprint).map_err(|error| AssetError::Internal {
        message: format!("failed to serialize asset manifest fingerprint: {error}"),
    })?;
    Ok(hash_bytes(&bytes))
}

pub(crate) fn hash_bytes(bytes: &[u8]) -> String {
    const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;

    let mut hash = FNV_OFFSET_BASIS;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    format!("{hash:016x}")
}

fn normalize_dependencies(dependencies: Vec<AssetId>) -> Vec<AssetId> {
    let mut unique = Vec::with_capacity(dependencies.len());
    for dependency in dependencies {
        if !unique.contains(&dependency) {
            unique.push(dependency);
        }
    }
    unique
}

fn load_raw_source_asset(
    entry: &AssetManifestEntry,
    raw_source_path: &Path,
    bytes: &[u8],
) -> Result<LoadedAsset<Arc<dyn Any + Send + Sync>>, AssetError> {
    if entry.asset_type == TextureAsset::TYPE {
        let texture = decode_texture_source_bytes(raw_source_path, bytes, TextureColorSpace::Srgb)?;
        return Ok(LoadedAsset::new(
            Arc::new(texture) as Arc<dyn Any + Send + Sync>
        ));
    }
    if entry.asset_type == FontAsset::TYPE {
        return Ok(LoadedAsset::new(
            Arc::new(FontAsset::new(Arc::<[u8]>::from(bytes.to_vec())))
                as Arc<dyn Any + Send + Sync>,
        ));
    }
    Err(AssetError::Unsupported {
        message: format!(
            "raw source loading is not supported for asset type `{}`",
            entry.asset_type
        ),
    })
}

#[derive(Clone, Debug, Default)]
struct AssetLoadTimingAccumulator {
    completed_source_loads: usize,
    failed_source_loads: usize,
    total_read_time: Duration,
    total_decode_time: Duration,
    total_load_time: Duration,
}

impl AssetLoadTimingAccumulator {
    fn record(&mut self, sample: AssetLoadTimingSample, succeeded: bool) {
        if !sample.sampled {
            return;
        }
        if succeeded {
            self.completed_source_loads += 1;
        } else {
            self.failed_source_loads += 1;
        }
        self.total_read_time += sample.read_time;
        self.total_decode_time += sample.decode_time;
        self.total_load_time += sample.total_time;
    }

    fn stats(&self) -> AssetLoadTimingStats {
        let samples = self.completed_source_loads + self.failed_source_loads;
        AssetLoadTimingStats {
            completed_source_loads: self.completed_source_loads,
            failed_source_loads: self.failed_source_loads,
            average_read_time: average_duration(self.total_read_time, samples),
            average_decode_time: average_duration(self.total_decode_time, samples),
            average_total_time: average_duration(self.total_load_time, samples),
        }
    }
}

fn average_duration(total: Duration, samples: usize) -> Option<Duration> {
    if samples == 0 {
        return None;
    }
    Some(Duration::from_secs_f64(
        total.as_secs_f64() / samples as f64,
    ))
}

pub(crate) struct AssetLoadQueue {
    load_tx: Sender<CompletedLoad>,
    load_rx: Receiver<CompletedLoad>,
    io: AssetIoService,
    inflight_loads: HashMap<(AssetId, u64), InflightLoad>,
    timings: AssetLoadTimingAccumulator,
    deferred_submissions: usize,
}

struct InflightLoad {
    cancel_token: AssetIoCancelToken,
    phase: AssetLoadPhaseTracker,
}

pub(crate) fn should_load_record_in_background(
    config: &AssetConfig,
    store: &AssetStore,
    id: AssetId,
) -> bool {
    config.background_loading || store.prefers_background_load(id)
}

pub(crate) fn has_current_inflight_load(
    store: &AssetStore,
    load_queue: &AssetLoadQueue,
    id: AssetId,
) -> bool {
    store
        .load_generation(id)
        .is_some_and(|generation| load_queue.contains(id, generation))
}

impl AssetLoadQueue {
    #[cfg(test)]
    pub(crate) fn new(worker_threads: usize, queue_capacity: usize) -> Self {
        Self::with_shutdown_timeout(worker_threads, queue_capacity, None)
    }

    pub(crate) fn with_shutdown_timeout(
        worker_threads: usize,
        queue_capacity: usize,
        shutdown_timeout: Option<Duration>,
    ) -> Self {
        let (load_tx, load_rx) = mpsc::channel();
        Self {
            load_tx,
            load_rx,
            io: AssetIoService::with_shutdown_timeout(
                worker_threads,
                queue_capacity,
                shutdown_timeout,
            ),
            inflight_loads: HashMap::default(),
            timings: AssetLoadTimingAccumulator::default(),
            deferred_submissions: 0,
        }
    }

    pub(crate) fn inflight_len(&self) -> usize {
        self.inflight_loads.len()
    }

    pub(crate) fn queued_len(&self) -> usize {
        self.io.queued_len()
    }

    pub(crate) fn oldest_queued_age(&self, now: Instant) -> Option<Duration> {
        self.io.oldest_queued_age(now)
    }

    pub(crate) fn source_load_phase_counts(&self) -> AssetSourceLoadPhaseCounts {
        let mut counts = AssetSourceLoadPhaseCounts::default();
        for inflight in self.inflight_loads.values() {
            match inflight.phase.phase() {
                AssetSourceLoadPhase::Queued => counts.queued += 1,
                AssetSourceLoadPhase::Reading => counts.reading += 1,
                AssetSourceLoadPhase::Decoding => counts.decoding += 1,
            }
        }
        counts
    }

    pub(crate) fn running_len(&self) -> usize {
        self.io.running_len()
    }

    pub(crate) fn queue_capacity(&self) -> usize {
        self.io.queue_capacity()
    }

    pub(crate) fn deferred_submission_count(&self) -> usize {
        self.deferred_submissions
    }

    pub(crate) fn timing_stats(&self) -> AssetLoadTimingStats {
        self.timings.stats()
    }

    pub(crate) fn record_timing(&mut self, sample: AssetLoadTimingSample, succeeded: bool) {
        self.timings.record(sample, succeeded);
    }

    pub(crate) fn worker_count(&self) -> usize {
        self.io.worker_count()
    }

    pub(crate) fn contains(&self, id: AssetId, generation: u64) -> bool {
        self.inflight_loads.contains_key(&(id, generation))
    }

    pub(crate) fn phase(&self, id: AssetId, generation: u64) -> Option<AssetSourceLoadPhase> {
        self.inflight_loads
            .get(&(id, generation))
            .map(|inflight| inflight.phase.phase())
    }

    pub(crate) fn cancel(&mut self, id: AssetId, generation: u64) -> bool {
        let Some(inflight) = self.inflight_loads.remove(&(id, generation)) else {
            return false;
        };
        inflight.cancel_token.cancel();
        true
    }

    pub(crate) fn cancel_non_loading(&mut self, store: &AssetStore) -> usize {
        let stale = self
            .inflight_loads
            .keys()
            .copied()
            .filter(|(id, generation)| store.should_cancel_load(*id, *generation))
            .collect::<Vec<_>>();
        let count = stale.len();
        for (id, generation) in stale {
            self.cancel(id, generation);
        }
        count
    }

    #[cfg(test)]
    pub(crate) fn submit(
        &mut self,
        id: AssetId,
        generation: u64,
        priority: i32,
        job: impl FnOnce() -> CompletedLoad + Send + 'static,
    ) -> Result<bool, AssetError> {
        self.submit_with_phase(id, generation, priority, move |phase| {
            phase.set(AssetSourceLoadPhase::Reading);
            job()
        })
    }

    pub(crate) fn submit_with_phase(
        &mut self,
        id: AssetId,
        generation: u64,
        priority: i32,
        job: impl FnOnce(AssetLoadPhaseTracker) -> CompletedLoad + Send + 'static,
    ) -> Result<bool, AssetError> {
        if self.inflight_loads.contains_key(&(id, generation)) {
            return Ok(false);
        }

        let tx = self.load_tx.clone();
        let phase = AssetLoadPhaseTracker::new();
        let phase_for_job = phase.clone();
        match self
            .io
            .submit_cancelable(AssetIoPriority::new(priority), move || {
                let _ = tx.send(job(phase_for_job));
            }) {
            Ok(cancel_token) => {
                self.inflight_loads.insert(
                    (id, generation),
                    InflightLoad {
                        cancel_token,
                        phase,
                    },
                );
                Ok(true)
            }
            Err(AssetIoSubmitError::QueueFull) => {
                self.deferred_submissions = self.deferred_submissions.saturating_add(1);
                Ok(false)
            }
            Err(AssetIoSubmitError::Closed) => Err(AssetError::Internal {
                message: "asset io service is closed".to_string(),
            }),
        }
    }

    pub(crate) fn drain_ready(&mut self) -> Vec<CompletedLoad> {
        let mut completions = Vec::new();
        loop {
            match self.load_rx.try_recv() {
                Ok(completion) => {
                    self.inflight_loads
                        .remove(&(completion.id, completion.generation));
                    self.record_timing(completion.timings, completion.result.is_ok());
                    completions.push(completion);
                }
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            }
        }
        completions
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::any::TypeId;
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    use crate::asset::install::{AssetInstallContext, AssetInstallResult};
    use crate::asset::provider::{
        raw_font_manifest_entry, raw_texture_manifest_entry, AssetSourceLocation,
        MemoryAssetProvider, ResolvedAssetSource,
    };
    use crate::asset::registry::{AssetRuntimeFactory, ManifestIndex};
    use crate::asset::request::{AssetRequestPhase, AssetRequests};
    use crate::asset::store::AssetRecord;
    use crate::asset::types::{
        AssetEventKind, AssetRegistryManifest, AssetRequestStatus, AssetState, ASSET_SYSTEM_VERSION,
    };

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

        let failure =
            match load_record_now(&config, &provider, &manifest, &factories, &mut store, id) {
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

        assert!(matches!(
            error,
            AssetError::FactoryNotRegistered { asset_type }
                if asset_type == RuntimeTestAsset::TYPE
        ));
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

        assert!(matches!(
            error,
            AssetError::InvalidCookedAsset { id: Some(failed), .. } if failed == id
        ));
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
}
