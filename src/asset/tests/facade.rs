use crate::asset::registry::AssetRuntimeFactory;
use crate::asset::{
    Asset, AssetConfig, AssetError, AssetEventKind, AssetId, AssetInstallContext,
    AssetInstallResult, AssetLoadContext, AssetManifestEntry, AssetPath, AssetRegistryManifest,
    AssetState, Assets, FontAsset, LoadedAsset, ASSET_SYSTEM_VERSION,
};
use std::path::{Path, PathBuf};
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
fn typed_asset_path_serializes_resolves_and_loads() -> Result<(), Box<dyn std::error::Error>> {
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
    .with_package_root("packages/base")
    .with_package_file("bundles/base.skybundle");
    let expected_source_path = config.asset_root.join("clip.dummy");
    let expected_cooked_path = config.cooked_root().join("clip.dummyc");
    let expected_package_path = config.asset_root.join("packages/base/clip.dummyc");
    let expected_package_file = config.asset_root.join("bundles/base.skybundle");
    std::fs::write(config.cooked_root().join("clip.dummyc"), b"ready")?;

    let server = Assets::new(config)?;
    server.register_factory(DummyFactory);
    let path = AssetPath::<DummyAsset>::new("clip.dummy");
    let serialized = serde_json::to_string(&path)?;
    assert_eq!(serialized, "\"clip.dummy\"");
    let decoded: AssetPath<DummyAsset> = serde_json::from_str(&serialized)?;
    assert_eq!(decoded.as_path(), Path::new("clip.dummy"));

    assert_eq!(
        server.source_path(asset_id),
        Some(PathBuf::from("clip.dummy"))
    );
    assert_eq!(server.asset_path::<DummyAsset>(asset_id)?, path);
    let metadata = server.metadata(asset_id).expect("metadata should resolve");
    assert_eq!(metadata.asset_id, asset_id);
    assert_eq!(metadata.asset_type, "dummy");
    assert_eq!(metadata.source_path, "clip.dummy");
    assert_eq!(metadata.cooked_path, "clip.dummyc");
    assert!(metadata.dependencies.is_empty());
    let watch_paths = server.watch_paths(asset_id).expect("watch paths");
    assert_eq!(watch_paths.source_path, expected_source_path);
    assert_eq!(watch_paths.cooked_path, expected_cooked_path);
    assert_eq!(watch_paths.package_paths, vec![expected_package_path]);
    assert_eq!(watch_paths.package_files, vec![expected_package_file]);

    let weak = server.resolve_asset_path(&decoded)?;
    assert_eq!(weak.id(), asset_id);
    let handle = server.load_path(&path)?;
    assert_eq!(handle.id(), asset_id);
    server.update()?;
    assert_eq!(server.get(&handle)?.0, "ready");

    let wrong_type_path = AssetPath::<FontAsset>::new("clip.dummy");
    let err = server.resolve_asset_path(&wrong_type_path).unwrap_err();
    assert!(matches!(
        err,
        AssetError::AssetTypeMismatch {
            expected,
            actual,
            ..
        } if expected == FontAsset::TYPE && actual == "dummy"
    ));
    let err = server.asset_path::<FontAsset>(asset_id).unwrap_err();
    assert!(matches!(
        err,
        AssetError::AssetTypeMismatch {
            expected,
            actual,
            ..
        } if expected == FontAsset::TYPE && actual == "dummy"
    ));
    Ok(())
}

#[test]
fn runtime_assets_are_installed_immediately_and_survive_manifest_reload(
) -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let config = write_manifest_entries(dir.path(), Vec::new())?;
    let server = Assets::new(config)?;
    let mut events = server.event_cursor();
    let handle = server.insert_runtime(DummyAsset("runtime".to_string()));
    let installed_events = server.events_since(&mut events);
    assert_eq!(installed_events.len(), 1);
    assert_eq!(installed_events[0].id, handle.id());
    assert_eq!(installed_events[0].kind, AssetEventKind::Installed);

    assert_eq!(server.state(&handle), AssetState::Installed);
    assert_eq!(server.get(&handle)?.0, "runtime");

    server.reload_manifest()?;
    assert!(server.events_since(&mut events).is_empty());
    assert_eq!(server.state(&handle), AssetState::Installed);
    assert_eq!(server.get(&handle)?.0, "runtime");

    let handle_id = handle.id();
    drop(handle);
    server.update()?;
    let unloaded_events = server.events_since(&mut events);
    assert!(unloaded_events
        .iter()
        .any(|event| { event.id == handle_id && event.kind == AssetEventKind::Unloaded }));
    assert_eq!(server.state_untyped(handle_id), AssetState::Unloaded);
    Ok(())
}

#[test]
fn asset_io_config_controls_worker_pool() {
    let dir = tempdir().expect("temporary asset root");
    let config = AssetConfig::new(dir.path(), "native")
        .with_io_worker_threads(1)
        .with_io_queue_capacity(3)
        .with_io_default_priority(7);
    let server = Assets::with_empty_manifest(config);
    let inner = server.inner.lock().expect("assets mutex poisoned");

    assert_eq!(inner.load_queue.worker_count(), 1);
    assert_eq!(inner.load_queue.queue_capacity(), 3);
    assert_eq!(inner.config.io_default_priority, 7);
}
