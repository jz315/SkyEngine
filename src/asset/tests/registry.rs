use crate::asset::registry::*;
use std::any::TypeId;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::asset::events::AssetEventLog;
use crate::asset::install::{AssetInstallContext, AssetInstallResult};
use crate::asset::request::{AssetRequestPhase, AssetRequests};
use crate::asset::store::{AssetRecord, AssetStore};
use crate::asset::types::{
    AssetEventKind, AssetLoadContext, AssetManifestEntry, AssetRegistryManifest,
    AssetRequestStatus, AssetState, LoadedAsset,
};
use crate::asset::{
    Asset, AssetConfig, AssetError, AssetFailurePhase, AssetId, ASSET_SYSTEM_VERSION,
};
use tempfile::tempdir;

struct RegistryDummyAsset;

impl Asset for RegistryDummyAsset {
    const TYPE: &'static str = "registry.dummy";
}

struct RegistryOtherAsset;

impl Asset for RegistryOtherAsset {
    const TYPE: &'static str = "registry.other";
}

struct RegistryDummyFactory;

impl AssetRuntimeFactory for RegistryDummyFactory {
    type Asset = RegistryDummyAsset;
    type Loaded = ();

    fn load(&self, _ctx: AssetLoadContext<'_>) -> Result<LoadedAsset<Self::Loaded>, AssetError> {
        Ok(LoadedAsset::new(()))
    }

    fn begin_install(
        &self,
        _loaded: &Self::Loaded,
        _ctx: AssetInstallContext<'_>,
    ) -> Result<AssetInstallResult<Self::Asset>, AssetError> {
        Ok(AssetInstallResult::Ready(RegistryDummyAsset))
    }
}

struct RegistryOtherFactory;

impl AssetRuntimeFactory for RegistryOtherFactory {
    type Asset = RegistryOtherAsset;
    type Loaded = ();

    fn load(&self, _ctx: AssetLoadContext<'_>) -> Result<LoadedAsset<Self::Loaded>, AssetError> {
        Ok(LoadedAsset::new(()))
    }

    fn begin_install(
        &self,
        _loaded: &Self::Loaded,
        _ctx: AssetInstallContext<'_>,
    ) -> Result<AssetInstallResult<Self::Asset>, AssetError> {
        Ok(AssetInstallResult::Ready(RegistryOtherAsset))
    }
}

struct RegistryLyingFactory;

impl AssetRuntimeFactory for RegistryLyingFactory {
    type Asset = RegistryOtherAsset;
    type Loaded = ();

    fn asset_type(&self) -> &'static str {
        RegistryDummyAsset::TYPE
    }

    fn load(&self, _ctx: AssetLoadContext<'_>) -> Result<LoadedAsset<Self::Loaded>, AssetError> {
        Ok(LoadedAsset::new(()))
    }

    fn begin_install(
        &self,
        _loaded: &Self::Loaded,
        _ctx: AssetInstallContext<'_>,
    ) -> Result<AssetInstallResult<Self::Asset>, AssetError> {
        Ok(AssetInstallResult::Ready(RegistryOtherAsset))
    }
}

fn test_entry(id: AssetId, asset_type: &'static str) -> AssetManifestEntry {
    AssetManifestEntry {
        asset_id: id,
        asset_type: asset_type.to_string(),
        importer: "registry.importer".to_string(),
        cooker: "registry.cooker".to_string(),
        version: 1,
        source_path: format!("{id}.source"),
        cooked_path: format!("{id}.cooked"),
        dependencies: Vec::new(),
        import_settings: serde_json::Value::Null,
    }
}

fn manifest(entries: Vec<AssetManifestEntry>) -> AssetRegistryManifest {
    AssetRegistryManifest {
        version: crate::asset::ASSET_SYSTEM_VERSION,
        target: "native".to_string(),
        provenance: Vec::new(),
        assets: entries,
    }
}

#[test]
fn asset_factories_register_and_lookup_entry_factory() {
    let id = AssetId::new();
    let entry = test_entry(id, RegistryDummyAsset::TYPE);
    let mut factories = AssetFactories::default();

    factories.register(RegistryDummyFactory);

    let factory = factories
        .for_entry(&entry)
        .expect("dummy factory should be registered");
    assert_eq!(factory.asset_type(), RegistryDummyAsset::TYPE);
    assert_eq!(
        factory.product_type_id(),
        TypeId::of::<RegistryDummyAsset>()
    );
    factories
        .validate_entry_product::<RegistryDummyAsset>(id, &entry)
        .expect("entry product type should match typed request");
    factories
        .ensure_registered_product::<RegistryDummyAsset>()
        .expect("registered product type should match asset type");
}

#[test]
fn asset_factories_report_missing_registered_asset_type() {
    let id = AssetId::new();
    let entry = test_entry(id, RegistryDummyAsset::TYPE);
    let factories = AssetFactories::default();

    match factories.for_entry(&entry) {
        Err(AssetError::FactoryNotRegistered { asset_type }) => {
            assert_eq!(asset_type, RegistryDummyAsset::TYPE);
        }
        Ok(_) => panic!("missing factory should not resolve"),
        Err(error) => panic!("unexpected error: {error}"),
    }

    match factories.ensure_registered_product::<RegistryDummyAsset>() {
        Err(AssetError::FactoryNotRegistered { asset_type }) => {
            assert_eq!(asset_type, RegistryDummyAsset::TYPE);
        }
        Ok(_) => panic!("missing factory should not validate"),
        Err(error) => panic!("unexpected error: {error}"),
    }
}

#[test]
fn asset_factories_validate_entry_product_reports_type_mismatch() {
    let id = AssetId::new();
    let entry = test_entry(id, RegistryOtherAsset::TYPE);
    let mut factories = AssetFactories::default();

    factories.register(RegistryOtherFactory);

    match factories.validate_entry_product::<RegistryDummyAsset>(id, &entry) {
        Err(AssetError::AssetTypeMismatch {
            id: error_id,
            expected,
            actual,
        }) => {
            assert_eq!(error_id, id);
            assert_eq!(expected, RegistryDummyAsset::TYPE);
            assert_eq!(actual, RegistryOtherAsset::TYPE);
        }
        Ok(_) => panic!("mismatched product type should not validate"),
        Err(error) => panic!("unexpected error: {error}"),
    }
}

#[test]
fn registry_validates_typed_request_from_manifest_entry() {
    let id = AssetId::new();
    let entry = test_entry(id, RegistryDummyAsset::TYPE);
    let registry = ManifestIndex::new(manifest(vec![entry]));
    let mut factories = AssetFactories::default();
    factories.register(RegistryDummyFactory);

    validate_typed_asset_request::<RegistryDummyAsset, _>(&registry, &factories, id)
        .expect("typed request should match manifest entry product");

    let missing = AssetId::new();
    match validate_typed_asset_request::<RegistryDummyAsset, _>(&registry, &factories, missing) {
        Err(AssetError::AssetNotFound { id: error_id }) => assert_eq!(error_id, missing),
        Ok(_) => panic!("missing typed request should fail"),
        Err(error) => panic!("unexpected error: {error}"),
    }
}

#[test]
fn registry_resolves_typed_source_asset_and_asset_path() {
    let id = AssetId::new();
    let entry = test_entry(id, RegistryDummyAsset::TYPE);
    let source_path = entry.source_path.clone();
    let config = AssetConfig::new("assets", "native");
    let registry = ManifestIndex::new(manifest(vec![entry]));
    let mut factories = AssetFactories::default();
    factories.register(RegistryDummyFactory);

    let resolved = resolve_typed_source_asset::<RegistryDummyAsset, _>(
        &config,
        &registry,
        &factories,
        Path::new(&source_path),
    )
    .expect("source path should resolve and validate");
    assert_eq!(resolved, id);

    let typed_path = typed_asset_path::<RegistryDummyAsset, _>(&registry, &factories, id)
        .expect("typed asset path should validate");
    assert_eq!(typed_path.as_path(), Path::new(&source_path));

    match resolve_typed_source_asset::<RegistryDummyAsset, _>(
        &config,
        &registry,
        &factories,
        Path::new("missing.source"),
    ) {
        Err(AssetError::AssetPathNotFound { path }) => {
            assert_eq!(path, PathBuf::from("missing.source"));
        }
        Ok(_) => panic!("missing source path should fail"),
        Err(error) => panic!("unexpected error: {error}"),
    }
}

#[test]
fn asset_factories_detect_registered_product_type_mismatch() {
    let mut factories = AssetFactories::default();

    factories.register(RegistryLyingFactory);

    match factories.ensure_registered_product::<RegistryDummyAsset>() {
        Err(AssetError::Internal { message }) => {
            assert_eq!(
                    message,
                    "registered `registry.dummy` factory product type does not match requested asset type"
                );
        }
        Ok(_) => panic!("factory product type mismatch should fail"),
        Err(error) => panic!("unexpected error: {error}"),
    }
}

#[test]
fn refresh_manifest_records_updates_bound_records_and_reports_missing() {
    let retained = AssetId::new();
    let missing = AssetId::new();
    let raw = AssetId::new();
    let initial_entry = test_entry(retained, RegistryDummyAsset::TYPE);
    let config = AssetConfig::new("assets", "native");
    let mut index = LocalManifestRegistry::new(manifest(vec![initial_entry.clone()]));
    assert_eq!(
        index.lookup_source_asset(&config, Path::new(&initial_entry.source_path)),
        Some(retained)
    );
    let mut store = AssetStore::default();
    store.records.insert(
        retained,
        AssetRecord::new(retained, RegistryDummyAsset::TYPE.to_string()),
    );
    store.records.insert(
        missing,
        AssetRecord::new(missing, RegistryDummyAsset::TYPE.to_string()),
    );
    store.records.insert(
        raw,
        AssetRecord::new_raw_texture(PathBuf::from("loose.png")),
    );

    let refresh = refresh_manifest_records(
        &mut index,
        &mut store,
        manifest(vec![test_entry(retained, RegistryOtherAsset::TYPE)]),
    );

    assert_eq!(refresh.missing_records, vec![missing]);
    assert_eq!(
        store
            .records
            .get(&retained)
            .expect("retained record exists")
            .asset_type,
        RegistryOtherAsset::TYPE
    );
    assert!(store.records.contains_key(&raw));
    assert!(index.entry(missing).is_none());
}

#[test]
fn refresh_manifest_records_or_fail_records_missing_manifest_failure() {
    let retained = AssetId::new();
    let missing = AssetId::new();
    let now = Instant::now();
    let initial_entry = test_entry(retained, RegistryDummyAsset::TYPE);
    let mut index = LocalManifestRegistry::new(manifest(vec![initial_entry]));
    let mut store = AssetStore::default();
    store.records.insert(
        retained,
        AssetRecord::new(retained, RegistryDummyAsset::TYPE.to_string()),
    );
    let mut missing_record = AssetRecord::new(missing, RegistryDummyAsset::TYPE.to_string());
    missing_record.state = AssetState::Installed;
    missing_record.strong_ref_count = 1;
    missing_record.load_generation = 9;
    store.records.insert(missing, missing_record);
    let mut events = AssetEventLog::new(8);
    let mut cursor = events.cursor();
    let mut requests = AssetRequests::default();
    requests.enqueue(missing, 9, None, 4, now);
    let request = requests.pop_queued().expect("queued request");
    requests.activate(request, AssetRequestPhase::Installing, 9, now);

    let refresh = refresh_manifest_records_or_fail(
        &mut index,
        &mut store,
        &mut events,
        &mut requests,
        manifest(vec![test_entry(retained, RegistryOtherAsset::TYPE)]),
        4,
        now,
    );

    assert_eq!(refresh.missing_records, vec![missing]);
    assert_eq!(store.records[&missing].state, AssetState::Failed);
    assert_eq!(
        store.records[&missing].failure_phase,
        Some(AssetFailurePhase::Lookup)
    );
    assert_eq!(
        store.records[&retained].asset_type,
        RegistryOtherAsset::TYPE
    );
    let emitted = events.events_since(&mut cursor);
    assert_eq!(emitted.len(), 1);
    assert_eq!(emitted[0].id, missing);
    assert_eq!(emitted[0].kind, AssetEventKind::Failed);
    assert_eq!(emitted[0].failure_phase, Some(AssetFailurePhase::Lookup));
    let failed = requests.failed_snapshots();
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].asset_id, missing);
    assert_eq!(failed[0].generation, 9);
    assert_eq!(failed[0].status, AssetRequestStatus::Failed);
}

#[test]
fn manifest_index_resolves_source_cooked_and_package_watch_paths() {
    let dir = tempdir().expect("temporary asset root");
    let config = AssetConfig::new(dir.path(), "native").with_package_root("packages/base");
    let id = AssetId::new();
    let entry = AssetManifestEntry {
        asset_id: id,
        asset_type: RegistryDummyAsset::TYPE.to_string(),
        importer: "registry.importer".to_string(),
        cooker: "registry.cooker".to_string(),
        version: 1,
        source_path: "source/hero.dummy".to_string(),
        cooked_path: "hero.dummyc".to_string(),
        dependencies: Vec::new(),
        import_settings: serde_json::Value::Null,
    };
    let index = LocalManifestRegistry::new(manifest(vec![entry]));

    assert_eq!(
        AssetRegistry::lookup_watch_asset(
            &index,
            &config,
            &config.asset_root.join("source/hero.dummy")
        ),
        Some(id)
    );
    assert_eq!(
        AssetRegistry::lookup_watch_asset(
            &index,
            &config,
            &config.cooked_root().join("hero.dummyc")
        ),
        Some(id)
    );
    assert_eq!(
        AssetRegistry::lookup_watch_asset(
            &index,
            &config,
            &config.asset_root.join("packages/base/hero.dummyc")
        ),
        Some(id)
    );
    assert_eq!(
        AssetRegistry::lookup_watch_asset(
            &index,
            &config,
            &config.asset_root.join("untracked.dummy")
        ),
        None
    );
}

#[test]
fn local_manifest_registry_resolves_metadata_dependencies_location_and_watch_key() {
    let dir = tempdir().expect("temporary asset root");
    let config = AssetConfig::new(dir.path(), "native")
        .with_package_root("packages/base")
        .with_package_file("bundles/base.skybundle");
    let id = AssetId::new();
    let dependency = AssetId::new();
    let entry = AssetManifestEntry {
        asset_id: id,
        asset_type: RegistryDummyAsset::TYPE.to_string(),
        importer: "registry.importer".to_string(),
        cooker: "registry.cooker".to_string(),
        version: 7,
        source_path: "source/hero.dummy".to_string(),
        cooked_path: "hero.dummyc".to_string(),
        dependencies: vec![dependency],
        import_settings: serde_json::Value::Null,
    };
    let registry = LocalManifestRegistry::new(manifest(vec![entry]));
    let registry_view: &dyn AssetRegistry = &registry;

    let metadata = registry_view.metadata(id).expect("metadata should resolve");
    assert_eq!(metadata.asset_id, id);
    assert_eq!(metadata.asset_type, RegistryDummyAsset::TYPE);
    assert_eq!(metadata.importer, "registry.importer");
    assert_eq!(metadata.cooker, "registry.cooker");
    assert_eq!(metadata.version, 7);
    assert_eq!(metadata.source_path, "source/hero.dummy");
    assert_eq!(metadata.cooked_path, "hero.dummyc");
    assert_eq!(
        registry_view.dependencies(id).expect("dependencies"),
        &[dependency]
    );

    let location = registry_view
        .resolve_location(&config, id)
        .expect("location should resolve");
    assert_eq!(
        location.source_path,
        config.asset_root.join("source/hero.dummy")
    );
    assert_eq!(
        location.cooked_path,
        config.cooked_root().join("hero.dummyc")
    );
    assert_eq!(
        location.package_paths,
        vec![config.asset_root.join("packages/base/hero.dummyc")]
    );
    assert_eq!(
        location.package_files,
        vec![config.asset_root.join("bundles/base.skybundle")]
    );

    let watch_key = registry_view.watch_key(&config, id).expect("watch key");
    assert_eq!(watch_key.source_path, location.source_path);
    assert_eq!(watch_key.cooked_path, location.cooked_path);
    assert_eq!(watch_key.package_paths, location.package_paths);
    assert_eq!(watch_key.package_files, location.package_files);
    assert!(registry_view.metadata(dependency).is_none());
    assert!(registry_view.dependencies(dependency).is_none());
    assert!(registry_view
        .resolve_location(&config, dependency)
        .is_none());
    assert!(registry_view.watch_key(&config, dependency).is_none());
}

#[test]
fn load_manifest_returns_empty_when_no_cooked_root_exists() {
    let dir = tempdir().expect("temporary asset root");
    let config = AssetConfig::new(dir.path(), "native");

    let manifest = load_manifest(&config).expect("missing manifest without cooked root");

    assert_eq!(manifest.version, ASSET_SYSTEM_VERSION);
    assert_eq!(manifest.target, "native");
    assert!(manifest.assets.is_empty());
}

#[test]
fn load_manifest_rejects_version_mismatch() {
    let dir = tempdir().expect("temporary asset root");
    let config = AssetConfig::new(dir.path(), "native");
    std::fs::create_dir_all(config.cooked_root()).expect("cooked root");
    std::fs::write(
        config.manifest_path(),
        serde_json::json!({
            "version": ASSET_SYSTEM_VERSION + 1,
            "target": "native",
            "assets": []
        })
        .to_string(),
    )
    .expect("manifest");

    let error = load_manifest(&config).expect_err("version mismatch should fail");

    assert!(matches!(
        error,
        AssetError::VersionMismatch {
            expected: ASSET_SYSTEM_VERSION,
            actual,
            ..
        } if actual == ASSET_SYSTEM_VERSION + 1
    ));
}
