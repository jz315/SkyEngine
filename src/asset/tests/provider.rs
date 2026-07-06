use crate::asset::provider::*;
use crate::asset::registry::LocalManifestRegistry;
use crate::asset::store::{AssetRecord, AssetStore};
use crate::asset::types::AssetError;
use crate::asset::{
    Asset, AssetConfig, AssetId, AssetManifestEntry, AssetRegistryManifest, FontAsset,
    TextureAsset, ASSET_SYSTEM_VERSION,
};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tempfile::tempdir;

fn entry(id: AssetId) -> AssetManifestEntry {
    AssetManifestEntry {
        asset_id: id,
        asset_type: "dummy".to_string(),
        importer: "dummy".to_string(),
        cooker: "dummy".to_string(),
        version: ASSET_SYSTEM_VERSION,
        source_path: "clip.dummy".to_string(),
        cooked_path: "clip.dummyc".to_string(),
        dependencies: Vec::new(),
        import_settings: serde_json::Value::Null,
    }
}

fn entry_with_cooked(id: AssetId, cooked_path: &str) -> AssetManifestEntry {
    AssetManifestEntry {
        cooked_path: cooked_path.to_string(),
        ..entry(id)
    }
}

#[test]
fn local_provider_resolves_cooked_paths_under_cooked_root() {
    let dir = tempdir().expect("temporary asset root");
    let config = AssetConfig::new(dir.path(), "native");
    let provider = LocalAssetProvider::new(&config);
    let id = AssetId::new();

    let source = provider
        .resolve(id, entry(id), None)
        .expect("source should resolve");

    assert_eq!(
        source.location(),
        &AssetSourceLocation::Cooked(config.cooked_root().join("clip.dummyc"))
    );
}

#[test]
fn local_provider_falls_back_to_package_roots_for_cooked_artifacts() {
    let dir = tempdir().expect("temporary asset root");
    let package = dir.path().join("packages/base");
    std::fs::create_dir_all(&package).expect("package root");
    std::fs::write(package.join("clip.dummyc"), b"ready").expect("package cooked bytes");
    let config = AssetConfig::new(dir.path(), "native").with_package_root("packages/base");
    let provider = LocalAssetProvider::new(&config);
    let id = AssetId::new();

    let source = provider
        .resolve(id, entry(id), None)
        .expect("source should resolve");

    assert_eq!(
        source.location(),
        &AssetSourceLocation::Package(package.join("clip.dummyc"))
    );
    assert_eq!(source.read_bytes(id).expect("package bytes"), b"ready");
}

#[test]
fn local_provider_falls_back_to_package_files_for_cooked_artifacts() {
    let dir = tempdir().expect("temporary asset root");
    let config = AssetConfig::new(dir.path(), "native").with_package_file("base.skybundle");
    let bundle = config.asset_root.join("base.skybundle");
    write_test_bundle(&bundle, &[("clip.dummyc", b"from-bundle")]).expect("bundle file");
    let provider = LocalAssetProvider::new(&config);
    let id = AssetId::new();

    let source = provider
        .resolve(id, entry(id), None)
        .expect("source should resolve");

    assert!(matches!(
        source.location(),
        AssetSourceLocation::Bundle {
            bundle_path,
            cooked_path,
            ..
        } if bundle_path == &bundle && cooked_path == "clip.dummyc"
    ));
    assert_eq!(source.read_bytes(id).expect("bundle bytes"), b"from-bundle");
}

#[test]
fn local_provider_stats_report_package_mounts_and_bundle_index_cache() {
    let dir = tempdir().expect("temporary asset root");
    let config = AssetConfig::new(dir.path(), "native")
        .with_package_root("packages/base")
        .with_package_file("base.skybundle");
    write_test_bundle(
        &config.asset_root.join("base.skybundle"),
        &[("clip.dummyc", b"from-bundle")],
    )
    .expect("bundle file");
    let provider = LocalAssetProvider::new(&config);
    let id = AssetId::new();

    let initial = provider.stats();
    assert_eq!(initial.package_roots, 1);
    assert_eq!(initial.package_files, 1);
    assert_eq!(initial.cached_bundle_indexes, 0);
    assert_eq!(initial.cached_bundle_index_entries, 0);
    assert_eq!(initial.resolved_bundle_sources, 0);
    assert_eq!(initial.cache_invalidations, 0);
    assert_eq!(initial.full_cache_invalidations, 0);

    let source = provider
        .resolve(id, entry(id), None)
        .expect("bundle source should resolve");
    assert!(matches!(
        source.location(),
        AssetSourceLocation::Bundle { .. }
    ));

    let after_resolve = provider.stats();
    assert_eq!(after_resolve.package_roots, 1);
    assert_eq!(after_resolve.package_files, 1);
    assert_eq!(after_resolve.cached_bundle_indexes, 1);
    assert_eq!(after_resolve.cached_bundle_index_entries, 1);
    assert_eq!(after_resolve.resolved_bundle_sources, 1);
    assert_eq!(after_resolve.resolved_cooked_sources, 0);
    assert_eq!(after_resolve.resolve_errors, 0);
    assert_eq!(after_resolve.cache_invalidations, 0);
    assert_eq!(after_resolve.full_cache_invalidations, 0);
}

#[test]
fn local_provider_stats_count_resolved_source_locations() {
    let dir = tempdir().expect("temporary asset root");
    let config = AssetConfig::new(dir.path(), "native")
        .with_package_root("packages/base")
        .with_package_file("base.skybundle");
    let package = config.asset_root.join("packages/base");
    std::fs::create_dir_all(&package).expect("package root");
    std::fs::create_dir_all(config.cooked_root()).expect("local cooked root");
    std::fs::write(config.cooked_root().join("local.dummyc"), b"local")
        .expect("local cooked bytes");
    std::fs::write(package.join("package.dummyc"), b"package").expect("package cooked bytes");
    write_test_bundle(
        &config.asset_root.join("base.skybundle"),
        &[("bundle.dummyc", b"bundle")],
    )
    .expect("bundle file");
    let provider = LocalAssetProvider::new(&config);
    let raw_id = AssetId::new();
    let cooked_id = AssetId::new();
    let package_id = AssetId::new();
    let bundle_id = AssetId::new();
    let missing_id = AssetId::new();

    let _ = provider
        .resolve(raw_id, entry(raw_id), Some(dir.path().join("raw.dummy")))
        .expect("raw source");
    let _ = provider
        .resolve(
            cooked_id,
            entry_with_cooked(cooked_id, "local.dummyc"),
            None,
        )
        .expect("local cooked source");
    let _ = provider
        .resolve(
            package_id,
            entry_with_cooked(package_id, "package.dummyc"),
            None,
        )
        .expect("package root source");
    let _ = provider
        .resolve(
            bundle_id,
            entry_with_cooked(bundle_id, "bundle.dummyc"),
            None,
        )
        .expect("bundle source");
    let _ = provider
        .resolve(
            missing_id,
            entry_with_cooked(missing_id, "missing.dummyc"),
            None,
        )
        .expect("missing cooked source still resolves to local cooked path");

    let stats = provider.stats();
    assert_eq!(stats.resolved_raw_sources, 1);
    assert_eq!(stats.resolved_cooked_sources, 2);
    assert_eq!(stats.resolved_package_sources, 1);
    assert_eq!(stats.resolved_bundle_sources, 1);
    assert_eq!(stats.resolve_errors, 0);
}

#[test]
fn local_provider_stats_count_resolve_errors() {
    let dir = tempdir().expect("temporary asset root");
    let config = AssetConfig::new(dir.path(), "native").with_package_file("broken.skybundle");
    std::fs::write(config.asset_root.join("broken.skybundle"), b"not-a-bundle")
        .expect("broken bundle bytes");
    let provider = LocalAssetProvider::new(&config);
    let id = AssetId::new();

    let error = provider
        .resolve(id, entry(id), None)
        .expect_err("invalid bundle should fail during source resolution");
    assert!(matches!(error, AssetError::InvalidCookedAsset { .. }));

    let stats = provider.stats();
    assert_eq!(stats.resolve_errors, 1);
    assert_eq!(stats.resolved_bundle_sources, 0);
    assert_eq!(stats.resolved_cooked_sources, 0);
}

#[test]
fn local_provider_prefers_package_root_over_package_file() {
    let dir = tempdir().expect("temporary asset root");
    let config = AssetConfig::new(dir.path(), "native")
        .with_package_root("packages/base")
        .with_package_file("base.skybundle");
    let package = config.asset_root.join("packages/base");
    std::fs::create_dir_all(&package).expect("package root");
    std::fs::write(package.join("clip.dummyc"), b"from-root").expect("package cooked bytes");
    write_test_bundle(
        &config.asset_root.join("base.skybundle"),
        &[("clip.dummyc", b"from-bundle")],
    )
    .expect("bundle file");
    let provider = LocalAssetProvider::new(&config);
    let id = AssetId::new();

    let source = provider
        .resolve(id, entry(id), None)
        .expect("source should resolve");

    assert_eq!(
        source.location(),
        &AssetSourceLocation::Package(package.join("clip.dummyc"))
    );
    assert_eq!(source.read_bytes(id).expect("package bytes"), b"from-root");
}

#[test]
fn local_provider_prefers_loose_cooked_over_package_root() {
    let dir = tempdir().expect("temporary asset root");
    let config = AssetConfig::new(dir.path(), "native").with_package_root("packages/base");
    let package = config.asset_root.join("packages/base");
    std::fs::create_dir_all(&package).expect("package root");
    std::fs::create_dir_all(config.cooked_root()).expect("local cooked root");
    std::fs::write(package.join("clip.dummyc"), b"package").expect("package cooked bytes");
    std::fs::write(config.cooked_root().join("clip.dummyc"), b"local").expect("local cooked bytes");
    let provider = LocalAssetProvider::new(&config);
    let id = AssetId::new();

    let source = provider
        .resolve(id, entry(id), None)
        .expect("source should resolve");

    assert_eq!(
        source.location(),
        &AssetSourceLocation::Cooked(config.cooked_root().join("clip.dummyc"))
    );
    assert_eq!(source.read_bytes(id).expect("local bytes"), b"local");
}

#[test]
fn local_provider_prefers_raw_source_path() {
    let dir = tempdir().expect("temporary asset root");
    let config = AssetConfig::new(dir.path(), "native");
    let provider = LocalAssetProvider::new(&config);
    let id = AssetId::new();
    let raw = dir.path().join("clip.dummy");

    let source = provider
        .resolve(id, entry(id), Some(raw.clone()))
        .expect("source should resolve");

    assert_eq!(source.location(), &AssetSourceLocation::Raw(raw));
}

#[test]
fn memory_provider_returns_in_memory_bytes() {
    let id = AssetId::new();
    let provider = MemoryAssetProvider::new().with_asset(id, b"ready".to_vec());

    let source = provider
        .resolve(id, entry(id), None)
        .expect("memory source should resolve");

    assert_eq!(source.read_bytes(id).expect("memory bytes"), b"ready");
    assert!(matches!(
        source.location(),
        AssetSourceLocation::Memory { label, .. } if label == &format!("memory://{id}")
    ));
}

#[test]
fn memory_provider_can_model_read_and_resolve_failures() {
    let read_id = AssetId::new();
    let resolve_id = AssetId::new();
    let read_error = AssetError::Io {
        path: PathBuf::from(format!("memory://{read_id}")),
        message: "read failed".to_string(),
    };
    let resolve_error = AssetError::Unsupported {
        message: "resolve failed".to_string(),
    };
    let provider = MemoryAssetProvider::new()
        .with_read_error(read_id, read_error.clone())
        .with_resolve_error(resolve_id, resolve_error.clone());

    let source = provider
        .resolve(read_id, entry(read_id), None)
        .expect("read-failing memory source should still resolve");
    assert_eq!(source.read_bytes(read_id).unwrap_err(), read_error);
    assert_eq!(
        provider
            .resolve(resolve_id, entry(resolve_id), None)
            .unwrap_err(),
        resolve_error
    );
}

#[test]
fn memory_provider_can_model_delayed_reads() {
    let id = AssetId::new();
    let delay = Duration::from_millis(15);
    let provider = MemoryAssetProvider::new().with_delayed_asset(id, b"slow".to_vec(), delay);

    let source = provider
        .resolve(id, entry(id), None)
        .expect("delayed memory source should resolve");
    let started = std::time::Instant::now();

    assert_eq!(
        source.read_bytes(id).expect("delayed memory bytes"),
        b"slow"
    );
    assert!(started.elapsed() >= delay);
}

#[test]
fn raw_source_request_normalizes_path_and_key() {
    let dir = tempdir().expect("temporary asset root");
    let config = AssetConfig::new(dir.path(), "native");

    let request = RawSourceRequest::new(&config, Path::new("textures/hero.png"));

    assert_eq!(request.path, dir.path().join("textures/hero.png"));
    assert_eq!(request.key, config.source_key(&request.path));
}

#[test]
fn record_manifest_entry_prefers_manifest_then_falls_back_to_raw_texture_and_font() {
    let dir = tempdir().expect("temporary asset root");
    let config = AssetConfig::new(dir.path(), "native");
    let manifest_id = AssetId::new();
    let texture_id = AssetId::new();
    let font_id = AssetId::new();
    let manifest_entry = entry(manifest_id);
    let manifest = LocalManifestRegistry::new(AssetRegistryManifest {
        version: ASSET_SYSTEM_VERSION,
        target: "native".to_string(),
        provenance: Vec::new(),
        assets: vec![manifest_entry.clone()],
    });
    let mut store = AssetStore::default();
    let texture_path = config.asset_root.join("textures/hero.png");
    let font_path = config.asset_root.join("fonts/ui.ttf");
    store
        .records
        .insert(texture_id, AssetRecord::new_raw_texture(texture_path));
    store
        .records
        .insert(font_id, AssetRecord::new_raw_font(font_path));

    let manifest_result =
        record_manifest_entry(&config, &manifest, &store, manifest_id).expect("manifest entry");
    let texture_result =
        record_manifest_entry(&config, &manifest, &store, texture_id).expect("texture entry");
    let font_result =
        record_manifest_entry(&config, &manifest, &store, font_id).expect("font entry");

    assert_eq!(manifest_result.asset_id, manifest_entry.asset_id);
    assert_eq!(manifest_result.asset_type, manifest_entry.asset_type);
    assert_eq!(manifest_result.cooked_path, manifest_entry.cooked_path);
    assert_eq!(texture_result.asset_type, TextureAsset::TYPE);
    assert_eq!(texture_result.source_path, "textures/hero.png");
    assert_eq!(
        texture_result.import_settings,
        serde_json::json!({ "srgb": true })
    );
    assert_eq!(font_result.asset_type, FontAsset::TYPE);
    assert_eq!(font_result.source_path, "fonts/ui.ttf");
}

#[test]
fn resolve_record_source_uses_record_entry_and_raw_source_path() {
    let dir = tempdir().expect("temporary asset root");
    let config = AssetConfig::new(dir.path(), "native");
    let provider = LocalAssetProvider::new(&config);
    let manifest = LocalManifestRegistry::new(AssetRegistryManifest {
        version: ASSET_SYSTEM_VERSION,
        target: "native".to_string(),
        provenance: Vec::new(),
        assets: Vec::new(),
    });
    let id = AssetId::new();
    let raw_path = config.asset_root.join("textures/hero.png");
    let mut store = AssetStore::default();
    store
        .records
        .insert(id, AssetRecord::new_raw_texture(raw_path.clone()));

    let source = resolve_record_source(&config, &provider, &manifest, &store, id)
        .expect("raw source should resolve");

    assert_eq!(source.location(), &AssetSourceLocation::Raw(raw_path));
    assert_eq!(source.entry().asset_type, TextureAsset::TYPE);
    assert_eq!(source.entry().source_path, "textures/hero.png");
}
