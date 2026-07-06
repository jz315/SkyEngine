use crate::asset::provider::*;
use crate::asset::watcher::AssetWatchEvent;
use crate::asset::{AssetConfig, AssetId, AssetManifestEntry, ASSET_SYSTEM_VERSION};
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

#[test]
fn local_provider_invalidates_cached_bundle_index_for_changed_bundle_path() {
    let dir = tempdir().expect("temporary asset root");
    let config = AssetConfig::new(dir.path(), "native").with_package_file("base.skybundle");
    let bundle = config.asset_root.join("base.skybundle");
    write_test_bundle(&bundle, &[("clip.dummyc", b"from-bundle")]).expect("bundle file");
    let provider = LocalAssetProvider::new(&config);
    let id = AssetId::new();

    let source = provider
        .resolve(id, entry(id), None)
        .expect("bundle source should resolve");
    assert!(matches!(
        source.location(),
        AssetSourceLocation::Bundle { .. }
    ));
    assert_eq!(provider.cached_bundle_count(), 1);

    write_test_bundle(&bundle, &[("other.dummyc", b"other")]).expect("rewritten bundle file");
    provider.invalidate_changed_paths(std::slice::from_ref(&bundle));

    assert_eq!(provider.cached_bundle_count(), 0);
    let source = provider
        .resolve(id, entry(id), None)
        .expect("missing bundle entry should fall through to cooked path");
    assert_eq!(
        source.location(),
        &AssetSourceLocation::Cooked(config.cooked_root().join("clip.dummyc"))
    );
    assert_eq!(provider.cached_bundle_count(), 1);
}

#[test]
fn provider_invalidation_from_watch_events_routes_changed_paths_and_rescans() {
    let dir = tempdir().expect("temporary asset root");
    let config = AssetConfig::new(dir.path(), "native").with_package_file("base.skybundle");
    let bundle = config.asset_root.join("base.skybundle");
    write_test_bundle(&bundle, &[("clip.dummyc", b"from-bundle")]).expect("bundle file");
    let provider = LocalAssetProvider::new(&config);
    let id = AssetId::new();

    provider
        .resolve(id, entry(id), None)
        .expect("bundle source should resolve");
    assert_eq!(provider.cached_bundle_count(), 1);

    invalidate_from_watch_events(&provider, &[AssetWatchEvent::Changed(bundle.clone())]);
    assert_eq!(provider.cached_bundle_count(), 0);
    let stats = provider.stats();
    assert_eq!(stats.cache_invalidations, 1);
    assert_eq!(stats.full_cache_invalidations, 0);

    provider
        .resolve(id, entry(id), None)
        .expect("bundle source should resolve again");
    assert_eq!(provider.cached_bundle_count(), 1);

    invalidate_from_watch_events(&provider, &[AssetWatchEvent::Rescan]);
    assert_eq!(provider.cached_bundle_count(), 0);
    let stats = provider.stats();
    assert_eq!(stats.cache_invalidations, 2);
    assert_eq!(stats.full_cache_invalidations, 1);
}
