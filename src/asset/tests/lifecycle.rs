use crate::asset::provider::{
    write_test_bundle, AssetProvider, MemoryAssetProvider, ResolvedAssetSource,
};
use crate::asset::registry::{load_manifest, AssetRegistryLoader, AssetRuntimeFactory};
use crate::asset::{
    Asset, AssetConfig, AssetError, AssetEventKind, AssetFailurePhase, AssetId, AssetInstallBudget,
    AssetInstallContext, AssetInstallPoll, AssetInstallResult, AssetInstallTask, AssetLoadContext,
    AssetManifestEntry, AssetRegistryManifest, AssetReloadSkipReason, AssetRequestStatus,
    AssetState, AssetUninstallContext, Assets, FontAsset, Handle, LoadedAsset, TextureAsset,
    ASSET_SYSTEM_VERSION,
};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tempfile::tempdir;

#[derive(Clone, Debug, PartialEq, Eq)]
struct DummyAsset(String);

impl Asset for DummyAsset {
    const TYPE: &'static str = "dummy";
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct LoadedDummy(String);

struct DummyFactory;

impl AssetRuntimeFactory for DummyFactory {
    type Asset = DummyAsset;
    type Loaded = LoadedDummy;

    fn load(&self, ctx: AssetLoadContext<'_>) -> Result<LoadedAsset<Self::Loaded>, AssetError> {
        let text =
            std::str::from_utf8(ctx.bytes).map_err(|error| AssetError::InvalidCookedAsset {
                id: Some(ctx.asset_id),
                message: error.to_string(),
            })?;
        Ok(LoadedAsset::new(LoadedDummy(text.to_string()))
            .with_dependencies(ctx.entry.dependencies.clone()))
    }

    fn begin_install(
        &self,
        loaded: &Self::Loaded,
        _ctx: AssetInstallContext<'_>,
    ) -> Result<AssetInstallResult<Self::Asset>, AssetError> {
        Ok(AssetInstallResult::Ready(DummyAsset(loaded.0.clone())))
    }
}

struct StaticRegistryLoader {
    manifest: AssetRegistryManifest,
    loads: Arc<AtomicUsize>,
}

impl AssetRegistryLoader for StaticRegistryLoader {
    fn load(&self, _config: &AssetConfig) -> Result<AssetRegistryManifest, AssetError> {
        self.loads.fetch_add(1, Ordering::SeqCst);
        Ok(self.manifest.clone())
    }
}

#[derive(Clone)]
struct SlowFactory {
    delay: Duration,
}

impl AssetRuntimeFactory for SlowFactory {
    type Asset = DummyAsset;
    type Loaded = LoadedDummy;

    fn load(&self, ctx: AssetLoadContext<'_>) -> Result<LoadedAsset<Self::Loaded>, AssetError> {
        std::thread::sleep(self.delay);
        let text =
            std::str::from_utf8(ctx.bytes).map_err(|error| AssetError::InvalidCookedAsset {
                id: Some(ctx.asset_id),
                message: error.to_string(),
            })?;
        Ok(LoadedAsset::new(LoadedDummy(text.to_string()))
            .with_dependencies(ctx.entry.dependencies.clone()))
    }

    fn begin_install(
        &self,
        loaded: &Self::Loaded,
        _ctx: AssetInstallContext<'_>,
    ) -> Result<AssetInstallResult<Self::Asset>, AssetError> {
        Ok(AssetInstallResult::Ready(DummyAsset(loaded.0.clone())))
    }
}

struct SlowTextureFactory {
    delay: Duration,
}

impl AssetRuntimeFactory for SlowTextureFactory {
    type Asset = TextureAsset;
    type Loaded = TextureAsset;

    fn load(&self, ctx: AssetLoadContext<'_>) -> Result<LoadedAsset<Self::Loaded>, AssetError> {
        std::thread::sleep(self.delay);
        Ok(LoadedAsset::new(TextureAsset::white_pixel())
            .with_dependencies(ctx.entry.dependencies.clone()))
    }

    fn begin_install(
        &self,
        loaded: &Self::Loaded,
        _ctx: AssetInstallContext<'_>,
    ) -> Result<AssetInstallResult<Self::Asset>, AssetError> {
        Ok(AssetInstallResult::Ready(loaded.clone()))
    }
}

struct FailingInstallFactory;

impl AssetRuntimeFactory for FailingInstallFactory {
    type Asset = DummyAsset;
    type Loaded = LoadedDummy;

    fn load(&self, ctx: AssetLoadContext<'_>) -> Result<LoadedAsset<Self::Loaded>, AssetError> {
        let text =
            std::str::from_utf8(ctx.bytes).map_err(|error| AssetError::InvalidCookedAsset {
                id: Some(ctx.asset_id),
                message: error.to_string(),
            })?;
        Ok(LoadedAsset::new(LoadedDummy(text.to_string())))
    }

    fn begin_install(
        &self,
        _loaded: &Self::Loaded,
        _ctx: AssetInstallContext<'_>,
    ) -> Result<AssetInstallResult<Self::Asset>, AssetError> {
        Err(AssetError::Unsupported {
            message: "install failed intentionally".to_string(),
        })
    }
}

struct FailingUninstallFactory;

impl AssetRuntimeFactory for FailingUninstallFactory {
    type Asset = DummyAsset;
    type Loaded = LoadedDummy;

    fn load(&self, ctx: AssetLoadContext<'_>) -> Result<LoadedAsset<Self::Loaded>, AssetError> {
        let text =
            std::str::from_utf8(ctx.bytes).map_err(|error| AssetError::InvalidCookedAsset {
                id: Some(ctx.asset_id),
                message: error.to_string(),
            })?;
        Ok(LoadedAsset::new(LoadedDummy(text.to_string())))
    }

    fn begin_install(
        &self,
        loaded: &Self::Loaded,
        _ctx: AssetInstallContext<'_>,
    ) -> Result<AssetInstallResult<Self::Asset>, AssetError> {
        Ok(AssetInstallResult::Ready(DummyAsset(loaded.0.clone())))
    }

    fn uninstall(
        &self,
        _installed: &Self::Asset,
        _ctx: AssetUninstallContext<'_>,
    ) -> Result<(), AssetError> {
        Err(AssetError::Unsupported {
            message: "uninstall failed intentionally".to_string(),
        })
    }
}

#[derive(Clone)]
struct DeferredInstallFactory {
    pending_polls: usize,
}

impl AssetRuntimeFactory for DeferredInstallFactory {
    type Asset = DummyAsset;
    type Loaded = LoadedDummy;

    fn load(&self, ctx: AssetLoadContext<'_>) -> Result<LoadedAsset<Self::Loaded>, AssetError> {
        let text =
            std::str::from_utf8(ctx.bytes).map_err(|error| AssetError::InvalidCookedAsset {
                id: Some(ctx.asset_id),
                message: error.to_string(),
            })?;
        Ok(LoadedAsset::new(LoadedDummy(text.to_string())))
    }

    fn begin_install(
        &self,
        loaded: &Self::Loaded,
        _ctx: AssetInstallContext<'_>,
    ) -> Result<AssetInstallResult<Self::Asset>, AssetError> {
        Ok(AssetInstallResult::Pending(Box::new(DeferredInstallTask {
            value: loaded.0.clone(),
            pending_polls: self.pending_polls,
        })))
    }
}

struct DeferredInstallTask {
    value: String,
    pending_polls: usize,
}

impl AssetInstallTask for DeferredInstallTask {
    type Output = DummyAsset;

    fn poll_install(
        &mut self,
        _ctx: AssetInstallContext<'_>,
        _budget: AssetInstallBudget,
    ) -> Result<AssetInstallPoll<Self::Output>, AssetError> {
        if self.pending_polls > 0 {
            self.pending_polls -= 1;
            return Ok(AssetInstallPoll::Pending);
        }

        Ok(AssetInstallPoll::Ready(DummyAsset(self.value.clone())))
    }
}

#[derive(Clone, Default)]
struct InvalidationCountingProvider {
    invalidate_all_calls: Arc<AtomicUsize>,
}

impl InvalidationCountingProvider {
    fn invalidate_all_calls(&self) -> usize {
        self.invalidate_all_calls.load(Ordering::SeqCst)
    }
}

impl AssetProvider for InvalidationCountingProvider {
    fn resolve(
        &self,
        id: AssetId,
        _entry: AssetManifestEntry,
        _raw_source_path: Option<PathBuf>,
    ) -> Result<ResolvedAssetSource, AssetError> {
        Err(AssetError::AssetNotFound { id })
    }

    fn invalidate_all(&self) {
        self.invalidate_all_calls.fetch_add(1, Ordering::SeqCst);
    }
}

#[derive(Clone)]
struct CountingFactory {
    loads: Arc<Mutex<HashMap<AssetId, usize>>>,
    installs: Arc<Mutex<HashMap<AssetId, usize>>>,
    uninstalls: Arc<Mutex<HashMap<AssetId, usize>>>,
}

impl CountingFactory {
    fn new() -> Self {
        Self {
            loads: Arc::new(Mutex::new(HashMap::default())),
            installs: Arc::new(Mutex::new(HashMap::default())),
            uninstalls: Arc::new(Mutex::new(HashMap::default())),
        }
    }

    fn load_count(&self, id: AssetId) -> usize {
        *self
            .loads
            .lock()
            .expect("counting loads mutex poisoned")
            .get(&id)
            .unwrap_or(&0)
    }

    fn install_count(&self, id: AssetId) -> usize {
        *self
            .installs
            .lock()
            .expect("counting installs mutex poisoned")
            .get(&id)
            .unwrap_or(&0)
    }

    fn uninstall_count(&self, id: AssetId) -> usize {
        *self
            .uninstalls
            .lock()
            .expect("counting uninstalls mutex poisoned")
            .get(&id)
            .unwrap_or(&0)
    }
}

impl AssetRuntimeFactory for CountingFactory {
    type Asset = DummyAsset;
    type Loaded = LoadedDummy;

    fn load(&self, ctx: AssetLoadContext<'_>) -> Result<LoadedAsset<Self::Loaded>, AssetError> {
        let mut loads = self.loads.lock().expect("counting loads mutex poisoned");
        *loads.entry(ctx.asset_id).or_insert(0) += 1;
        drop(loads);

        let text =
            std::str::from_utf8(ctx.bytes).map_err(|error| AssetError::InvalidCookedAsset {
                id: Some(ctx.asset_id),
                message: error.to_string(),
            })?;
        Ok(LoadedAsset::new(LoadedDummy(text.to_string()))
            .with_dependencies(ctx.entry.dependencies.clone()))
    }

    fn begin_install(
        &self,
        loaded: &Self::Loaded,
        ctx: AssetInstallContext<'_>,
    ) -> Result<AssetInstallResult<Self::Asset>, AssetError> {
        let mut installs = self
            .installs
            .lock()
            .expect("counting installs mutex poisoned");
        *installs.entry(ctx.asset_id).or_insert(0) += 1;
        Ok(AssetInstallResult::Ready(DummyAsset(loaded.0.clone())))
    }

    fn uninstall(
        &self,
        _installed: &Self::Asset,
        ctx: AssetUninstallContext<'_>,
    ) -> Result<(), AssetError> {
        let mut uninstalls = self
            .uninstalls
            .lock()
            .expect("counting uninstalls mutex poisoned");
        *uninstalls.entry(ctx.asset_id).or_insert(0) += 1;
        Ok(())
    }
}

fn write_manifest_entries(
    root: &Path,
    entries: Vec<AssetManifestEntry>,
) -> Result<AssetConfig, Box<dyn std::error::Error>> {
    let config = AssetConfig::new(root, "native");
    std::fs::create_dir_all(config.cooked_root())?;
    let manifest = AssetRegistryManifest {
        version: ASSET_SYSTEM_VERSION,
        target: "native".to_string(),
        provenance: Vec::new(),
        assets: entries,
    };
    std::fs::write(
        config.manifest_path(),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    Ok(config)
}

fn write_manifest(
    root: &Path,
    entry: AssetManifestEntry,
) -> Result<AssetConfig, Box<dyn std::error::Error>> {
    write_manifest_entries(root, vec![entry])
}

#[test]
fn queued_request_is_canceled_when_last_lease_is_dropped_before_update(
) -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let asset_id = AssetId::new();
    let config = write_manifest(
        dir.path(),
        AssetManifestEntry {
            asset_id,
            asset_type: "dummy".to_string(),
            importer: "dummy".to_string(),
            cooker: "dummy".to_string(),
            version: 1,
            source_path: "clip.dummy".to_string(),
            cooked_path: "missing.dummyc".to_string(),
            dependencies: Vec::new(),
            import_settings: serde_json::Value::Null,
        },
    )?;

    let server = Assets::new(config)?;
    server.register_factory(DummyFactory);
    let handle = server.load_id::<DummyAsset>(asset_id)?;
    drop(handle);

    server.update()?;

    assert_eq!(server.state_untyped(asset_id), AssetState::Unloaded);
    let stats = server.stats();
    assert_eq!(stats.submitted_requests, 1);
    assert_eq!(stats.activated_requests, 0);
    assert_eq!(stats.canceled_requests, 1);
    let canceled = server.canceled_request_snapshots();
    assert_eq!(canceled.len(), 1);
    assert_eq!(canceled[0].asset_id, asset_id);
    assert_eq!(canceled[0].status, AssetRequestStatus::Canceled);
    Ok(())
}

#[test]
fn asset_events_include_backend_residency_context() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let dependency_id = AssetId::new();
    let asset_id = AssetId::new();
    let config = write_manifest_entries(
        dir.path(),
        vec![
            AssetManifestEntry {
                asset_id: asset_id,
                asset_type: "dummy".to_string(),
                importer: "dummy".to_string(),
                cooker: "dummy".to_string(),
                version: 1,
                source_path: "clip.dummy".to_string(),
                cooked_path: "clip.dummyc".to_string(),
                dependencies: vec![dependency_id],
                import_settings: serde_json::Value::Null,
            },
            AssetManifestEntry {
                asset_id: dependency_id,
                asset_type: "dummy".to_string(),
                importer: "dummy".to_string(),
                cooker: "dummy".to_string(),
                version: 1,
                source_path: "dependency.dummy".to_string(),
                cooked_path: "dependency.dummyc".to_string(),
                dependencies: Vec::new(),
                import_settings: serde_json::Value::Null,
            },
        ],
    )?;
    let cooked_path = config.cooked_root().join("clip.dummyc");
    std::fs::write(&cooked_path, b"ready")?;
    std::fs::write(
        config.cooked_root().join("dependency.dummyc"),
        b"dependency",
    )?;

    let server = Assets::new(config)?;
    server.register_factory(DummyFactory);
    let mut events = server.event_cursor();
    let handle = server.load_id::<DummyAsset>(asset_id)?;

    server.update()?;

    let installed_events = server.events_since(&mut events);
    let loaded = installed_events
        .iter()
        .find(|event| event.id == asset_id && event.kind == AssetEventKind::Loaded)
        .expect("parent asset should emit loaded event");
    assert_eq!(loaded.state, AssetState::Loaded);
    assert_eq!(loaded.generation, 1);
    assert_eq!(loaded.asset_type, "dummy");
    assert_eq!(loaded.dependencies, vec![dependency_id]);
    assert!(loaded.manifest_fingerprint.is_some());
    assert!(loaded.content_hash.is_some());
    assert!(!loaded.reload_pending);
    let installed = installed_events
        .iter()
        .find(|event| event.id == asset_id && event.kind == AssetEventKind::Installed)
        .expect("parent asset should emit installed event");
    assert_eq!(installed.state, AssetState::Installed);
    assert_eq!(installed.generation, 1);
    assert_eq!(installed.asset_type, "dummy");
    assert_eq!(installed.dependencies, vec![dependency_id]);
    assert!(installed.manifest_fingerprint.is_some());
    assert!(installed.content_hash.is_some());
    assert!(!installed.reload_pending);

    std::fs::write(&cooked_path, b"fresh")?;
    let report = server.reload_changed_with_report()?;
    assert_eq!(report.changed_roots, vec![asset_id]);
    let reload_events = server.events_since(&mut events);
    let reload = reload_events
        .iter()
        .find(|event| event.id == asset_id && event.kind == AssetEventKind::ReloadQueued)
        .expect("parent asset should emit reload queued event");
    assert_eq!(reload.generation, 2);
    assert_eq!(reload.asset_type, "dummy");
    assert!(reload.content_hash.is_none());
    assert!(reload.reload_pending);

    server.update()?;
    assert_eq!(server.get(&handle)?.0, "fresh");
    let reloaded_events = server.events_since(&mut events);
    let reloaded = reloaded_events
        .iter()
        .find(|event| event.id == asset_id && event.kind == AssetEventKind::Reloaded)
        .expect("parent asset should emit reloaded event");
    assert_eq!(reloaded.state, AssetState::Installed);
    assert_eq!(reloaded.generation, 2);
    assert_eq!(reloaded.asset_type, "dummy");
    assert_eq!(reloaded.dependencies, vec![dependency_id]);
    assert!(reloaded.manifest_fingerprint.is_some());
    assert!(reloaded.content_hash.is_some());
    assert!(!reloaded.reload_pending);
    Ok(())
}

#[test]
fn memory_provider_loads_cooked_asset_without_filesystem_bytes(
) -> Result<(), Box<dyn std::error::Error>> {
    let asset_id = AssetId::new();
    let entry = AssetManifestEntry {
        asset_id,
        asset_type: "dummy".to_string(),
        importer: "dummy".to_string(),
        cooker: "dummy".to_string(),
        version: 1,
        source_path: "memory.dummy".to_string(),
        cooked_path: "memory.dummyc".to_string(),
        dependencies: Vec::new(),
        import_settings: serde_json::Value::Null,
    };
    let manifest = AssetRegistryManifest {
        version: ASSET_SYSTEM_VERSION,
        target: "native".to_string(),
        provenance: Vec::new(),
        assets: vec![entry],
    };
    let provider = MemoryAssetProvider::new().with_asset(asset_id, b"from-memory".to_vec());
    let server = Assets::with_manifest_and_provider(
        AssetConfig::new("memory-root", "native"),
        manifest,
        provider,
    );
    server.register_factory(DummyFactory);

    let handle = server.load_id::<DummyAsset>(asset_id)?;
    server.update()?;

    assert_eq!(server.get(&handle)?.0, "from-memory");
    Ok(())
}

#[test]
fn registry_loader_supplies_manifest_without_local_manifest_file(
) -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let config = AssetConfig::new(dir.path(), "native");
    std::fs::create_dir_all(config.cooked_root())?;
    assert!(matches!(
        load_manifest(&config),
        Err(AssetError::ManifestMissing { .. })
    ));

    let asset_id = AssetId::new();
    let manifest = AssetRegistryManifest {
        version: ASSET_SYSTEM_VERSION,
        target: "native".to_string(),
        provenance: Vec::new(),
        assets: vec![AssetManifestEntry {
            asset_id,
            asset_type: "dummy".to_string(),
            importer: "dummy".to_string(),
            cooker: "dummy".to_string(),
            version: 1,
            source_path: "virtual.dummy".to_string(),
            cooked_path: "virtual.dummyc".to_string(),
            dependencies: Vec::new(),
            import_settings: serde_json::Value::Null,
        }],
    };
    let loads = Arc::new(AtomicUsize::new(0));
    let provider = MemoryAssetProvider::new().with_asset(asset_id, b"from-registry".to_vec());
    let server = Assets::with_registry_loader_and_provider(
        config,
        StaticRegistryLoader {
            manifest,
            loads: loads.clone(),
        },
        provider,
    )?;
    server.register_factory(DummyFactory);

    let handle = server.load_id::<DummyAsset>(asset_id)?;
    server.update()?;
    assert_eq!(server.get(&handle)?.0, "from-registry");
    assert_eq!(loads.load(Ordering::SeqCst), 1);

    server.reload_manifest()?;
    assert_eq!(loads.load(Ordering::SeqCst), 2);
    Ok(())
}

#[test]
fn memory_provider_read_failure_records_failed_request_without_filesystem(
) -> Result<(), Box<dyn std::error::Error>> {
    let asset_id = AssetId::new();
    let entry = AssetManifestEntry {
        asset_id,
        asset_type: "dummy".to_string(),
        importer: "dummy".to_string(),
        cooker: "dummy".to_string(),
        version: 1,
        source_path: "memory.dummy".to_string(),
        cooked_path: "memory.dummyc".to_string(),
        dependencies: Vec::new(),
        import_settings: serde_json::Value::Null,
    };
    let manifest = AssetRegistryManifest {
        version: ASSET_SYSTEM_VERSION,
        target: "native".to_string(),
        provenance: Vec::new(),
        assets: vec![entry],
    };
    let read_error = AssetError::Io {
        path: PathBuf::from(format!("memory://{asset_id}")),
        message: "simulated read failure".to_string(),
    };
    let provider = MemoryAssetProvider::new().with_read_error(asset_id, read_error.clone());
    let server = Assets::with_manifest_and_provider(
        AssetConfig::new("memory-root", "native"),
        manifest,
        provider,
    );
    server.register_factory(DummyFactory);

    let handle = server.load_id::<DummyAsset>(asset_id)?;
    let error = server
        .update()
        .expect_err("memory read failure should surface through normal update");

    assert_eq!(error, read_error);
    assert_eq!(server.state(&handle), AssetState::Failed);
    assert_eq!(server.failure_phase(&handle), Some(AssetFailurePhase::Read));
    let failed = server.failed_request_snapshots();
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].asset_id, asset_id);
    assert_eq!(failed[0].status, AssetRequestStatus::Failed);
    assert_eq!(failed[0].generation, 1);
    assert!(failed[0]
        .last_error
        .as_deref()
        .is_some_and(|message| message.contains("simulated read failure")));
    Ok(())
}

#[test]
fn memory_provider_delayed_source_stays_inflight_without_filesystem(
) -> Result<(), Box<dyn std::error::Error>> {
    let asset_id = AssetId::new();
    let entry = AssetManifestEntry {
        asset_id,
        asset_type: "dummy".to_string(),
        importer: "dummy".to_string(),
        cooker: "dummy".to_string(),
        version: 1,
        source_path: "memory.dummy".to_string(),
        cooked_path: "memory.dummyc".to_string(),
        dependencies: Vec::new(),
        import_settings: serde_json::Value::Null,
    };
    let manifest = AssetRegistryManifest {
        version: ASSET_SYSTEM_VERSION,
        target: "native".to_string(),
        provenance: Vec::new(),
        assets: vec![entry],
    };
    let provider = MemoryAssetProvider::new().with_delayed_asset(
        asset_id,
        b"delayed-memory".to_vec(),
        Duration::from_millis(60),
    );
    let server = Assets::with_manifest_and_provider(
        AssetConfig::new("memory-root", "native").with_background_loading(true),
        manifest,
        provider,
    );
    server.register_factory(DummyFactory);

    let handle = server.load_id::<DummyAsset>(asset_id)?;
    server.update()?;

    assert_eq!(server.state(&handle), AssetState::Loading);
    assert_eq!(server.stats().inflight_loads, 1);
    let active = server.active_request_snapshots();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].asset_id, asset_id);
    assert_eq!(active[0].status, AssetRequestStatus::Loading);

    std::thread::sleep(Duration::from_millis(90));
    server.update()?;

    assert_eq!(server.state(&handle), AssetState::Installed);
    assert_eq!(server.get(&handle)?.0, "delayed-memory");
    assert_eq!(server.stats().inflight_loads, 0);
    Ok(())
}

#[test]
fn memory_provider_loads_dependency_chain_without_filesystem(
) -> Result<(), Box<dyn std::error::Error>> {
    let parent_id = AssetId::new();
    let dependency_id = AssetId::new();
    let manifest = AssetRegistryManifest {
        version: ASSET_SYSTEM_VERSION,
        target: "native".to_string(),
        provenance: Vec::new(),
        assets: vec![
            AssetManifestEntry {
                asset_id: parent_id,
                asset_type: "dummy".to_string(),
                importer: "dummy".to_string(),
                cooker: "dummy".to_string(),
                version: 1,
                source_path: "parent.dummy".to_string(),
                cooked_path: "parent.dummyc".to_string(),
                dependencies: vec![dependency_id],
                import_settings: serde_json::Value::Null,
            },
            AssetManifestEntry {
                asset_id: dependency_id,
                asset_type: "dummy".to_string(),
                importer: "dummy".to_string(),
                cooker: "dummy".to_string(),
                version: 1,
                source_path: "dependency.dummy".to_string(),
                cooked_path: "dependency.dummyc".to_string(),
                dependencies: Vec::new(),
                import_settings: serde_json::Value::Null,
            },
        ],
    };
    let provider = MemoryAssetProvider::new()
        .with_asset(parent_id, b"parent".to_vec())
        .with_asset(dependency_id, b"dependency".to_vec());
    let server = Assets::with_manifest_and_provider(
        AssetConfig::new("memory-root", "native"),
        manifest,
        provider,
    );
    server.register_factory(DummyFactory);

    let parent = server.load_id::<DummyAsset>(parent_id)?;
    server.update()?;

    assert_eq!(server.state(&parent), AssetState::Installed);
    assert_eq!(server.get(&parent)?.0, "parent");
    assert_eq!(server.state_untyped(dependency_id), AssetState::Installed);

    drop(parent);
    server.update()?;

    assert_eq!(server.state_untyped(parent_id), AssetState::Unloaded);
    assert_eq!(server.state_untyped(dependency_id), AssetState::Unloaded);
    Ok(())
}

#[test]
fn package_root_cooked_artifact_loads_through_normal_assets_facade(
) -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let asset_id = AssetId::new();
    let config = write_manifest(
        dir.path(),
        AssetManifestEntry {
            asset_id,
            asset_type: "dummy".to_string(),
            importer: "dummy".to_string(),
            cooker: "dummy".to_string(),
            version: 1,
            source_path: "clip.dummy".to_string(),
            cooked_path: "clip.dummyc".to_string(),
            dependencies: Vec::new(),
            import_settings: serde_json::Value::Null,
        },
    )?
    .with_package_root("packages/base");
    let package_root = config.asset_root.join("packages/base");
    std::fs::create_dir_all(&package_root)?;
    std::fs::write(package_root.join("clip.dummyc"), b"from-package")?;

    let server = Assets::new(config)?;
    server.register_factory(DummyFactory);
    let handle = server.load_id::<DummyAsset>(asset_id)?;
    server.update()?;

    assert_eq!(server.get(&handle)?.0, "from-package");
    Ok(())
}

#[test]
fn package_file_cooked_artifact_loads_through_normal_assets_facade(
) -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let asset_id = AssetId::new();
    let config = write_manifest(
        dir.path(),
        AssetManifestEntry {
            asset_id,
            asset_type: "dummy".to_string(),
            importer: "dummy".to_string(),
            cooker: "dummy".to_string(),
            version: 1,
            source_path: "clip.dummy".to_string(),
            cooked_path: "clip.dummyc".to_string(),
            dependencies: Vec::new(),
            import_settings: serde_json::Value::Null,
        },
    )?
    .with_package_file("base.skybundle");
    write_test_bundle(
        &config.asset_root.join("base.skybundle"),
        &[("clip.dummyc", b"from-bundle")],
    )?;

    let server = Assets::new(config)?;
    server.register_factory(DummyFactory);
    let handle = server.load_id::<DummyAsset>(asset_id)?;
    server.update()?;

    assert_eq!(server.get(&handle)?.0, "from-bundle");
    let stats = server.stats();
    assert_eq!(stats.provider.package_files, 1);
    assert_eq!(stats.provider.cached_bundle_indexes, 1);
    assert_eq!(stats.provider.cached_bundle_index_entries, 1);
    Ok(())
}

#[test]
fn force_reload_reads_rewritten_package_file_bundle() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let asset_id = AssetId::new();
    let config = write_manifest(
        dir.path(),
        AssetManifestEntry {
            asset_id,
            asset_type: "dummy".to_string(),
            importer: "dummy".to_string(),
            cooker: "dummy".to_string(),
            version: 1,
            source_path: "clip.dummy".to_string(),
            cooked_path: "clip.dummyc".to_string(),
            dependencies: Vec::new(),
            import_settings: serde_json::Value::Null,
        },
    )?
    .with_package_file("base.skybundle");
    let bundle = config.asset_root.join("base.skybundle");
    write_test_bundle(&bundle, &[("clip.dummyc", b"from-bundle")])?;

    let server = Assets::new(config)?;
    server.register_factory(DummyFactory);
    let handle = server.load_id::<DummyAsset>(asset_id)?;
    server.update()?;
    assert_eq!(server.get(&handle)?.0, "from-bundle");

    write_test_bundle(&bundle, &[("clip.dummyc", b"from-updated-bundle")])?;
    let report = server.force_reload(asset_id)?;
    assert_eq!(report.changed_roots, vec![asset_id]);
    server.update()?;

    assert_eq!(server.get(&handle)?.0, "from-updated-bundle");
    Ok(())
}

#[test]
fn replace_runtime_keeps_handle_and_updates_payload() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let config = write_manifest_entries(dir.path(), Vec::new())?;
    let server = Assets::new(config)?;
    let mut events = server.event_cursor();
    let handle = server.insert_runtime(DummyAsset("frame-a".to_string()));
    let _ = server.events_since(&mut events);

    server.replace_runtime(&handle, DummyAsset("frame-b".to_string()))?;

    assert_eq!(server.state(&handle), AssetState::Installed);
    assert_eq!(server.get(&handle)?.0, "frame-b");
    let events = server.events_since(&mut events);
    assert!(events
        .iter()
        .any(|event| event.id == handle.id() && event.kind == AssetEventKind::Installed));
    Ok(())
}

#[test]
fn assets_progress_to_installed_and_can_be_read() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let asset_id = AssetId::new();
    let config = write_manifest(
        dir.path(),
        AssetManifestEntry {
            asset_id,
            asset_type: "dummy".to_string(),
            importer: "dummy".to_string(),
            cooker: "dummy".to_string(),
            version: 1,
            source_path: "clip.dummy".to_string(),
            cooked_path: "clip.dummyc".to_string(),
            dependencies: Vec::new(),
            import_settings: serde_json::Value::Null,
        },
    )?;
    std::fs::write(config.cooked_root().join("clip.dummyc"), b"ready")?;

    let server = Assets::new(config)?;
    server.register_factory(DummyFactory);
    let handle = server.load_id::<DummyAsset>(asset_id)?;

    server.update()?;
    assert_eq!(server.state(&handle), AssetState::Installed);
    assert_eq!(server.get(&handle)?.0, "ready");
    Ok(())
}

#[test]
fn missing_dependency_marks_asset_failed() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let asset_id = AssetId::new();
    let dependency = AssetId::new();
    let config = write_manifest(
        dir.path(),
        AssetManifestEntry {
            asset_id,
            asset_type: "dummy".to_string(),
            importer: "dummy".to_string(),
            cooker: "dummy".to_string(),
            version: 1,
            source_path: "clip.dummy".to_string(),
            cooked_path: "clip.dummyc".to_string(),
            dependencies: vec![dependency],
            import_settings: serde_json::Value::Null,
        },
    )?;
    std::fs::write(config.cooked_root().join("clip.dummyc"), b"ready")?;

    let server = Assets::new(config)?;
    server.register_factory(DummyFactory);
    let handle = server.load_id::<DummyAsset>(asset_id)?;
    let result = server.update();

    assert!(matches!(
        result,
        Err(AssetError::MissingDependency { id, dependency: dep }) if id == asset_id && dep == dependency
    ));
    assert_eq!(server.state(&handle), AssetState::Failed);
    assert_eq!(
        server.failure_phase(&handle),
        Some(AssetFailurePhase::Dependency)
    );
    Ok(())
}

#[test]
fn install_failure_records_install_phase() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let asset_id = AssetId::new();
    let config = write_manifest(
        dir.path(),
        AssetManifestEntry {
            asset_id,
            asset_type: "dummy".to_string(),
            importer: "dummy".to_string(),
            cooker: "dummy".to_string(),
            version: 1,
            source_path: "clip.dummy".to_string(),
            cooked_path: "clip.dummyc".to_string(),
            dependencies: Vec::new(),
            import_settings: serde_json::Value::Null,
        },
    )?;
    std::fs::write(config.cooked_root().join("clip.dummyc"), b"ready")?;

    let server = Assets::new(config)?;
    server.register_factory(FailingInstallFactory);
    let handle = server.load_id::<DummyAsset>(asset_id)?;
    let result = server.update();

    assert!(matches!(result, Err(AssetError::Unsupported { .. })));
    assert_eq!(server.state(&handle), AssetState::Failed);
    assert_eq!(
        server.failure_phase(&handle),
        Some(AssetFailurePhase::Install)
    );
    let failures = server.failed_asset_snapshots();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].asset_id, asset_id);
    assert_eq!(failures[0].asset_type, "dummy");
    assert_eq!(failures[0].state, AssetState::Failed);
    assert_eq!(failures[0].phase, Some(AssetFailurePhase::Install));
    assert!(matches!(failures[0].error, AssetError::Unsupported { .. }));
    Ok(())
}

#[test]
fn install_precondition_failure_records_install_phase_and_failed_request(
) -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let asset_id = AssetId::new();
    let config = write_manifest(
        dir.path(),
        AssetManifestEntry {
            asset_id,
            asset_type: "dummy".to_string(),
            importer: "dummy".to_string(),
            cooker: "dummy".to_string(),
            version: 1,
            source_path: "clip.dummy".to_string(),
            cooked_path: "clip.dummyc".to_string(),
            dependencies: Vec::new(),
            import_settings: serde_json::Value::Null,
        },
    )?;
    std::fs::write(config.cooked_root().join("clip.dummyc"), b"ready")?;

    let server = Assets::new(config)?;
    server.register_factory(DummyFactory);
    let mut events = server.event_cursor();
    let handle = server.load_id::<DummyAsset>(asset_id)?;
    {
        let mut inner = server.inner.lock().expect("assets mutex poisoned");
        inner
            .store
            .force_missing_loaded_payload_for_install_test(asset_id)?;
    }

    let error = server
        .update()
        .expect_err("missing loaded payload should fail install");
    assert!(matches!(
        error,
        AssetError::InvalidState {
            id,
            state: AssetState::Installing,
            ..
        } if id == asset_id
    ));
    assert_eq!(server.state(&handle), AssetState::Failed);
    assert_eq!(
        server.failure_phase(&handle),
        Some(AssetFailurePhase::Install)
    );

    let failed_events = server.events_since(&mut events);
    let failed = failed_events
        .iter()
        .find(|event| event.id == asset_id && event.kind == AssetEventKind::Failed)
        .expect("install precondition failure should emit a failed event");
    assert_eq!(failed.failure_phase, Some(AssetFailurePhase::Install));

    let failed_requests = server.failed_request_snapshots();
    assert_eq!(failed_requests.len(), 1);
    assert_eq!(failed_requests[0].asset_id, asset_id);
    assert_eq!(failed_requests[0].status, AssetRequestStatus::Failed);
    assert!(failed_requests[0]
        .last_error
        .as_deref()
        .is_some_and(|error| error.contains("missing loaded payload")));
    Ok(())
}

#[test]
fn background_provider_resolve_failure_records_lookup_phase_and_failed_request(
) -> Result<(), Box<dyn std::error::Error>> {
    let asset_id = AssetId::new();
    let manifest = AssetRegistryManifest {
        version: ASSET_SYSTEM_VERSION,
        target: "native".to_string(),
        provenance: Vec::new(),
        assets: vec![AssetManifestEntry {
            asset_id,
            asset_type: "dummy".to_string(),
            importer: "dummy".to_string(),
            cooker: "dummy".to_string(),
            version: 1,
            source_path: "clip.dummy".to_string(),
            cooked_path: "clip.dummyc".to_string(),
            dependencies: Vec::new(),
            import_settings: serde_json::Value::Null,
        }],
    };
    let provider = MemoryAssetProvider::new()
        .with_resolve_error(asset_id, AssetError::AssetNotFound { id: asset_id });
    let server = Assets::with_manifest_and_provider(
        AssetConfig::new("memory-root", "native").with_background_loading(true),
        manifest,
        provider,
    );
    server.register_factory(DummyFactory);
    let mut events = server.event_cursor();
    let handle = server.load_id::<DummyAsset>(asset_id)?;

    let error = server
        .update()
        .expect_err("provider resolve failure should fail during background submit");
    assert_eq!(error, AssetError::AssetNotFound { id: asset_id });
    assert_eq!(server.state(&handle), AssetState::Failed);
    assert_eq!(
        server.failure_phase(&handle),
        Some(AssetFailurePhase::Lookup)
    );

    let failed_events = server.events_since(&mut events);
    let failed = failed_events
        .iter()
        .find(|event| event.id == asset_id && event.kind == AssetEventKind::Failed)
        .expect("background submit failure should emit a failed event");
    assert_eq!(failed.failure_phase, Some(AssetFailurePhase::Lookup));
    assert_eq!(failed.state, AssetState::Failed);

    let failed_requests = server.failed_request_snapshots();
    assert_eq!(failed_requests.len(), 1);
    assert_eq!(failed_requests[0].asset_id, asset_id);
    assert_eq!(failed_requests[0].status, AssetRequestStatus::Failed);
    assert!(failed_requests[0]
        .last_error
        .as_deref()
        .is_some_and(|error| error.contains("not found in manifest")));
    Ok(())
}

#[test]
fn load_blocking_provider_resolve_failure_records_lookup_phase_and_failed_request(
) -> Result<(), Box<dyn std::error::Error>> {
    let asset_id = AssetId::new();
    let manifest = AssetRegistryManifest {
        version: ASSET_SYSTEM_VERSION,
        target: "native".to_string(),
        provenance: Vec::new(),
        assets: vec![AssetManifestEntry {
            asset_id,
            asset_type: "dummy".to_string(),
            importer: "dummy".to_string(),
            cooker: "dummy".to_string(),
            version: 1,
            source_path: "clip.dummy".to_string(),
            cooked_path: "clip.dummyc".to_string(),
            dependencies: Vec::new(),
            import_settings: serde_json::Value::Null,
        }],
    };
    let provider = MemoryAssetProvider::new()
        .with_resolve_error(asset_id, AssetError::AssetNotFound { id: asset_id });
    let server = Assets::with_manifest_and_provider(
        AssetConfig::new("memory-root", "native").with_background_loading(true),
        manifest,
        provider,
    );
    server.register_factory(DummyFactory);
    let mut events = server.event_cursor();

    let error = server
        .load_blocking::<DummyAsset>(asset_id)
        .expect_err("blocking provider resolve failure should fail");
    assert_eq!(error, AssetError::AssetNotFound { id: asset_id });
    assert_eq!(server.state_untyped(asset_id), AssetState::Failed);
    assert_eq!(
        server.failure_phase_untyped(asset_id),
        Some(AssetFailurePhase::Lookup)
    );

    let failed_events = server.events_since(&mut events);
    let failed = failed_events
        .iter()
        .find(|event| event.id == asset_id && event.kind == AssetEventKind::Failed)
        .expect("blocking provider resolve failure should emit failed event");
    assert_eq!(failed.failure_phase, Some(AssetFailurePhase::Lookup));
    assert_eq!(failed.state, AssetState::Failed);

    let failed_requests = server.failed_request_snapshots();
    assert_eq!(failed_requests.len(), 1);
    assert_eq!(failed_requests[0].asset_id, asset_id);
    assert_eq!(failed_requests[0].status, AssetRequestStatus::Failed);
    assert!(failed_requests[0]
        .last_error
        .as_deref()
        .is_some_and(|error| error.contains("not found in manifest")));
    Ok(())
}

#[test]
fn load_blocking_provider_read_failure_records_read_phase_and_failed_request(
) -> Result<(), Box<dyn std::error::Error>> {
    let asset_id = AssetId::new();
    let manifest = AssetRegistryManifest {
        version: ASSET_SYSTEM_VERSION,
        target: "native".to_string(),
        provenance: Vec::new(),
        assets: vec![AssetManifestEntry {
            asset_id,
            asset_type: "dummy".to_string(),
            importer: "dummy".to_string(),
            cooker: "dummy".to_string(),
            version: 1,
            source_path: "clip.dummy".to_string(),
            cooked_path: "clip.dummyc".to_string(),
            dependencies: Vec::new(),
            import_settings: serde_json::Value::Null,
        }],
    };
    let read_error = AssetError::Io {
        path: PathBuf::from(format!("memory://{asset_id}")),
        message: "blocking read failed".to_string(),
    };
    let provider = MemoryAssetProvider::new().with_read_error(asset_id, read_error.clone());
    let server = Assets::with_manifest_and_provider(
        AssetConfig::new("memory-root", "native").with_background_loading(true),
        manifest,
        provider,
    );
    server.register_factory(DummyFactory);
    let mut events = server.event_cursor();

    let error = server
        .load_blocking::<DummyAsset>(asset_id)
        .expect_err("blocking provider read failure should fail");
    assert_eq!(error, read_error);
    assert_eq!(server.state_untyped(asset_id), AssetState::Failed);
    assert_eq!(
        server.failure_phase_untyped(asset_id),
        Some(AssetFailurePhase::Read)
    );
    assert_eq!(server.stats().load_timings.failed_source_loads, 1);

    let failed_events = server.events_since(&mut events);
    let failed = failed_events
        .iter()
        .find(|event| event.id == asset_id && event.kind == AssetEventKind::Failed)
        .expect("blocking provider read failure should emit failed event");
    assert_eq!(failed.failure_phase, Some(AssetFailurePhase::Read));
    assert_eq!(failed.state, AssetState::Failed);

    let failed_requests = server.failed_request_snapshots();
    assert_eq!(failed_requests.len(), 1);
    assert_eq!(failed_requests[0].asset_id, asset_id);
    assert_eq!(failed_requests[0].status, AssetRequestStatus::Failed);
    assert!(failed_requests[0]
        .last_error
        .as_deref()
        .is_some_and(|error| error.contains("blocking read failed")));
    Ok(())
}

#[test]
fn deferred_install_task_advances_across_updates() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let asset_id = AssetId::new();
    let config = write_manifest(
        dir.path(),
        AssetManifestEntry {
            asset_id,
            asset_type: "dummy".to_string(),
            importer: "dummy".to_string(),
            cooker: "dummy".to_string(),
            version: 1,
            source_path: "clip.dummy".to_string(),
            cooked_path: "clip.dummyc".to_string(),
            dependencies: Vec::new(),
            import_settings: serde_json::Value::Null,
        },
    )?;
    std::fs::write(config.cooked_root().join("clip.dummyc"), b"ready")?;

    let server = Assets::new(config)?;
    server.register_factory(DeferredInstallFactory { pending_polls: 1 });
    let handle = server.load_id::<DummyAsset>(asset_id)?;

    server.update()?;
    assert_eq!(server.state(&handle), AssetState::Installing);
    assert!(server.try_get(&handle).is_none());

    server.update()?;
    assert_eq!(server.state(&handle), AssetState::Installing);
    assert!(server.try_get(&handle).is_none());

    server.update()?;
    assert_eq!(server.state(&handle), AssetState::Installed);
    assert_eq!(server.get(&handle)?.0, "ready");
    Ok(())
}

#[test]
fn install_time_budget_can_defer_install_task_polling() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let asset_id = AssetId::new();
    let config = write_manifest(
        dir.path(),
        AssetManifestEntry {
            asset_id,
            asset_type: "dummy".to_string(),
            importer: "dummy".to_string(),
            cooker: "dummy".to_string(),
            version: 1,
            source_path: "clip.dummy".to_string(),
            cooked_path: "clip.dummyc".to_string(),
            dependencies: Vec::new(),
            import_settings: serde_json::Value::Null,
        },
    )?
    .with_install_time_budget(Duration::ZERO);
    std::fs::write(config.cooked_root().join("clip.dummyc"), b"ready")?;

    let server = Assets::new(config)?;
    server.register_factory(DeferredInstallFactory { pending_polls: 0 });
    let handle = server.load_id::<DummyAsset>(asset_id)?;

    server.update()?;
    assert_eq!(server.state(&handle), AssetState::Installing);
    assert!(server.try_get(&handle).is_none());

    let asset = server.load_blocking::<DummyAsset>(asset_id)?;
    assert_eq!(asset.0, "ready");
    assert_eq!(server.state(&handle), AssetState::Installed);
    Ok(())
}

#[test]
fn load_blocking_waits_for_deferred_install_task() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let asset_id = AssetId::new();
    let config = write_manifest(
        dir.path(),
        AssetManifestEntry {
            asset_id,
            asset_type: "dummy".to_string(),
            importer: "dummy".to_string(),
            cooker: "dummy".to_string(),
            version: 1,
            source_path: "clip.dummy".to_string(),
            cooked_path: "clip.dummyc".to_string(),
            dependencies: Vec::new(),
            import_settings: serde_json::Value::Null,
        },
    )?;
    std::fs::write(config.cooked_root().join("clip.dummyc"), b"ready")?;

    let server = Assets::new(config)?;
    server.register_factory(DeferredInstallFactory { pending_polls: 2 });

    let asset = server.load_blocking::<DummyAsset>(asset_id)?;

    assert_eq!(asset.0, "ready");
    assert_eq!(server.state_untyped(asset_id), AssetState::Installed);
    Ok(())
}

#[test]
fn typed_handles_reject_manifest_type_mismatch() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let asset_id = AssetId::new();
    let config = write_manifest(
        dir.path(),
        AssetManifestEntry {
            asset_id,
            asset_type: "texture".to_string(),
            importer: "texture.image".to_string(),
            cooker: "texture.rgba8".to_string(),
            version: 1,
            source_path: "hero.png".to_string(),
            cooked_path: "hero.skytx".to_string(),
            dependencies: Vec::new(),
            import_settings: serde_json::json!({ "srgb": true }),
        },
    )?;
    let server = Assets::new(config)?;
    let result = server.load_id::<DummyAsset>(asset_id);
    assert!(matches!(result, Err(AssetError::AssetTypeMismatch { .. })));
    Ok(())
}

#[test]
fn cooked_runtime_load_reports_schema_mismatch_before_decode(
) -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let asset_id = AssetId::new();
    let config = write_manifest(
        dir.path(),
        AssetManifestEntry {
            asset_id,
            asset_type: FontAsset::TYPE.to_string(),
            importer: "font.raw".to_string(),
            cooker: "font.legacy".to_string(),
            version: 99,
            source_path: "ui.ttf".to_string(),
            cooked_path: "ui.skyasset".to_string(),
            dependencies: Vec::new(),
            import_settings: serde_json::Value::Null,
        },
    )?;
    std::fs::write(
        config.cooked_root().join("ui.skyasset"),
        crate::asset::font::encode_font_cooked(&FontAsset::new(vec![1, 2, 3, 4])),
    )?;

    let server = Assets::new(config)?;
    let error = server
        .load_blocking::<FontAsset>(asset_id)
        .expect_err("schema mismatch should fail before font decode");

    match error {
        AssetError::CookedSchemaMismatch {
            id,
            expected_cooker,
            expected_version,
            actual_cooker,
            actual_version,
        } => {
            assert_eq!(id, asset_id);
            assert_eq!(expected_cooker, "font.raw_bytes");
            assert_eq!(expected_version, 1);
            assert_eq!(actual_cooker, "font.legacy");
            assert_eq!(actual_version, 99);
        }
        other => panic!("unexpected error: {other:?}"),
    }
    assert_eq!(
        server.failure_phase_untyped(asset_id),
        Some(AssetFailurePhase::Decode)
    );
    Ok(())
}

#[test]
fn dropping_last_handle_returns_asset_to_unloaded_state() -> Result<(), Box<dyn std::error::Error>>
{
    let dir = tempdir()?;
    let asset_id = AssetId::new();
    let config = write_manifest(
        dir.path(),
        AssetManifestEntry {
            asset_id,
            asset_type: "dummy".to_string(),
            importer: "dummy".to_string(),
            cooker: "dummy".to_string(),
            version: 1,
            source_path: "clip.dummy".to_string(),
            cooked_path: "clip.dummyc".to_string(),
            dependencies: Vec::new(),
            import_settings: serde_json::Value::Null,
        },
    )?;
    std::fs::write(config.cooked_root().join("clip.dummyc"), b"ready")?;

    let server = Assets::new(config)?;
    server.register_factory(DummyFactory);
    let handle = server.load_id::<DummyAsset>(asset_id)?;
    server.update()?;

    let handle_id = handle.id();
    drop(handle);
    server.update()?;
    assert_eq!(server.state_untyped(handle_id), AssetState::Unloaded);
    Ok(())
}

#[test]
fn queued_release_from_old_generation_preserves_reacquired_weak_handle(
) -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let asset_id = AssetId::new();
    let config = write_manifest(
        dir.path(),
        AssetManifestEntry {
            asset_id,
            asset_type: "dummy".to_string(),
            importer: "dummy".to_string(),
            cooker: "dummy".to_string(),
            version: 1,
            source_path: "clip.dummy".to_string(),
            cooked_path: "clip.dummyc".to_string(),
            dependencies: Vec::new(),
            import_settings: serde_json::Value::Null,
        },
    )?;
    let cooked_path = config.cooked_root().join("clip.dummyc");
    std::fs::write(&cooked_path, b"old")?;

    let server = Assets::new(config)?;
    server.register_factory(DummyFactory);
    let handle = server.load_id::<DummyAsset>(asset_id)?;
    server.update()?;
    assert_eq!(server.get(&handle)?.0, "old");

    let weak = handle.downgrade();
    drop(handle);
    std::fs::write(&cooked_path, b"new")?;
    let report = server.force_reload(asset_id)?;
    assert_eq!(report.changed_roots, vec![asset_id]);

    let reacquired = server.load_handle(weak)?;
    server.update()?;
    assert_eq!(server.state(&reacquired), AssetState::Installed);
    assert_eq!(server.get(&reacquired)?.0, "new");

    drop(reacquired);
    server.update()?;
    assert_eq!(server.state_untyped(asset_id), AssetState::Unloaded);
    Ok(())
}

#[test]
fn dropping_last_handle_calls_factory_uninstall_before_unload(
) -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let asset_id = AssetId::new();
    let config = write_manifest(
        dir.path(),
        AssetManifestEntry {
            asset_id,
            asset_type: "dummy".to_string(),
            importer: "dummy".to_string(),
            cooker: "dummy".to_string(),
            version: 1,
            source_path: "clip.dummy".to_string(),
            cooked_path: "clip.dummyc".to_string(),
            dependencies: Vec::new(),
            import_settings: serde_json::Value::Null,
        },
    )?;
    std::fs::write(config.cooked_root().join("clip.dummyc"), b"ready")?;

    let server = Assets::new(config)?;
    let factory = CountingFactory::new();
    server.register_factory(factory.clone());
    let mut events = server.event_cursor();
    let handle = server.load_id::<DummyAsset>(asset_id)?;
    server.update()?;
    let _ = server.events_since(&mut events);

    drop(handle);
    server.update()?;

    assert_eq!(factory.uninstall_count(asset_id), 1);
    assert_eq!(server.state_untyped(asset_id), AssetState::Unloaded);
    let emitted = server.events_since(&mut events);
    assert!(emitted
        .iter()
        .any(|event| event.id == asset_id && event.kind == AssetEventKind::Unloaded));
    Ok(())
}

#[test]
fn uninstall_failure_records_phase_and_failed_event() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let asset_id = AssetId::new();
    let config = write_manifest(
        dir.path(),
        AssetManifestEntry {
            asset_id,
            asset_type: "dummy".to_string(),
            importer: "dummy".to_string(),
            cooker: "dummy".to_string(),
            version: 1,
            source_path: "clip.dummy".to_string(),
            cooked_path: "clip.dummyc".to_string(),
            dependencies: Vec::new(),
            import_settings: serde_json::Value::Null,
        },
    )?;
    std::fs::write(config.cooked_root().join("clip.dummyc"), b"ready")?;

    let server = Assets::new(config)?;
    server.register_factory(FailingUninstallFactory);
    let mut events = server.event_cursor();
    let handle = server.load_id::<DummyAsset>(asset_id)?;
    server.update()?;
    let _ = server.events_since(&mut events);

    drop(handle);
    let error = server
        .update()
        .expect_err("uninstall hook failure should surface from update");
    assert!(matches!(error, AssetError::Unsupported { .. }));
    assert_eq!(
        server.failure_phase_untyped(asset_id),
        Some(AssetFailurePhase::Uninstall)
    );

    let failed_events = server.events_since(&mut events);
    let failed = failed_events
        .iter()
        .find(|event| event.id == asset_id && event.kind == AssetEventKind::Failed)
        .expect("failed uninstall should emit a failed event");
    assert_eq!(failed.failure_phase, Some(AssetFailurePhase::Uninstall));
    assert_eq!(failed.state, AssetState::Failed);
    Ok(())
}

#[test]
fn multiple_loads_hold_independent_leases() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let asset_id = AssetId::new();
    let config = write_manifest(
        dir.path(),
        AssetManifestEntry {
            asset_id,
            asset_type: "dummy".to_string(),
            importer: "dummy".to_string(),
            cooker: "dummy".to_string(),
            version: 1,
            source_path: "clip.dummy".to_string(),
            cooked_path: "clip.dummyc".to_string(),
            dependencies: Vec::new(),
            import_settings: serde_json::Value::Null,
        },
    )?;
    std::fs::write(config.cooked_root().join("clip.dummyc"), b"ready")?;

    let server = Assets::new(config)?;
    server.register_factory(DummyFactory);
    let first = server.load_id::<DummyAsset>(asset_id)?;
    let second = server.load_id::<DummyAsset>(asset_id)?;
    server.update()?;

    drop(first);
    server.update()?;
    assert_eq!(server.state(&second), AssetState::Installed);
    assert_eq!(server.get(&second)?.0, "ready");

    let second_id = second.id();
    drop(second);
    server.update()?;
    assert_eq!(server.state_untyped(second_id), AssetState::Unloaded);
    Ok(())
}

#[test]
fn same_frame_drop_and_reacquire_keeps_latest_lease_alive() -> Result<(), Box<dyn std::error::Error>>
{
    let dir = tempdir()?;
    let asset_id = AssetId::new();
    let config = write_manifest(
        dir.path(),
        AssetManifestEntry {
            asset_id,
            asset_type: "dummy".to_string(),
            importer: "dummy".to_string(),
            cooker: "dummy".to_string(),
            version: 1,
            source_path: "clip.dummy".to_string(),
            cooked_path: "clip.dummyc".to_string(),
            dependencies: Vec::new(),
            import_settings: serde_json::Value::Null,
        },
    )?;
    std::fs::write(config.cooked_root().join("clip.dummyc"), b"ready")?;

    let server = Assets::new(config)?;
    server.register_factory(DummyFactory);
    let first = server.load_id::<DummyAsset>(asset_id)?;
    drop(first);
    let second = server.load_id::<DummyAsset>(asset_id)?;

    server.update()?;

    assert_eq!(server.state(&second), AssetState::Installed);
    assert_eq!(server.get(&second)?.0, "ready");
    assert_eq!(server.stats().strong_references, 1);

    let second_id = second.id();
    drop(second);
    server.update()?;
    assert_eq!(server.state_untyped(second_id), AssetState::Unloaded);
    Ok(())
}

#[test]
fn dependencies_load_transitively_and_release_with_parent() -> Result<(), Box<dyn std::error::Error>>
{
    let dir = tempdir()?;
    let parent_id = AssetId::new();
    let dependency_id = AssetId::new();
    let config = write_manifest_entries(
        dir.path(),
        vec![
            AssetManifestEntry {
                asset_id: parent_id,
                asset_type: "dummy".to_string(),
                importer: "dummy".to_string(),
                cooker: "dummy".to_string(),
                version: 1,
                source_path: "parent.dummy".to_string(),
                cooked_path: "parent.dummyc".to_string(),
                dependencies: vec![dependency_id],
                import_settings: serde_json::Value::Null,
            },
            AssetManifestEntry {
                asset_id: dependency_id,
                asset_type: "dummy".to_string(),
                importer: "dummy".to_string(),
                cooker: "dummy".to_string(),
                version: 1,
                source_path: "dependency.dummy".to_string(),
                cooked_path: "dependency.dummyc".to_string(),
                dependencies: Vec::new(),
                import_settings: serde_json::Value::Null,
            },
        ],
    )?;
    std::fs::write(config.cooked_root().join("parent.dummyc"), b"parent")?;
    std::fs::write(
        config.cooked_root().join("dependency.dummyc"),
        b"dependency",
    )?;

    let server = Assets::new(config)?;
    server.register_factory(DummyFactory);
    let parent = server.load_id::<DummyAsset>(parent_id)?;
    server.update()?;

    assert_eq!(server.state(&parent), AssetState::Installed);
    assert_eq!(server.state_untyped(dependency_id), AssetState::Installed);

    drop(parent);
    server.update()?;
    assert_eq!(server.state_untyped(parent_id), AssetState::Unloaded);
    assert_eq!(server.state_untyped(dependency_id), AssetState::Unloaded);
    Ok(())
}

#[test]
fn direct_dependency_request_survives_parent_drop() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let parent_id = AssetId::new();
    let dependency_id = AssetId::new();
    let config = write_manifest_entries(
        dir.path(),
        vec![
            AssetManifestEntry {
                asset_id: parent_id,
                asset_type: "dummy".to_string(),
                importer: "dummy".to_string(),
                cooker: "dummy".to_string(),
                version: 1,
                source_path: "parent.dummy".to_string(),
                cooked_path: "parent.dummyc".to_string(),
                dependencies: vec![dependency_id],
                import_settings: serde_json::Value::Null,
            },
            AssetManifestEntry {
                asset_id: dependency_id,
                asset_type: "dummy".to_string(),
                importer: "dummy".to_string(),
                cooker: "dummy".to_string(),
                version: 1,
                source_path: "dependency.dummy".to_string(),
                cooked_path: "dependency.dummyc".to_string(),
                dependencies: Vec::new(),
                import_settings: serde_json::Value::Null,
            },
        ],
    )?;
    std::fs::write(config.cooked_root().join("parent.dummyc"), b"parent")?;
    std::fs::write(
        config.cooked_root().join("dependency.dummyc"),
        b"dependency",
    )?;

    let server = Assets::new(config)?;
    server.register_factory(DummyFactory);
    let parent = server.load_id::<DummyAsset>(parent_id)?;
    let dependency = server.load_id::<DummyAsset>(dependency_id)?;
    server.update()?;

    drop(parent);
    server.update()?;
    assert_eq!(server.state_untyped(parent_id), AssetState::Unloaded);
    assert_eq!(server.state(&dependency), AssetState::Installed);

    let dependency_id = dependency.id();
    drop(dependency);
    server.update()?;
    assert_eq!(server.state_untyped(dependency_id), AssetState::Unloaded);
    Ok(())
}

#[test]
fn load_uses_manifest_source_lookup() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let asset_root = dir.path().join("assets");
    std::fs::create_dir_all(asset_root.join("nested"))?;

    let asset_id = AssetId::new();
    let config = write_manifest_entries(
        &asset_root,
        vec![AssetManifestEntry {
            asset_id,
            asset_type: "dummy".to_string(),
            importer: "dummy".to_string(),
            cooker: "dummy".to_string(),
            version: 1,
            source_path: "nested/clip.dummy".to_string(),
            cooked_path: "clip.dummyc".to_string(),
            dependencies: Vec::new(),
            import_settings: serde_json::Value::Null,
        }],
    )?;
    std::fs::write(config.cooked_root().join("clip.dummyc"), b"ready")?;

    let server = Assets::new(config)?;
    server.register_factory(DummyFactory);
    let handle = server.load::<DummyAsset>(asset_root.join("nested").join("clip.dummy"))?;
    server.update()?;

    assert_eq!(handle.id(), asset_id);
    assert_eq!(server.get(&handle)?.0, "ready");
    Ok(())
}

#[test]
fn load_font_installs_raw_font_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    std::fs::write(dir.path().join("title.ttf"), b"fake-font")?;

    let server = Assets::with_empty_manifest(AssetConfig::new(dir.path(), "native"));
    let handle = server.load_font("title.ttf")?;
    for _ in 0..16 {
        server.update()?;
        if server.is_installed(&handle) {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }

    assert_eq!(server.get(&handle)?.bytes(), b"fake-font");
    Ok(())
}

#[test]
fn load_font_by_asset_id_reads_cooked_font() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let asset_id = AssetId::new();
    let config = write_manifest_entries(
        dir.path(),
        vec![AssetManifestEntry {
            asset_id,
            asset_type: FontAsset::TYPE.to_string(),
            importer: "font.raw".to_string(),
            cooker: "font.raw_bytes".to_string(),
            version: 1,
            source_path: "ui/title.ttf".to_string(),
            cooked_path: "title.skyfont".to_string(),
            dependencies: Vec::new(),
            import_settings: serde_json::Value::Null,
        }],
    )?;
    let cooked = super::super::font::encode_font_cooked(&FontAsset::new(Arc::<[u8]>::from(
        b"cooked-font".to_vec(),
    )));
    std::fs::write(config.cooked_root().join("title.skyfont"), cooked)?;

    let server = Assets::new(config)?;
    let handle = server.load_id::<FontAsset>(asset_id)?;
    server.update()?;

    assert_eq!(server.get(&handle)?.bytes(), b"cooked-font");
    Ok(())
}

#[test]
fn load_texture_deduplicates_raw_paths() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    image::save_buffer(
        dir.path().join("white.png"),
        &[255, 255, 255, 255],
        1,
        1,
        image::ColorType::Rgba8,
    )?;

    let server = Assets::with_empty_manifest(AssetConfig::new(dir.path(), "native"));
    let first = server.load_texture("white.png")?;
    let second = server.load_texture(dir.path().join("white.png"))?;
    assert_eq!(first, second);

    wait_for_terminal_texture(&server, &first)?;
    assert_eq!(server.state(&first), AssetState::Installed);
    assert_eq!(server.get(&first)?.size(), [1, 1]);
    Ok(())
}

#[test]
fn per_request_priority_is_used_for_manifest_and_raw_loads(
) -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    image::save_buffer(
        dir.path().join("white.png"),
        &[255, 255, 255, 255],
        1,
        1,
        image::ColorType::Rgba8,
    )?;

    let manifest_id = AssetId::new();
    let config = write_manifest(
        dir.path(),
        AssetManifestEntry {
            asset_id: manifest_id,
            asset_type: "dummy".to_string(),
            importer: "dummy".to_string(),
            cooker: "dummy".to_string(),
            version: 1,
            source_path: "clip.dummy".to_string(),
            cooked_path: "clip.dummyc".to_string(),
            dependencies: Vec::new(),
            import_settings: serde_json::Value::Null,
        },
    )?;
    std::fs::write(config.cooked_root().join("clip.dummyc"), b"ready")?;

    let server = Assets::new(config)?;
    server.register_factory(DummyFactory);
    let manifest = server.load_id_with_priority::<DummyAsset>(manifest_id, 21)?;
    let raw = server.load_texture_with_priority("white.png", -3)?;

    let snapshots = server.queued_request_snapshots();
    let manifest_snapshot = snapshots
        .iter()
        .find(|snapshot| snapshot.asset_id == manifest.id())
        .expect("manifest request snapshot");
    assert_eq!(manifest_snapshot.priority, 21);
    let raw_snapshot = snapshots
        .iter()
        .find(|snapshot| snapshot.asset_id == raw.id())
        .expect("raw texture request snapshot");
    assert_eq!(raw_snapshot.priority, -3);
    Ok(())
}

#[test]
fn load_texture_installs_raw_texture_metadata() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let pixels = [
        0, 0, 0, 0, 255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255, 255, 255,
    ];
    image::save_buffer(
        dir.path().join("pose.png"),
        &pixels,
        3,
        2,
        image::ColorType::Rgba8,
    )?;

    let server = Assets::with_empty_manifest(AssetConfig::new(dir.path(), "native"));
    let handle = server.load_texture("pose.png")?;
    wait_for_terminal_texture(&server, &handle)?;

    let texture = server.get(&handle)?;
    assert_eq!(texture.size(), [3, 2]);
    assert_eq!(texture.visible_rect(), [1, 0, 2, 2]);
    Ok(())
}

#[test]
fn load_texture_missing_raw_path_fails_without_panic() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let server = Assets::with_empty_manifest(AssetConfig::new(dir.path(), "native"));
    let handle = server.load_texture("missing.png")?;

    wait_for_terminal_texture(&server, &handle)?;
    assert_eq!(server.state(&handle), AssetState::Failed);
    assert!(matches!(server.error(&handle), Some(AssetError::Io { .. })));
    assert_eq!(server.failure_phase(&handle), Some(AssetFailurePhase::Read));
    let failed_requests = server.failed_request_snapshots();
    assert_eq!(failed_requests.len(), 1);
    assert_eq!(failed_requests[0].asset_id, handle.id());
    assert_eq!(
        failed_requests[0].status,
        crate::asset::AssetRequestStatus::Failed
    );
    assert!(failed_requests[0]
        .last_error
        .as_deref()
        .is_some_and(|error| error.contains("I/O error")));
    Ok(())
}

#[test]
fn load_texture_returns_before_raw_decode_completes() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    image::save_buffer(
        dir.path().join("white.png"),
        &[255, 255, 255, 255],
        1,
        1,
        image::ColorType::Rgba8,
    )?;

    let server = Assets::with_empty_manifest(AssetConfig::new(dir.path(), "native"));
    let handle = server.load_texture("white.png")?;

    assert_ne!(server.state(&handle), AssetState::Installed);
    wait_for_terminal_texture(&server, &handle)?;
    assert_eq!(server.state(&handle), AssetState::Installed);
    Ok(())
}

#[test]
fn load_texture_retries_failed_raw_path_when_requested_again(
) -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let path = dir.path().join("late.png");
    let server = Assets::with_empty_manifest(AssetConfig::new(dir.path(), "native"));
    let handle = server.load_texture("late.png")?;

    wait_for_terminal_texture(&server, &handle)?;
    assert_eq!(server.state(&handle), AssetState::Failed);

    image::save_buffer(&path, &[255, 255, 255, 255], 1, 1, image::ColorType::Rgba8)?;
    let retry = server.load_texture("late.png")?;
    assert_eq!(retry, handle);
    wait_for_terminal_texture(&server, &retry)?;

    assert_eq!(server.state(&retry), AssetState::Installed);
    assert_eq!(server.failure_phase(&retry), None);
    assert_eq!(server.get(&retry)?.size(), [1, 1]);
    Ok(())
}

#[test]
fn load_texture_reuses_raw_handle_after_unload() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    image::save_buffer(
        dir.path().join("white.png"),
        &[255, 255, 255, 255],
        1,
        1,
        image::ColorType::Rgba8,
    )?;

    let server = Assets::with_empty_manifest(AssetConfig::new(dir.path(), "native"));
    let first = server.load_texture("white.png")?;
    wait_for_terminal_texture(&server, &first)?;
    let first_id = first.id();
    drop(first);
    server.update()?;
    assert_eq!(server.state_untyped(first_id), AssetState::Unloaded);

    let second = server.load_texture("white.png")?;
    assert_eq!(second.id(), first_id);
    wait_for_terminal_texture(&server, &second)?;
    assert_eq!(server.state(&second), AssetState::Installed);
    Ok(())
}

#[test]
fn reload_manifest_refreshes_source_lookup() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let asset_root = dir.path().join("assets");
    std::fs::create_dir_all(&asset_root)?;

    let config = AssetConfig::new(&asset_root, "native");
    std::fs::create_dir_all(config.cooked_root())?;
    std::fs::write(
        config.manifest_path(),
        serde_json::to_vec_pretty(&AssetRegistryManifest::default())?,
    )?;

    let server = Assets::new(config.clone())?;
    assert_eq!(server.resolve_path(asset_root.join("late.dummy")), None);

    let asset_id = AssetId::new();
    std::fs::write(
        config.manifest_path(),
        serde_json::to_vec_pretty(&AssetRegistryManifest {
            version: ASSET_SYSTEM_VERSION,
            target: "native".to_string(),
            provenance: Vec::new(),
            assets: vec![AssetManifestEntry {
                asset_id,
                asset_type: "dummy".to_string(),
                importer: "dummy".to_string(),
                cooker: "dummy".to_string(),
                version: 1,
                source_path: "late.dummy".to_string(),
                cooked_path: "late.dummyc".to_string(),
                dependencies: Vec::new(),
                import_settings: serde_json::Value::Null,
            }],
        })?,
    )?;

    server.reload_manifest()?;
    assert_eq!(
        server.resolve_path(asset_root.join("late.dummy")),
        Some(asset_id)
    );
    Ok(())
}

#[test]
fn reload_manifest_invalidates_provider_package_cache() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let config = write_manifest_entries(dir.path(), Vec::new())?;
    let provider = InvalidationCountingProvider::default();
    let server = Assets::with_manifest_and_provider(
        config.clone(),
        AssetRegistryManifest::default(),
        provider.clone(),
    );
    assert_eq!(provider.invalidate_all_calls(), 0);

    std::fs::write(
        config.manifest_path(),
        serde_json::to_vec_pretty(&AssetRegistryManifest {
            version: ASSET_SYSTEM_VERSION,
            target: "native".to_string(),
            provenance: Vec::new(),
            assets: Vec::new(),
        })?,
    )?;

    server.reload_manifest()?;

    assert_eq!(provider.invalidate_all_calls(), 1);
    Ok(())
}

#[test]
fn reload_scan_and_force_reload_invalidate_provider_once_per_manifest_refresh(
) -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let asset_id = AssetId::new();
    let config = write_manifest(
        dir.path(),
        AssetManifestEntry {
            asset_id,
            asset_type: "dummy".to_string(),
            importer: "dummy".to_string(),
            cooker: "dummy".to_string(),
            version: 1,
            source_path: "clip.dummy".to_string(),
            cooked_path: "clip.dummyc".to_string(),
            dependencies: Vec::new(),
            import_settings: serde_json::Value::Null,
        },
    )?;
    let provider = InvalidationCountingProvider::default();
    let server = Assets::with_manifest_and_provider(
        config.clone(),
        load_manifest(&config)?,
        provider.clone(),
    );
    server.register_factory(DummyFactory);
    let _handle = server.load_id::<DummyAsset>(asset_id)?;

    let report = server.reload_changed_with_report()?;
    assert!(report.changed_roots.is_empty());
    assert_eq!(provider.invalidate_all_calls(), 1);

    let report = server.force_reload(asset_id)?;
    assert_eq!(report.changed_roots, vec![asset_id]);
    assert_eq!(provider.invalidate_all_calls(), 2);
    Ok(())
}

fn wait_for_terminal_texture(
    server: &Assets,
    handle: &Handle<TextureAsset>,
) -> Result<(), AssetError> {
    for _ in 0..64 {
        let _ = server.update();
        match server.state(handle) {
            AssetState::Installed | AssetState::Failed => return Ok(()),
            _ => std::thread::sleep(Duration::from_millis(5)),
        }
    }
    Err(AssetError::InvalidState {
        id: handle.id(),
        state: server.state(handle),
        message: "raw texture did not reach a terminal state".to_string(),
    })
}

fn wait_for_terminal_dummy(server: &Assets, handle: &Handle<DummyAsset>) -> Result<(), AssetError> {
    for _ in 0..64 {
        let _ = server.update();
        match server.state(handle) {
            AssetState::Installed | AssetState::Failed => return Ok(()),
            _ => std::thread::sleep(Duration::from_millis(5)),
        }
    }
    Err(AssetError::InvalidState {
        id: handle.id(),
        state: server.state(handle),
        message: "dummy asset did not reach a terminal state".to_string(),
    })
}

#[test]
fn dependency_cycle_marks_asset_failed() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let first_id = AssetId::new();
    let second_id = AssetId::new();
    let config = write_manifest_entries(
        dir.path(),
        vec![
            AssetManifestEntry {
                asset_id: first_id,
                asset_type: "dummy".to_string(),
                importer: "dummy".to_string(),
                cooker: "dummy".to_string(),
                version: 1,
                source_path: "first.dummy".to_string(),
                cooked_path: "first.dummyc".to_string(),
                dependencies: vec![second_id],
                import_settings: serde_json::Value::Null,
            },
            AssetManifestEntry {
                asset_id: second_id,
                asset_type: "dummy".to_string(),
                importer: "dummy".to_string(),
                cooker: "dummy".to_string(),
                version: 1,
                source_path: "second.dummy".to_string(),
                cooked_path: "second.dummyc".to_string(),
                dependencies: vec![first_id],
                import_settings: serde_json::Value::Null,
            },
        ],
    )?;
    std::fs::write(config.cooked_root().join("first.dummyc"), b"first")?;
    std::fs::write(config.cooked_root().join("second.dummyc"), b"second")?;

    let server = Assets::new(config)?;
    server.register_factory(DummyFactory);
    let handle = server.load_id::<DummyAsset>(first_id)?;
    let result = server.update();

    assert!(matches!(result, Err(AssetError::DependencyCycle { .. })));
    assert_eq!(server.state(&handle), AssetState::Failed);
    Ok(())
}

#[test]
fn background_loading_completes_on_later_updates() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let asset_id = AssetId::new();
    let config = write_manifest(
        dir.path(),
        AssetManifestEntry {
            asset_id,
            asset_type: "dummy".to_string(),
            importer: "dummy".to_string(),
            cooker: "dummy".to_string(),
            version: 1,
            source_path: "clip.dummy".to_string(),
            cooked_path: "clip.dummyc".to_string(),
            dependencies: Vec::new(),
            import_settings: serde_json::Value::Null,
        },
    )?
    .with_background_loading(true);
    std::fs::write(config.cooked_root().join("clip.dummyc"), b"ready")?;

    let server = Assets::new(config)?;
    server.register_factory(SlowFactory {
        delay: Duration::from_millis(60),
    });
    let handle = server.load_id::<DummyAsset>(asset_id)?;

    server.update()?;
    assert_eq!(server.state(&handle), AssetState::Loading);

    std::thread::sleep(Duration::from_millis(90));
    server.update()?;
    assert_eq!(server.state(&handle), AssetState::Installed);
    assert_eq!(server.get(&handle)?.0, "ready");
    Ok(())
}

#[test]
fn background_completion_after_release_is_discarded() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let asset_id = AssetId::new();
    let config = write_manifest(
        dir.path(),
        AssetManifestEntry {
            asset_id,
            asset_type: "dummy".to_string(),
            importer: "dummy".to_string(),
            cooker: "dummy".to_string(),
            version: 1,
            source_path: "clip.dummy".to_string(),
            cooked_path: "clip.dummyc".to_string(),
            dependencies: Vec::new(),
            import_settings: serde_json::Value::Null,
        },
    )?
    .with_background_loading(true);
    std::fs::write(config.cooked_root().join("clip.dummyc"), b"ready")?;

    let server = Assets::new(config)?;
    server.register_factory(SlowFactory {
        delay: Duration::from_millis(60),
    });
    let handle = server.load_id::<DummyAsset>(asset_id)?;

    server.update()?;
    assert_eq!(server.state(&handle), AssetState::Loading);
    assert_eq!(server.stats().inflight_loads, 1);

    drop(handle);
    server.update()?;
    assert_eq!(server.state_untyped(asset_id), AssetState::Unloaded);

    std::thread::sleep(Duration::from_millis(90));
    server.update()?;

    let stats = server.stats();
    assert_eq!(server.state_untyped(asset_id), AssetState::Unloaded);
    assert_eq!(stats.inflight_loads, 0);
    assert_eq!(stats.states.installed, 0);
    assert_eq!(stats.states.unloaded, 1);
    Ok(())
}

#[test]
fn burst_background_loads_stay_bounded_by_worker_pool_and_queue(
) -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let mut entries = Vec::new();
    for index in 0..12 {
        let asset_id = AssetId::new();
        entries.push(AssetManifestEntry {
            asset_id,
            asset_type: "dummy".to_string(),
            importer: "dummy".to_string(),
            cooker: "dummy".to_string(),
            version: 1,
            source_path: format!("burst-{index}.dummy"),
            cooked_path: format!("burst-{index}.dummyc"),
            dependencies: Vec::new(),
            import_settings: serde_json::Value::Null,
        });
    }
    let ids = entries
        .iter()
        .map(|entry| entry.asset_id)
        .collect::<Vec<_>>();
    let config = write_manifest_entries(dir.path(), entries.clone())?
        .with_background_loading(true)
        .with_io_worker_threads(2)
        .with_io_queue_capacity(3);
    for (index, entry) in entries.iter().enumerate() {
        std::fs::write(
            config.cooked_root().join(&entry.cooked_path),
            format!("ready-{index}"),
        )?;
    }

    let server = Assets::new(config)?;
    server.register_factory(SlowFactory {
        delay: Duration::from_millis(250),
    });
    let handles = ids
        .iter()
        .map(|id| server.load_id::<DummyAsset>(*id))
        .collect::<Result<Vec<_>, _>>()?;

    server.update()?;

    let bounded = server.stats();

    assert_eq!(bounded.load_worker_threads, 2);
    assert_eq!(bounded.load_queue_capacity, 3);
    assert!(bounded.inflight_loads <= bounded.load_worker_threads + bounded.load_queue_capacity);
    assert!(
        bounded.inflight_loads < handles.len(),
        "all burst loads entered in-flight state without backpressure: {bounded:?}"
    );
    assert_eq!(bounded.active_requests, handles.len());
    assert_eq!(
        bounded.source_load_phases.queued
            + bounded.source_load_phases.reading
            + bounded.source_load_phases.decoding,
        bounded.inflight_loads
    );
    assert!(bounded.deferred_load_submissions >= handles.len() - bounded.inflight_loads);
    assert_eq!(bounded.states.loading, handles.len());
    Ok(())
}

#[test]
fn asset_stats_report_queue_inflight_and_state_counts() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let asset_id = AssetId::new();
    let config = write_manifest(
        dir.path(),
        AssetManifestEntry {
            asset_id,
            asset_type: "dummy".to_string(),
            importer: "dummy".to_string(),
            cooker: "dummy".to_string(),
            version: 1,
            source_path: "clip.dummy".to_string(),
            cooked_path: "clip.dummyc".to_string(),
            dependencies: Vec::new(),
            import_settings: serde_json::Value::Null,
        },
    )?
    .with_background_loading(true);
    std::fs::write(config.cooked_root().join("clip.dummyc"), b"ready")?;

    let server = Assets::new(config)?;
    server.register_factory(SlowFactory {
        delay: Duration::from_millis(60),
    });
    let handle = server.load_id::<DummyAsset>(asset_id)?;

    let diagnostics = server.diagnostics_snapshot();
    let queued = diagnostics.stats.clone();
    assert_eq!(queued.records, 1);
    assert_eq!(queued.queued_requests, 1);
    assert_eq!(queued.active_requests, 0);
    assert_eq!(
        queued.load_worker_threads,
        server.config().io_worker_threads
    );
    assert_eq!(
        queued.load_queue_capacity,
        server.config().io_queue_capacity
    );
    assert_eq!(queued.submitted_requests, 1);
    assert_eq!(queued.activated_requests, 0);
    assert!(queued.oldest_queued_request_age.is_some());
    assert_eq!(queued.strong_references, 1);
    assert_eq!(queued.states.unloaded, 1);
    assert_eq!(diagnostics.queued_requests.len(), 1);
    assert!(diagnostics.active_requests.is_empty());
    assert!(diagnostics.failures.is_empty());
    assert!(diagnostics.last_reload_report.is_empty());
    let snapshots = server.queued_request_snapshots();
    assert_eq!(snapshots.len(), 1);
    assert_eq!(snapshots[0].request_id, 0);
    assert_eq!(snapshots[0].asset_id, asset_id);
    assert_eq!(snapshots[0].priority, server.config().io_default_priority);
    assert_eq!(
        snapshots[0].status,
        crate::asset::AssetRequestStatus::Queued
    );

    server.update()?;
    let loading = server.stats();
    assert_eq!(loading.queued_requests, 0);
    assert_eq!(loading.active_requests, 1);
    assert_eq!(loading.submitted_requests, 1);
    assert_eq!(loading.activated_requests, 1);
    assert_eq!(loading.oldest_queued_request_age, None);
    assert_eq!(loading.inflight_loads, 1);
    assert_eq!(loading.states.loading, 1);
    assert!(server.queued_request_snapshots().is_empty());
    let active = server.active_request_snapshots();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].request_id, 0);
    assert_eq!(active[0].asset_id, asset_id);
    assert_eq!(active[0].generation, 1);
    assert!(
        matches!(
            active[0].status,
            crate::asset::AssetRequestStatus::Loading | crate::asset::AssetRequestStatus::Decoding
        ),
        "active request may advance from source read to decode before diagnostics are sampled"
    );
    assert!(active[0].active_age.is_some());

    wait_for_terminal_dummy(&server, &handle)?;
    let installed = server.stats();
    assert_eq!(installed.active_requests, 0);
    assert_eq!(installed.inflight_loads, 0);
    assert_eq!(installed.states.installed, 1);
    assert_eq!(installed.load_timings.completed_source_loads, 1);
    assert_eq!(installed.load_timings.failed_source_loads, 0);
    assert!(installed.load_timings.average_total_time.is_some());
    assert!(installed.retained_events > 0);
    Ok(())
}

#[test]
fn load_blocking_completes_slow_asset_without_fixed_poll_limit(
) -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let asset_id = AssetId::new();
    let config = write_manifest(
        dir.path(),
        AssetManifestEntry {
            asset_id,
            asset_type: "dummy".to_string(),
            importer: "dummy".to_string(),
            cooker: "dummy".to_string(),
            version: 1,
            source_path: "clip.dummy".to_string(),
            cooked_path: "clip.dummyc".to_string(),
            dependencies: Vec::new(),
            import_settings: serde_json::Value::Null,
        },
    )?
    .with_background_loading(true);
    std::fs::write(config.cooked_root().join("clip.dummyc"), b"ready")?;

    let server = Assets::new(config)?;
    server.register_factory(SlowFactory {
        delay: Duration::from_millis(60),
    });

    let started = std::time::Instant::now();
    let asset = server.load_blocking::<DummyAsset>(asset_id)?;

    assert!(started.elapsed() >= Duration::from_millis(50));
    assert_eq!(asset.0, "ready");
    Ok(())
}

#[test]
fn load_blocking_timeout_reports_current_state() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let asset_id = AssetId::new();
    let config = write_manifest(
        dir.path(),
        AssetManifestEntry {
            asset_id,
            asset_type: "dummy".to_string(),
            importer: "dummy".to_string(),
            cooker: "dummy".to_string(),
            version: 1,
            source_path: "clip.dummy".to_string(),
            cooked_path: "clip.dummyc".to_string(),
            dependencies: Vec::new(),
            import_settings: serde_json::Value::Null,
        },
    )?
    .with_background_loading(true);
    std::fs::write(config.cooked_root().join("clip.dummyc"), b"ready")?;

    let server = Assets::new(config)?;
    server.register_factory(SlowFactory {
        delay: Duration::from_millis(80),
    });

    let result =
        server.load_blocking_with_timeout::<DummyAsset>(asset_id, Duration::from_millis(5));

    assert!(matches!(
        result,
        Err(AssetError::InvalidState {
            id,
            state: AssetState::Loading,
            ..
        }) if id == asset_id
    ));
    Ok(())
}

#[test]
fn load_blocking_installs_dependency_chain() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let parent_id = AssetId::new();
    let dependency_id = AssetId::new();
    let config = write_manifest_entries(
        dir.path(),
        vec![
            AssetManifestEntry {
                asset_id: parent_id,
                asset_type: "dummy".to_string(),
                importer: "dummy".to_string(),
                cooker: "dummy".to_string(),
                version: 1,
                source_path: "parent.dummy".to_string(),
                cooked_path: "parent.dummyc".to_string(),
                dependencies: vec![dependency_id],
                import_settings: serde_json::Value::Null,
            },
            AssetManifestEntry {
                asset_id: dependency_id,
                asset_type: "dummy".to_string(),
                importer: "dummy".to_string(),
                cooker: "dummy".to_string(),
                version: 1,
                source_path: "dependency.dummy".to_string(),
                cooked_path: "dependency.dummyc".to_string(),
                dependencies: Vec::new(),
                import_settings: serde_json::Value::Null,
            },
        ],
    )?;
    std::fs::write(config.cooked_root().join("parent.dummyc"), b"parent")?;
    std::fs::write(
        config.cooked_root().join("dependency.dummyc"),
        b"dependency",
    )?;

    let server = Assets::new(config)?;
    server.register_factory(DummyFactory);

    let asset = server.load_blocking::<DummyAsset>(parent_id)?;

    assert_eq!(asset.0, "parent");
    assert_eq!(server.state_untyped(parent_id), AssetState::Installed);
    assert_eq!(server.state_untyped(dependency_id), AssetState::Installed);
    Ok(())
}

#[test]
fn cooked_texture_load_uses_background_worker_even_without_global_background_loading(
) -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let asset_id = AssetId::new();
    let config = write_manifest(
        dir.path(),
        AssetManifestEntry {
            asset_id,
            asset_type: TextureAsset::TYPE.to_string(),
            importer: "texture".to_string(),
            cooker: "texture".to_string(),
            version: 1,
            source_path: "white.png".to_string(),
            cooked_path: "white.skytex".to_string(),
            dependencies: Vec::new(),
            import_settings: serde_json::Value::Null,
        },
    )?;
    std::fs::write(
        config.cooked_root().join("white.skytex"),
        crate::asset::texture::encode_texture_cooked(&TextureAsset::white_pixel()),
    )?;

    let server = Assets::new(config)?;
    server.register_factory(SlowTextureFactory {
        delay: Duration::from_millis(60),
    });
    let handle = server.load_id::<TextureAsset>(asset_id)?;
    server.update()?;
    assert_eq!(server.state(&handle), AssetState::Loading);
    assert_eq!(server.stats().inflight_loads, 1);

    wait_for_terminal_texture(&server, &handle)?;
    assert_eq!(server.state(&handle), AssetState::Installed);
    assert_eq!(server.get(&handle)?.size(), [1, 1]);
    Ok(())
}

#[test]
fn install_budget_limits_number_of_installs_per_update() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let first_id = AssetId::new();
    let second_id = AssetId::new();
    let config = write_manifest_entries(
        dir.path(),
        vec![
            AssetManifestEntry {
                asset_id: first_id,
                asset_type: "dummy".to_string(),
                importer: "dummy".to_string(),
                cooker: "dummy".to_string(),
                version: 1,
                source_path: "first.dummy".to_string(),
                cooked_path: "first.dummyc".to_string(),
                dependencies: Vec::new(),
                import_settings: serde_json::Value::Null,
            },
            AssetManifestEntry {
                asset_id: second_id,
                asset_type: "dummy".to_string(),
                importer: "dummy".to_string(),
                cooker: "dummy".to_string(),
                version: 1,
                source_path: "second.dummy".to_string(),
                cooked_path: "second.dummyc".to_string(),
                dependencies: Vec::new(),
                import_settings: serde_json::Value::Null,
            },
        ],
    )?
    .with_install_budget_per_update(1);
    std::fs::write(config.cooked_root().join("first.dummyc"), b"first")?;
    std::fs::write(config.cooked_root().join("second.dummyc"), b"second")?;

    let server = Assets::new(config)?;
    server.register_factory(DummyFactory);
    let first = server.load_id::<DummyAsset>(first_id)?;
    let second = server.load_id::<DummyAsset>(second_id)?;

    server.update()?;
    let states = [server.state(&first), server.state(&second)];
    let installed = states
        .iter()
        .filter(|state| **state == AssetState::Installed)
        .count();
    let installing = states
        .iter()
        .filter(|state| **state == AssetState::Installing)
        .count();
    assert_eq!(
        installed, 1,
        "states after first budgeted update: {states:?}"
    );
    assert_eq!(
        installing, 1,
        "states after first budgeted update: {states:?}"
    );

    server.update()?;
    assert_eq!(server.state(&first), AssetState::Installed);
    assert_eq!(server.state(&second), AssetState::Installed);
    Ok(())
}

#[test]
fn reload_changed_returns_empty_when_assets_are_unchanged() -> Result<(), Box<dyn std::error::Error>>
{
    let dir = tempdir()?;
    let asset_id = AssetId::new();
    let config = write_manifest(
        dir.path(),
        AssetManifestEntry {
            asset_id,
            asset_type: "dummy".to_string(),
            importer: "dummy".to_string(),
            cooker: "dummy".to_string(),
            version: 1,
            source_path: "clip.dummy".to_string(),
            cooked_path: "clip.dummyc".to_string(),
            dependencies: Vec::new(),
            import_settings: serde_json::Value::Null,
        },
    )?;
    std::fs::write(config.cooked_root().join("clip.dummyc"), b"ready")?;

    let server = Assets::new(config)?;
    server.register_factory(DummyFactory);
    let handle = server.load_id::<DummyAsset>(asset_id)?;
    server.update()?;

    let changed = server.reload_changed()?;
    assert!(changed.is_empty());
    let report = server.last_reload_report();
    assert!(report.changed_roots.is_empty());
    assert!(report.impacted.is_empty());
    assert_eq!(report.skipped.len(), 1);
    assert_eq!(report.skipped[0].asset_id, asset_id);
    assert_eq!(report.skipped[0].reason, AssetReloadSkipReason::Unchanged);
    assert_eq!(server.get(&handle)?.0, "ready");
    Ok(())
}

#[test]
fn reload_changed_reloads_modified_asset() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let asset_id = AssetId::new();
    let config = write_manifest(
        dir.path(),
        AssetManifestEntry {
            asset_id,
            asset_type: "dummy".to_string(),
            importer: "dummy".to_string(),
            cooker: "dummy".to_string(),
            version: 1,
            source_path: "clip.dummy".to_string(),
            cooked_path: "clip.dummyc".to_string(),
            dependencies: Vec::new(),
            import_settings: serde_json::Value::Null,
        },
    )?;
    let cooked_path = config.cooked_root().join("clip.dummyc");
    std::fs::write(&cooked_path, b"ready")?;

    let server = Assets::new(config)?;
    server.register_factory(DummyFactory);
    let handle = server.load_id::<DummyAsset>(asset_id)?;
    server.update()?;
    assert_eq!(server.get(&handle)?.0, "ready");

    std::fs::write(&cooked_path, b"fresh")?;
    let changed = server.reload_changed()?;
    assert_eq!(changed, vec![asset_id]);
    server.update()?;

    assert_eq!(server.get(&handle)?.0, "fresh");
    Ok(())
}

#[test]
fn failed_reload_keeps_last_good_installed_asset() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let asset_id = AssetId::new();
    let config = write_manifest(
        dir.path(),
        AssetManifestEntry {
            asset_id,
            asset_type: "dummy".to_string(),
            importer: "dummy".to_string(),
            cooker: "dummy".to_string(),
            version: 1,
            source_path: "clip.dummy".to_string(),
            cooked_path: "clip.dummyc".to_string(),
            dependencies: Vec::new(),
            import_settings: serde_json::Value::Null,
        },
    )?;
    let cooked_path = config.cooked_root().join("clip.dummyc");
    std::fs::write(&cooked_path, b"ready")?;

    let server = Assets::new(config)?;
    server.register_factory(DummyFactory);
    let mut events = server.event_cursor();
    let handle = server.load_id::<DummyAsset>(asset_id)?;
    server.update()?;
    assert_eq!(server.get(&handle)?.0, "ready");
    let _ = server.events_since(&mut events);

    std::fs::write(&cooked_path, [0xff, 0xfe, 0xfd])?;
    let changed = server.reload_changed()?;
    assert_eq!(changed, vec![asset_id]);
    let error = server
        .update()
        .expect_err("invalid reload payload should report a read failure");
    assert!(matches!(error, AssetError::InvalidCookedAsset { .. }));

    assert_eq!(server.state(&handle), AssetState::Installed);
    assert_eq!(server.get(&handle)?.0, "ready");
    assert!(matches!(
        server.error(&handle),
        Some(AssetError::InvalidCookedAsset { .. })
    ));
    assert_eq!(
        server.failure_phase(&handle),
        Some(AssetFailurePhase::Decode)
    );
    let failed_events = server.events_since(&mut events);
    let failed = failed_events
        .iter()
        .find(|event| event.id == asset_id && event.kind == AssetEventKind::Failed)
        .expect("failed reload should emit a diagnostic event");
    assert_eq!(failed.state, AssetState::Installed);
    assert_eq!(failed.failure_phase, Some(AssetFailurePhase::Decode));
    let failures = server.failed_asset_snapshots();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].asset_id, asset_id);
    assert_eq!(failures[0].state, AssetState::Installed);
    assert_eq!(failures[0].phase, Some(AssetFailurePhase::Decode));

    std::fs::write(&cooked_path, b"fresh")?;
    let changed = server.reload_changed()?;
    assert_eq!(changed, vec![asset_id]);
    server.update()?;
    assert_eq!(server.get(&handle)?.0, "fresh");
    assert!(server.error(&handle).is_none());
    assert!(server.failed_asset_snapshots().is_empty());
    Ok(())
}

#[test]
fn auto_reload_reloads_modified_asset_on_update() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let asset_id = AssetId::new();
    let config = write_manifest(
        dir.path(),
        AssetManifestEntry {
            asset_id,
            asset_type: "dummy".to_string(),
            importer: "dummy".to_string(),
            cooker: "dummy".to_string(),
            version: 1,
            source_path: "clip.dummy".to_string(),
            cooked_path: "clip.dummyc".to_string(),
            dependencies: Vec::new(),
            import_settings: serde_json::Value::Null,
        },
    )?
    .with_auto_reload(true)
    .with_auto_reload_interval(Duration::ZERO);
    let cooked_path = config.cooked_root().join("clip.dummyc");
    std::fs::write(&cooked_path, b"ready")?;

    let server = Assets::new(config)?;
    server.register_factory(DummyFactory);
    let handle = server.load_id::<DummyAsset>(asset_id)?;
    server.update()?;
    assert_eq!(server.get(&handle)?.0, "ready");
    assert!(server.last_reload_report().is_empty());

    std::fs::write(&cooked_path, b"fresh")?;
    server.update()?;

    assert_eq!(server.get(&handle)?.0, "fresh");
    let report = server.last_reload_report();
    assert_eq!(report.changed_roots, vec![asset_id]);
    assert_eq!(report.impacted, vec![asset_id]);
    Ok(())
}

#[test]
fn auto_reload_debounces_modified_asset_on_update() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let asset_id = AssetId::new();
    let config = write_manifest(
        dir.path(),
        AssetManifestEntry {
            asset_id,
            asset_type: "dummy".to_string(),
            importer: "dummy".to_string(),
            cooker: "dummy".to_string(),
            version: 1,
            source_path: "clip.dummy".to_string(),
            cooked_path: "clip.dummyc".to_string(),
            dependencies: Vec::new(),
            import_settings: serde_json::Value::Null,
        },
    )?
    .with_auto_reload(true)
    .with_auto_reload_interval(Duration::ZERO)
    .with_auto_reload_debounce(Duration::from_millis(40));
    let cooked_path = config.cooked_root().join("clip.dummyc");
    std::fs::write(&cooked_path, b"ready")?;

    let server = Assets::new(config)?;
    server.register_factory(DummyFactory);
    let handle = server.load_id::<DummyAsset>(asset_id)?;
    server.update()?;
    assert_eq!(server.get(&handle)?.0, "ready");

    std::fs::write(&cooked_path, b"fresh")?;
    server.update()?;
    assert_eq!(server.get(&handle)?.0, "ready");
    let pending = server.reload_status();
    assert_eq!(pending.pending_roots, vec![asset_id]);
    assert!(pending.last_report.is_empty());

    std::thread::sleep(Duration::from_millis(70));
    server.update()?;
    assert_eq!(server.get(&handle)?.0, "fresh");
    let reloaded = server.reload_status();
    assert!(reloaded.pending_roots.is_empty());
    assert_eq!(reloaded.last_report.changed_roots, vec![asset_id]);
    assert_eq!(reloaded.last_report.impacted, vec![asset_id]);
    Ok(())
}

#[test]
fn auto_reload_freeze_delays_scan_until_unfrozen() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let asset_id = AssetId::new();
    let config = write_manifest(
        dir.path(),
        AssetManifestEntry {
            asset_id,
            asset_type: "dummy".to_string(),
            importer: "dummy".to_string(),
            cooker: "dummy".to_string(),
            version: 1,
            source_path: "clip.dummy".to_string(),
            cooked_path: "clip.dummyc".to_string(),
            dependencies: Vec::new(),
            import_settings: serde_json::Value::Null,
        },
    )?
    .with_auto_reload(true)
    .with_auto_reload_interval(Duration::ZERO);
    let cooked_path = config.cooked_root().join("clip.dummyc");
    std::fs::write(&cooked_path, b"ready")?;

    let server = Assets::new(config)?;
    server.register_factory(DummyFactory);
    let handle = server.load_id::<DummyAsset>(asset_id)?;
    server.update()?;
    assert_eq!(server.get(&handle)?.0, "ready");

    server.set_auto_reload_frozen(true);
    std::fs::write(&cooked_path, b"fresh")?;
    server.update()?;
    assert_eq!(server.get(&handle)?.0, "ready");
    let frozen = server.reload_status();
    assert!(frozen.auto_reload_frozen);
    assert!(frozen.pending_roots.is_empty());

    server.set_auto_reload_frozen(false);
    server.update()?;
    assert_eq!(server.get(&handle)?.0, "fresh");
    let reloaded = server.reload_status();
    assert!(!reloaded.auto_reload_frozen);
    assert_eq!(reloaded.last_report.changed_roots, vec![asset_id]);
    Ok(())
}

#[cfg(feature = "asset-watch")]
#[test]
fn file_watcher_triggers_auto_reload_before_poll_interval() -> Result<(), Box<dyn std::error::Error>>
{
    let dir = tempdir()?;
    let asset_id = AssetId::new();
    let config = write_manifest(
        dir.path(),
        AssetManifestEntry {
            asset_id,
            asset_type: "dummy".to_string(),
            importer: "dummy".to_string(),
            cooker: "dummy".to_string(),
            version: 1,
            source_path: "clip.dummy".to_string(),
            cooked_path: "clip.dummyc".to_string(),
            dependencies: Vec::new(),
            import_settings: serde_json::Value::Null,
        },
    )?
    .with_auto_reload(true)
    .with_auto_reload_interval(Duration::from_secs(60))
    .with_file_watcher(true);
    let cooked_path = config.cooked_root().join("clip.dummyc");
    std::fs::write(&cooked_path, b"ready")?;

    let server = Assets::new(config)?;
    server.register_factory(DummyFactory);
    let handle = server.load_id::<DummyAsset>(asset_id)?;
    server.update()?;
    assert_eq!(server.get(&handle)?.0, "ready");

    std::fs::write(&cooked_path, b"fresh")?;
    for _ in 0..100 {
        server.update()?;
        if server.get(&handle)?.0 == "fresh" {
            let report = server.last_reload_report();
            assert_eq!(report.changed_roots, vec![asset_id]);
            assert_eq!(report.impacted, vec![asset_id]);
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(25));
    }

    panic!("asset watcher did not trigger reload before poll interval");
}

#[cfg(feature = "asset-watch")]
#[test]
fn file_watcher_triggers_auto_reload_for_external_package_root(
) -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let package_dir = tempdir()?;
    let asset_id = AssetId::new();
    let config = write_manifest(
        dir.path(),
        AssetManifestEntry {
            asset_id,
            asset_type: "dummy".to_string(),
            importer: "dummy".to_string(),
            cooker: "dummy".to_string(),
            version: 1,
            source_path: "clip.dummy".to_string(),
            cooked_path: "clip.dummyc".to_string(),
            dependencies: Vec::new(),
            import_settings: serde_json::Value::Null,
        },
    )?
    .with_package_root(package_dir.path())
    .with_auto_reload(true)
    .with_auto_reload_interval(Duration::from_secs(60))
    .with_file_watcher(true);
    let package_cooked_path = package_dir.path().join("clip.dummyc");
    std::fs::write(&package_cooked_path, b"ready")?;

    let server = Assets::new(config)?;
    server.register_factory(DummyFactory);
    let handle = server.load_id::<DummyAsset>(asset_id)?;
    server.update()?;
    assert_eq!(server.get(&handle)?.0, "ready");

    std::fs::write(&package_cooked_path, b"fresh")?;
    for _ in 0..100 {
        server.update()?;
        if server.get(&handle)?.0 == "fresh" {
            let report = server.last_reload_report();
            assert_eq!(report.changed_roots, vec![asset_id]);
            assert_eq!(report.impacted, vec![asset_id]);
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(25));
    }

    panic!("external package root watcher did not trigger reload before poll interval");
}

#[cfg(feature = "asset-watch")]
#[test]
fn file_watcher_triggers_auto_reload_for_external_package_file(
) -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let package_dir = tempdir()?;
    let asset_id = AssetId::new();
    let package_file = package_dir.path().join("base.skybundle");
    let config = write_manifest(
        dir.path(),
        AssetManifestEntry {
            asset_id,
            asset_type: "dummy".to_string(),
            importer: "dummy".to_string(),
            cooker: "dummy".to_string(),
            version: 1,
            source_path: "clip.dummy".to_string(),
            cooked_path: "clip.dummyc".to_string(),
            dependencies: Vec::new(),
            import_settings: serde_json::Value::Null,
        },
    )?
    .with_package_file(&package_file)
    .with_auto_reload(true)
    .with_auto_reload_interval(Duration::from_secs(60))
    .with_file_watcher(true);
    write_test_bundle(&package_file, &[("clip.dummyc", b"ready")])?;

    let server = Assets::new(config)?;
    server.register_factory(DummyFactory);
    let handle = server.load_id::<DummyAsset>(asset_id)?;
    server.update()?;
    assert_eq!(server.get(&handle)?.0, "ready");

    write_test_bundle(&package_file, &[("clip.dummyc", b"fresh")])?;
    for _ in 0..100 {
        server.update()?;
        if server.get(&handle)?.0 == "fresh" {
            let report = server.last_reload_report();
            assert_eq!(report.changed_roots, vec![asset_id]);
            assert_eq!(report.impacted, vec![asset_id]);
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(25));
    }

    panic!("external package file watcher did not trigger reload before poll interval");
}

#[test]
fn force_reload_reloads_asset_even_when_hash_is_unchanged() -> Result<(), Box<dyn std::error::Error>>
{
    let dir = tempdir()?;
    let asset_id = AssetId::new();
    let config = write_manifest(
        dir.path(),
        AssetManifestEntry {
            asset_id,
            asset_type: "dummy".to_string(),
            importer: "dummy".to_string(),
            cooker: "dummy".to_string(),
            version: 1,
            source_path: "clip.dummy".to_string(),
            cooked_path: "clip.dummyc".to_string(),
            dependencies: Vec::new(),
            import_settings: serde_json::Value::Null,
        },
    )?;
    std::fs::write(config.cooked_root().join("clip.dummyc"), b"ready")?;

    let server = Assets::new(config)?;
    let factory = CountingFactory::new();
    server.register_factory(factory.clone());
    let handle = server.load_id::<DummyAsset>(asset_id)?;
    server.update()?;
    assert_eq!(server.get(&handle)?.0, "ready");
    assert_eq!(factory.load_count(asset_id), 1);

    let report = server.force_reload(asset_id)?;
    assert_eq!(report.changed_roots, vec![asset_id]);
    assert_eq!(report.impacted, vec![asset_id]);
    server.update()?;
    assert_eq!(server.get(&handle)?.0, "ready");
    assert_eq!(factory.load_count(asset_id), 2);
    assert_eq!(factory.install_count(asset_id), 2);
    Ok(())
}

#[test]
fn reload_changed_reloads_dependents_of_changed_dependency(
) -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let parent_id = AssetId::new();
    let dependency_id = AssetId::new();
    let config = write_manifest_entries(
        dir.path(),
        vec![
            AssetManifestEntry {
                asset_id: parent_id,
                asset_type: "dummy".to_string(),
                importer: "dummy".to_string(),
                cooker: "dummy".to_string(),
                version: 1,
                source_path: "parent.dummy".to_string(),
                cooked_path: "parent.dummyc".to_string(),
                dependencies: vec![dependency_id],
                import_settings: serde_json::Value::Null,
            },
            AssetManifestEntry {
                asset_id: dependency_id,
                asset_type: "dummy".to_string(),
                importer: "dummy".to_string(),
                cooker: "dummy".to_string(),
                version: 1,
                source_path: "dependency.dummy".to_string(),
                cooked_path: "dependency.dummyc".to_string(),
                dependencies: Vec::new(),
                import_settings: serde_json::Value::Null,
            },
        ],
    )?;
    std::fs::write(config.cooked_root().join("parent.dummyc"), b"parent")?;
    let dependency_path = config.cooked_root().join("dependency.dummyc");
    std::fs::write(&dependency_path, b"dependency-v1")?;

    let server = Assets::new(config)?;
    let factory = CountingFactory::new();
    server.register_factory(factory.clone());
    let parent = server.load_id::<DummyAsset>(parent_id)?;
    server.update()?;
    assert_eq!(server.get(&parent)?.0, "parent");
    assert_eq!(factory.load_count(parent_id), 1);
    assert_eq!(factory.load_count(dependency_id), 1);
    assert_eq!(factory.install_count(parent_id), 1);
    assert_eq!(factory.install_count(dependency_id), 1);

    std::fs::write(&dependency_path, b"dependency-v2")?;
    let report = server.reload_changed_with_report()?;
    assert_eq!(report.changed_roots, vec![dependency_id]);
    let changed_ids: HashSet<_> = report.impacted.into_iter().collect();
    assert_eq!(changed_ids, HashSet::from([parent_id, dependency_id]));

    server.update()?;
    assert_eq!(factory.load_count(parent_id), 2);
    assert_eq!(factory.load_count(dependency_id), 2);
    assert_eq!(factory.install_count(parent_id), 2);
    assert_eq!(factory.install_count(dependency_id), 2);
    Ok(())
}

#[test]
fn large_dependency_graph_loads_reloads_and_releases_closure(
) -> Result<(), Box<dyn std::error::Error>> {
    const ASSET_COUNT: usize = 63;

    let dir = tempdir()?;
    let ids: Vec<_> = (0..ASSET_COUNT).map(|_| AssetId::new()).collect();
    let entries = ids
        .iter()
        .enumerate()
        .map(|(index, &asset_id)| {
            let dependencies = [index * 2 + 1, index * 2 + 2]
                .into_iter()
                .filter_map(|child| ids.get(child).copied())
                .collect();
            AssetManifestEntry {
                asset_id,
                asset_type: "dummy".to_string(),
                importer: "dummy".to_string(),
                cooker: "dummy".to_string(),
                version: 1,
                source_path: format!("asset-{index}.dummy"),
                cooked_path: format!("asset-{index}.dummyc"),
                dependencies,
                import_settings: serde_json::Value::Null,
            }
        })
        .collect();
    let config = write_manifest_entries(dir.path(), entries)?;
    let cooked_root = config.cooked_root().to_path_buf();
    for index in 0..ASSET_COUNT {
        std::fs::write(
            cooked_root.join(format!("asset-{index}.dummyc")),
            format!("asset-{index}-v1"),
        )?;
    }

    let server = Assets::new(config)?;
    let factory = CountingFactory::new();
    server.register_factory(factory.clone());
    let root = server.load_id::<DummyAsset>(ids[0])?;
    server.update()?;

    assert_eq!(server.get(&root)?.0, "asset-0-v1");
    assert_eq!(server.stats().states.installed, ASSET_COUNT);
    for &id in &ids {
        assert_eq!(server.state_untyped(id), AssetState::Installed);
        assert_eq!(factory.load_count(id), 1);
        assert_eq!(factory.install_count(id), 1);
    }

    let leaf_indexes: Vec<_> = (0..ASSET_COUNT)
        .filter(|index| index * 2 + 1 >= ASSET_COUNT)
        .collect();
    for &index in &leaf_indexes {
        std::fs::write(
            cooked_root.join(format!("asset-{index}.dummyc")),
            format!("asset-{index}-v2"),
        )?;
    }

    let report = server.reload_changed_with_report()?;
    let changed_roots: HashSet<_> = report.changed_roots.into_iter().collect();
    let expected_roots: HashSet<_> = leaf_indexes.iter().map(|&index| ids[index]).collect();
    assert_eq!(changed_roots, expected_roots);
    let impacted: HashSet<_> = report.impacted.into_iter().collect();
    let expected_impacted: HashSet<_> = ids.iter().copied().collect();
    assert_eq!(impacted, expected_impacted);

    for _ in 0..ASSET_COUNT {
        server.update()?;
        if ids.iter().all(|&id| factory.install_count(id) == 2) {
            break;
        }
    }

    assert_eq!(server.get(&root)?.0, "asset-0-v1");
    assert_eq!(server.stats().states.installed, ASSET_COUNT);
    for &id in &ids {
        assert_eq!(server.state_untyped(id), AssetState::Installed);
        assert_eq!(factory.load_count(id), 2);
        assert_eq!(factory.install_count(id), 2);
    }

    drop(root);
    server.update()?;
    assert_eq!(server.stats().states.unloaded, ASSET_COUNT);
    Ok(())
}
