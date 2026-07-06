use crate::asset::cook::*;
use crate::asset::types::{
    AssetConfig, AssetError, AssetMeta, AssetRegistryManifest, ASSET_SYSTEM_VERSION,
};
use std::path::Path;
use tempfile::tempdir;

fn write_png(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let image = image::RgbaImage::from_raw(1, 1, vec![255, 0, 0, 255]).unwrap();
    image.save(path)?;
    Ok(())
}

fn write_jpeg(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let image = image::RgbImage::from_raw(1, 1, vec![64, 128, 255]).unwrap();
    image.save(path)?;
    Ok(())
}

fn write_audio_placeholder(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::write(path, b"placeholder audio")?;
    Ok(())
}

fn cook_blob_registered(
    _config: &AssetConfig,
    source: &Path,
    _meta: &AssetMeta,
    cooked_path: &Path,
) -> Result<(), AssetError> {
    let bytes = std::fs::read(source).map_err(|error| AssetError::Io {
        path: source.to_path_buf(),
        message: error.to_string(),
    })?;
    std::fs::write(cooked_path, bytes).map_err(|error| AssetError::Io {
        path: cooked_path.to_path_buf(),
        message: error.to_string(),
    })
}

static BLOB_COOKER: CookerDescriptor = CookerDescriptor {
    asset_type: "blob",
    importer: "blob.raw",
    cooker: "blob.copy",
    version: 3,
    dependency_schema: None,
    source_extensions: &["blob"],
    cooked_dir: "blob",
    cooked_extension: "skyblob",
    cook: cook_blob_registered,
    update_dependencies: None,
    default_import_settings: None,
    normalize_import_settings: None,
};

static ALT_BLOB_COOKER: CookerDescriptor = CookerDescriptor {
    asset_type: "alt_blob",
    importer: "alt_blob.raw",
    cooker: "alt_blob.copy",
    version: 1,
    dependency_schema: None,
    source_extensions: &["blob"],
    cooked_dir: "alt_blob",
    cooked_extension: "skyaltblob",
    cook: cook_blob_registered,
    update_dependencies: None,
    default_import_settings: None,
    normalize_import_settings: None,
};

#[test]
fn import_is_reentrant_and_keeps_asset_id_stable() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let source = dir.path().join("hero.png");
    write_png(&source)?;

    let first = import_path(dir.path(), &source)?;
    let second = import_path(dir.path(), &source)?;

    assert_eq!(first.asset_id, second.asset_id);
    assert_eq!(first.source_path, second.source_path);
    Ok(())
}

#[test]
fn cook_all_writes_manifest_and_texture_output() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let source = dir.path().join("hero.png");
    write_png(&source)?;

    let config = AssetConfig::new(dir.path(), "native");
    let manifest = cook_all(&config)?;

    assert_eq!(manifest.assets.len(), 1);
    let cooked = config.cooked_root().join(&manifest.assets[0].cooked_path);
    assert!(cooked.exists());
    assert!(config.manifest_path().exists());
    Ok(())
}

#[test]
fn cook_all_writes_manifest_provenance() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let source = dir.path().join("hero.png");
    write_png(&source)?;

    let config = AssetConfig::new(dir.path(), "native").with_profile("editor");
    let manifest = cook_all(&config)?;

    assert_eq!(manifest.provenance.len(), 1);
    let entry = &manifest.assets[0];
    let provenance = &manifest.provenance[0];
    assert_eq!(provenance.asset_id, entry.asset_id);
    assert!(provenance.source_hash.is_some());
    assert!(provenance.cooked_hash.is_some());
    assert!(!provenance.dependency_hash.is_empty());
    assert_eq!(provenance.platform, "native");
    assert_eq!(provenance.profile, "editor");

    let saved = read_manifest(&config)?;
    assert_eq!(saved.provenance, manifest.provenance);
    Ok(())
}

#[test]
fn custom_cooker_registry_cooks_new_asset_kind_without_builtin_match(
) -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let source = dir.path().join("level.blob");
    std::fs::write(&source, b"custom blob")?;
    let config = AssetConfig::new(dir.path(), "native");
    let registry = CookRegistry::empty().with_registered(&BLOB_COOKER);

    let meta = import_path_with_registry(dir.path(), &source, &registry)?;
    assert_eq!(meta.asset_type, "blob");
    assert_eq!(meta.importer, "blob.raw");
    assert_eq!(meta.cooker, "blob.copy");
    assert_eq!(meta.version, 3);

    let manifest = cook_all_with_registry(&config, &registry)?;
    assert_eq!(manifest.assets.len(), 1);
    let entry = &manifest.assets[0];
    assert_eq!(entry.asset_type, "blob");
    assert_eq!(entry.version, 3);
    assert!(entry.cooked_path.starts_with("blob/"));
    assert!(entry.cooked_path.ends_with(".skyblob"));
    assert_eq!(
        std::fs::read(config.cooked_root().join(&entry.cooked_path))?,
        b"custom blob"
    );
    assert!(verify_with_registry(&config, &registry)?.is_clean());
    Ok(())
}

#[test]
fn import_settings_asset_type_selects_cooker_for_shared_extension(
) -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let source = dir.path().join("level.blob");
    std::fs::write(&source, b"alternate blob")?;
    let registry = CookRegistry::empty()
        .with_registered(&BLOB_COOKER)
        .with_registered(&ALT_BLOB_COOKER);

    let default_meta = import_path_with_registry(dir.path(), &source, &registry)?;
    assert_eq!(default_meta.asset_type, "blob");

    let meta_path = meta_path_for(&source);
    let mut selected = read_meta(&meta_path)?;
    selected.import_settings = serde_json::json!({ "asset_type": "alt_blob" });
    write_meta(&meta_path, &selected)?;

    let alt_meta = import_path_with_registry(dir.path(), &source, &registry)?;
    assert_eq!(alt_meta.asset_id, default_meta.asset_id);
    assert_eq!(alt_meta.asset_type, "alt_blob");
    assert_eq!(alt_meta.importer, "alt_blob.raw");
    assert_eq!(alt_meta.cooker, "alt_blob.copy");
    assert_eq!(
        alt_meta
            .import_settings
            .get("asset_type")
            .and_then(|value| value.as_str()),
        Some("alt_blob")
    );

    let config = AssetConfig::new(dir.path(), "native");
    let manifest = cook_all_with_registry(&config, &registry)?;
    assert_eq!(manifest.assets.len(), 1);
    let entry = &manifest.assets[0];
    assert_eq!(entry.asset_type, "alt_blob");
    assert_eq!(entry.importer, "alt_blob.raw");
    assert!(entry.cooked_path.starts_with("alt_blob/"));
    assert!(entry.cooked_path.ends_with(".skyaltblob"));
    assert!(verify_with_registry(&config, &registry)?.is_clean());
    Ok(())
}

#[test]
fn import_settings_asset_type_rejects_unsupported_cooker_for_source(
) -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let source = dir.path().join("level.blob");
    std::fs::write(&source, b"alternate blob")?;
    let registry = CookRegistry::empty().with_registered(&BLOB_COOKER);

    let _meta = import_path_with_registry(dir.path(), &source, &registry)?;
    let meta_path = meta_path_for(&source);
    let mut selected = read_meta(&meta_path)?;
    selected.import_settings = serde_json::json!({ "asset_type": "missing_blob" });
    write_meta(&meta_path, &selected)?;

    let error = import_path_with_registry(dir.path(), &source, &registry)
        .expect_err("unsupported explicit asset_type should fail");
    let AssetError::Unsupported { message } = error else {
        panic!("unexpected error");
    };
    assert!(message.contains("missing_blob"));
    Ok(())
}

#[test]
fn cook_all_writes_jpeg_texture_output() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let source = dir.path().join("hero.jpeg");
    write_jpeg(&source)?;

    let config = AssetConfig::new(dir.path(), "native");
    let manifest = cook_all(&config)?;

    assert_eq!(manifest.assets.len(), 1);
    assert_eq!(manifest.assets[0].asset_type, "texture");
    assert!(config
        .cooked_root()
        .join(&manifest.assets[0].cooked_path)
        .exists());
    Ok(())
}

#[test]
fn cook_all_writes_font_output() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let source = dir.path().join("title.ttf");
    std::fs::write(&source, b"fake-font")?;

    let config = AssetConfig::new(dir.path(), "native");
    let manifest = cook_all(&config)?;

    assert_eq!(manifest.assets.len(), 1);
    assert_eq!(manifest.assets[0].asset_type, "font");
    assert!(config
        .cooked_root()
        .join(&manifest.assets[0].cooked_path)
        .exists());
    Ok(())
}

#[test]
fn cook_all_writes_video_clip_with_texture_dependencies() -> Result<(), Box<dyn std::error::Error>>
{
    let dir = tempdir()?;
    let scene_dir = dir.path().join("scene");
    let frames_dir = scene_dir.join("frames");
    std::fs::create_dir_all(&frames_dir)?;
    write_png(&frames_dir.join("0001.png"))?;
    write_png(&frames_dir.join("0002.png"))?;
    let video = scene_dir.join("opening.skyvideo");
    std::fs::write(
        &video,
        r#"{
  "width": 1,
  "height": 1,
  "fps": 24,
  "frames": [
    "frames/0001.png",
    { "texture": "frames/0002.png", "duration_ms": 80 }
  ]
}"#,
    )?;

    let config = AssetConfig::new(dir.path(), "native");
    let manifest = cook_all(&config)?;
    let entry = manifest
        .assets
        .iter()
        .find(|entry| entry.asset_type == "video_clip")
        .expect("video clip should be in manifest");

    assert_eq!(entry.dependencies.len(), 2);
    assert!(entry.cooked_path.starts_with("video/"));
    let cooked = std::fs::read_to_string(config.cooked_root().join(&entry.cooked_path))?;
    let cooked: serde_json::Value = serde_json::from_str(&cooked)?;
    assert_eq!(cooked["width"], 1);
    assert_eq!(cooked["height"], 1);
    assert!(cooked["frames"][0]["texture"].as_str().is_some());
    assert_eq!(cooked["frames"][1]["duration_seconds"], 0.08);
    Ok(())
}

#[test]
fn verify_reports_missing_cooked_output() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let source = dir.path().join("hero.png");
    write_png(&source)?;
    let config = AssetConfig::new(dir.path(), "native");
    let meta = import_path(dir.path(), &source)?;

    let manifest = AssetRegistryManifest {
        version: ASSET_SYSTEM_VERSION,
        target: "native".to_string(),
        provenance: Vec::new(),
        assets: vec![build_manifest_entry(&meta, &CookRegistry::default())],
    };
    write_manifest(&config, &manifest)?;

    let result = verify(&config);
    assert!(matches!(result, Err(AssetError::VerificationFailed { .. })));
    Ok(())
}

#[test]
fn verify_reports_manifest_provenance_drift() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let source = dir.path().join("hero.png");
    write_png(&source)?;

    let config = AssetConfig::new(dir.path(), "native").with_profile("editor");
    let mut manifest = cook_all(&config)?;
    manifest.provenance[0].dependency_hash = "stale".to_string();
    write_manifest(&config, &manifest)?;

    let error = verify(&config).expect_err("stale provenance should fail verification");
    let AssetError::VerificationFailed { issues } = error else {
        panic!("unexpected error");
    };
    assert!(issues
        .iter()
        .any(|issue| issue.contains("manifest provenance mismatch")));
    Ok(())
}

#[test]
fn import_marks_music_tokens_as_streaming_audio() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let music_dir = dir.path().join("music");
    std::fs::create_dir_all(&music_dir)?;
    let source = music_dir.join("boss_theme.wav");
    write_audio_placeholder(&source)?;

    let meta = import_path(dir.path(), &source)?;

    assert_eq!(meta.asset_type, "music_track");
    assert!(meta.source_hash.is_some());
    assert!(meta.meta_hash.is_some());
    assert_eq!(
        meta.import_settings
            .get("asset_type")
            .and_then(|value| value.as_str()),
        Some("music_track")
    );
    assert_eq!(
        meta.import_settings
            .get("stream")
            .and_then(|value| value.as_bool()),
        Some(true)
    );
    Ok(())
}

#[test]
fn import_does_not_treat_partial_audio_names_as_music() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let source = dir.path().join("musicbox.wav");
    write_audio_placeholder(&source)?;

    let meta = import_path(dir.path(), &source)?;

    assert_eq!(meta.asset_type, "sound_clip");
    assert_eq!(
        meta.import_settings
            .get("asset_type")
            .and_then(|value| value.as_str()),
        Some("sound_clip")
    );
    assert_eq!(
        meta.import_settings
            .get("stream")
            .and_then(|value| value.as_bool()),
        Some(false)
    );
    Ok(())
}

#[test]
fn import_normalizes_audio_meta_when_stream_flag_changes() -> Result<(), Box<dyn std::error::Error>>
{
    let dir = tempdir()?;
    let source = dir.path().join("speech.wav");
    write_audio_placeholder(&source)?;

    let meta = import_path(dir.path(), &source)?;
    assert_eq!(meta.asset_type, "sound_clip");

    let meta_path = meta_path_for(&source);
    let mut updated = read_meta(&meta_path)?;
    updated.asset_type = "sound_clip".to_string();
    updated.import_settings = serde_json::json!({ "stream": true });
    write_meta(&meta_path, &updated)?;

    let normalized = import_path(dir.path(), &source)?;
    assert_eq!(normalized.asset_type, "music_track");
    assert_eq!(
        normalized
            .import_settings
            .get("asset_type")
            .and_then(|value| value.as_str()),
        Some("music_track")
    );
    assert_eq!(
        normalized
            .import_settings
            .get("stream")
            .and_then(|value| value.as_bool()),
        Some(true)
    );
    Ok(())
}

#[test]
fn import_audio_meta_prefers_explicit_asset_type_over_legacy_stream(
) -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let source = dir.path().join("speech.wav");
    write_audio_placeholder(&source)?;

    let meta = import_path(dir.path(), &source)?;
    let meta_path = meta_path_for(&source);
    let mut updated = read_meta(&meta_path)?;
    updated.import_settings = serde_json::json!({
        "asset_type": "music_track",
        "stream": false,
    });
    write_meta(&meta_path, &updated)?;

    let normalized = import_path(dir.path(), &source)?;
    assert_eq!(normalized.asset_type, "music_track");
    assert_eq!(
        normalized
            .import_settings
            .get("asset_type")
            .and_then(|value| value.as_str()),
        Some("music_track")
    );
    assert_eq!(
        normalized
            .import_settings
            .get("stream")
            .and_then(|value| value.as_bool()),
        Some(true)
    );
    assert_eq!(normalized.asset_id, meta.asset_id);
    Ok(())
}

#[test]
fn verify_reports_dependency_cycles() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let source_a = dir.path().join("a.wav");
    let source_b = dir.path().join("b.wav");
    write_audio_placeholder(&source_a)?;
    write_audio_placeholder(&source_b)?;

    let meta_a = import_path(dir.path(), &source_a)?;
    let meta_b = import_path(dir.path(), &source_b)?;

    let meta_a_path = meta_path_for(&source_a);
    let meta_b_path = meta_path_for(&source_b);

    let mut updated_a = read_meta(&meta_a_path)?;
    updated_a.dependencies = vec![meta_b.asset_id];
    write_meta(&meta_a_path, &updated_a)?;

    let mut updated_b = read_meta(&meta_b_path)?;
    updated_b.dependencies = vec![meta_a.asset_id];
    write_meta(&meta_b_path, &updated_b)?;

    let config = AssetConfig::new(dir.path(), "native");
    let manifest = AssetRegistryManifest {
        version: ASSET_SYSTEM_VERSION,
        target: "native".to_string(),
        provenance: Vec::new(),
        assets: vec![
            build_manifest_entry(&updated_a, &CookRegistry::default()),
            build_manifest_entry(&updated_b, &CookRegistry::default()),
        ],
    };
    write_manifest(&config, &manifest)?;
    let cooked_a = config
        .cooked_root()
        .join(cooked_relative_path(&updated_a, &CookRegistry::default()));
    let cooked_b = config
        .cooked_root()
        .join(cooked_relative_path(&updated_b, &CookRegistry::default()));
    std::fs::create_dir_all(
        cooked_a
            .parent()
            .expect("audio output should have a parent"),
    )?;
    std::fs::create_dir_all(
        cooked_b
            .parent()
            .expect("audio output should have a parent"),
    )?;
    std::fs::write(cooked_a, b"a")?;
    std::fs::write(cooked_b, b"b")?;

    let result = verify(&config);
    let Err(AssetError::VerificationFailed { issues }) = result else {
        panic!("expected verification failure");
    };
    assert!(
        issues
            .iter()
            .any(|issue| issue.contains("dependency cycle detected")),
        "issues: {issues:?}"
    );
    Ok(())
}

#[test]
fn verify_detects_tampered_cooked_artifact_even_when_newer(
) -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let source = dir.path().join("hero.png");
    write_png(&source)?;

    let config = AssetConfig::new(dir.path(), "native");
    let manifest = cook_all(&config)?;
    let cooked_path = config.cooked_root().join(&manifest.assets[0].cooked_path);
    std::fs::write(&cooked_path, b"tampered")?;

    let result = verify(&config);
    let Err(AssetError::VerificationFailed { issues }) = result else {
        panic!("expected verification failure");
    };
    assert!(
        issues
            .iter()
            .any(|issue| issue.contains("cooked artifact out of date")),
        "issues: {issues:?}"
    );
    Ok(())
}

#[test]
fn verify_reports_cooker_version_drift() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let source = dir.path().join("hero.png");
    write_png(&source)?;

    let config = AssetConfig::new(dir.path(), "native");
    let _manifest = cook_all(&config)?;
    let meta_path = meta_path_for(&source);
    let mut meta = read_meta(&meta_path)?;
    meta.version = 0;
    write_meta(&meta_path, &meta)?;

    let result = verify(&config);
    let Err(AssetError::VerificationFailed { issues }) = result else {
        panic!("expected verification failure");
    };
    assert!(
        issues
            .iter()
            .any(|issue| issue.contains("cooker version drift")),
        "issues: {issues:?}"
    );
    Ok(())
}
