use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::texture::{encode_texture_cooked, TextureAsset, TextureColorSpace};
use super::types::{
    normalize_source_key, AssetConfig, AssetError, AssetId, AssetManifestEntry, AssetMeta,
    AssetRegistryManifest, ASSET_SYSTEM_VERSION,
};

#[derive(Debug, Default)]
pub struct VerifyReport {
    pub issues: Vec<String>,
}

impl VerifyReport {
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.issues.is_empty()
    }
}

pub fn import_path(
    asset_root: impl AsRef<Path>,
    path: impl AsRef<Path>,
) -> Result<AssetMeta, AssetError> {
    let asset_root = asset_root.as_ref();
    let source = resolve_source_path(asset_root, path.as_ref())?;
    let source_key = source_key(asset_root, &source)?;
    let meta_path = meta_path_for(&source);

    let meta = if meta_path.exists() {
        let mut meta = read_meta(&meta_path)?;
        meta.source_path = source_key.clone();
        meta
    } else {
        default_meta_for_source(&source_key)
    };

    write_meta(&meta_path, &meta)?;
    Ok(meta)
}

pub fn cook_all(config: &AssetConfig) -> Result<AssetRegistryManifest, AssetError> {
    let source_files = collect_source_files(&config.asset_root)?;
    let mut manifest = AssetRegistryManifest {
        version: ASSET_SYSTEM_VERSION,
        target: config.target.clone(),
        assets: Vec::new(),
    };

    for source in source_files {
        let meta = import_path(&config.asset_root, &source)?;
        let entry = cook_meta(config, &source, &meta)?;
        manifest.assets.push(entry);
    }

    manifest
        .assets
        .sort_by(|a, b| a.source_path.cmp(&b.source_path));
    write_manifest(config, &manifest)?;
    Ok(manifest)
}

pub fn cook_target(config: &AssetConfig, query: &str) -> Result<AssetRegistryManifest, AssetError> {
    let source_files = collect_source_files(&config.asset_root)?;
    let mut selected = false;
    let mut manifest = AssetRegistryManifest {
        version: ASSET_SYSTEM_VERSION,
        target: config.target.clone(),
        assets: Vec::new(),
    };

    for source in source_files {
        let meta = import_path(&config.asset_root, &source)?;
        let matches = matches_query(query, &meta, &source, &config.asset_root)?;
        if matches {
            selected = true;
        }
        let entry = if matches {
            cook_meta(config, &source, &meta)?
        } else {
            ensure_cooked_entry(config, &source, &meta)?
        };
        manifest.assets.push(entry);
    }

    if !selected {
        return Err(AssetError::AssetPathNotFound {
            path: PathBuf::from(query),
        });
    }

    manifest
        .assets
        .sort_by(|a, b| a.source_path.cmp(&b.source_path));
    write_manifest(config, &manifest)?;
    Ok(manifest)
}

pub fn verify(config: &AssetConfig) -> Result<VerifyReport, AssetError> {
    let mut report = VerifyReport::default();
    let source_files = collect_source_files(&config.asset_root)?;
    let meta_files = collect_meta_files(&config.asset_root)?;

    let manifest = if config.manifest_path().exists() {
        Some(read_manifest(config)?)
    } else {
        report
            .issues
            .push(format!("manifest missing at {:?}", config.manifest_path()));
        None
    };

    for source in &source_files {
        let meta_path = meta_path_for(source);
        if !meta_path.exists() {
            report
                .issues
                .push(format!("missing meta for source {:?}", source));
        }
    }

    let mut known_ids = std::collections::HashSet::new();
    let mut metas = Vec::new();
    for meta_path in meta_files {
        match read_meta(&meta_path) {
            Ok(meta) => {
                known_ids.insert(meta.asset_id);
                metas.push((meta_path, meta));
            }
            Err(error) => report.issues.push(error.to_string()),
        }
    }

    for (meta_path, meta) in &metas {
        let source = config.asset_root.join(&meta.source_path);
        if !source.exists() {
            report.issues.push(format!(
                "meta {:?} points to missing source {:?}",
                meta_path, source
            ));
        }
        for dependency in &meta.dependencies {
            if !known_ids.contains(dependency) {
                report.issues.push(format!(
                    "meta {:?} has missing dependency {}",
                    meta_path, dependency
                ));
            }
        }

        let cooked_path = config.cooked_root().join(cooked_relative_path(meta));
        if !cooked_path.exists() {
            report.issues.push(format!(
                "cooked artifact missing for {} at {:?}",
                meta.asset_id, cooked_path
            ));
        } else if is_asset_dirty(&source, meta_path, &cooked_path)? {
            report.issues.push(format!(
                "cooked artifact out of date for {} at {:?}",
                meta.asset_id, cooked_path
            ));
        }

        if let Some(manifest) = &manifest {
            let Some(entry) = manifest
                .assets
                .iter()
                .find(|entry| entry.asset_id == meta.asset_id)
            else {
                report.issues.push(format!(
                    "manifest missing entry for asset {}",
                    meta.asset_id
                ));
                continue;
            };

            if entry.source_path != meta.source_path || entry.asset_type != meta.asset_type {
                report.issues.push(format!(
                    "manifest entry mismatch for asset {}",
                    meta.asset_id
                ));
            }
        }
    }

    if report.is_clean() {
        Ok(report)
    } else {
        Err(AssetError::VerificationFailed {
            issues: report.issues,
        })
    }
}

fn ensure_cooked_entry(
    config: &AssetConfig,
    source: &Path,
    meta: &AssetMeta,
) -> Result<AssetManifestEntry, AssetError> {
    let cooked_path = config.cooked_root().join(cooked_relative_path(meta));
    if !cooked_path.exists() || is_asset_dirty(source, &meta_path_for(source), &cooked_path)? {
        cook_meta(config, source, meta)
    } else {
        Ok(build_manifest_entry(meta))
    }
}

fn cook_meta(
    config: &AssetConfig,
    source: &Path,
    meta: &AssetMeta,
) -> Result<AssetManifestEntry, AssetError> {
    let cooked_relative = cooked_relative_path(meta);
    let cooked_path = config.cooked_root().join(&cooked_relative);
    let meta_path = meta_path_for(source);

    if !cooked_path.exists() || is_asset_dirty(source, &meta_path, &cooked_path)? {
        if let Some(parent) = cooked_path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| AssetError::Io {
                path: parent.to_path_buf(),
                message: error.to_string(),
            })?;
        }

        match asset_kind(meta)? {
            AssetKind::Texture => cook_texture(source, meta, &cooked_path)?,
            AssetKind::SoundClip | AssetKind::MusicTrack => cook_audio(source, &cooked_path)?,
        }
    }

    let mut entry = build_manifest_entry(meta);
    entry.cooked_path = normalize_source_key(&cooked_relative);
    Ok(entry)
}

fn cook_texture(source: &Path, meta: &AssetMeta, cooked_path: &Path) -> Result<(), AssetError> {
    let image = image::open(source)
        .map_err(|error| AssetError::Io {
            path: source.to_path_buf(),
            message: error.to_string(),
        })?
        .to_rgba8();
    let (width, height) = image.dimensions();
    let srgb = meta
        .import_settings
        .get("srgb")
        .and_then(|value| value.as_bool())
        .unwrap_or(true);
    let color_space = if srgb {
        TextureColorSpace::Srgb
    } else {
        TextureColorSpace::Linear
    };
    let asset = TextureAsset::new(width, height, color_space, image.into_raw());
    let bytes = encode_texture_cooked(&asset);
    std::fs::write(cooked_path, bytes).map_err(|error| AssetError::Io {
        path: cooked_path.to_path_buf(),
        message: error.to_string(),
    })
}

fn cook_audio(source: &Path, cooked_path: &Path) -> Result<(), AssetError> {
    std::fs::copy(source, cooked_path)
        .map_err(|error| AssetError::Io {
            path: cooked_path.to_path_buf(),
            message: error.to_string(),
        })
        .map(|_| ())
}

fn build_manifest_entry(meta: &AssetMeta) -> AssetManifestEntry {
    AssetManifestEntry {
        asset_id: meta.asset_id,
        asset_type: meta.asset_type.clone(),
        importer: meta.importer.clone(),
        cooker: meta.cooker.clone(),
        version: meta.version,
        source_path: meta.source_path.clone(),
        cooked_path: cooked_relative_path(meta),
        dependencies: meta.dependencies.clone(),
        import_settings: meta.import_settings.clone(),
    }
}

fn read_meta(path: &Path) -> Result<AssetMeta, AssetError> {
    let bytes = std::fs::read(path).map_err(|error| AssetError::Io {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;
    serde_json::from_slice(&bytes).map_err(|error| AssetError::Json {
        path: path.to_path_buf(),
        message: error.to_string(),
    })
}

fn write_meta(path: &Path, meta: &AssetMeta) -> Result<(), AssetError> {
    let bytes = serde_json::to_vec_pretty(meta).map_err(|error| AssetError::Json {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;
    std::fs::write(path, bytes).map_err(|error| AssetError::Io {
        path: path.to_path_buf(),
        message: error.to_string(),
    })
}

fn read_manifest(config: &AssetConfig) -> Result<AssetRegistryManifest, AssetError> {
    let path = config.manifest_path();
    let bytes = std::fs::read(&path).map_err(|error| AssetError::Io {
        path: path.clone(),
        message: error.to_string(),
    })?;
    serde_json::from_slice(&bytes).map_err(|error| AssetError::Json {
        path,
        message: error.to_string(),
    })
}

fn write_manifest(
    config: &AssetConfig,
    manifest: &AssetRegistryManifest,
) -> Result<(), AssetError> {
    std::fs::create_dir_all(config.cooked_root()).map_err(|error| AssetError::Io {
        path: config.cooked_root(),
        message: error.to_string(),
    })?;
    let bytes = serde_json::to_vec_pretty(manifest).map_err(|error| AssetError::Json {
        path: config.manifest_path(),
        message: error.to_string(),
    })?;
    std::fs::write(config.manifest_path(), bytes).map_err(|error| AssetError::Io {
        path: config.manifest_path(),
        message: error.to_string(),
    })
}

fn collect_source_files(root: &Path) -> Result<Vec<PathBuf>, AssetError> {
    let mut files = Vec::new();
    collect_recursive(root, &mut files, false)?;
    files.sort();
    Ok(files)
}

fn collect_meta_files(root: &Path) -> Result<Vec<PathBuf>, AssetError> {
    let mut files = Vec::new();
    collect_recursive(root, &mut files, true)?;
    files.sort();
    Ok(files)
}

fn collect_recursive(
    root: &Path,
    files: &mut Vec<PathBuf>,
    only_meta: bool,
) -> Result<(), AssetError> {
    if !root.exists() {
        return Ok(());
    }

    let entries = std::fs::read_dir(root).map_err(|error| AssetError::Io {
        path: root.to_path_buf(),
        message: error.to_string(),
    })?;

    for entry in entries {
        let entry = entry.map_err(|error| AssetError::Io {
            path: root.to_path_buf(),
            message: error.to_string(),
        })?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(|error| AssetError::Io {
            path: path.clone(),
            message: error.to_string(),
        })?;

        if file_type.is_dir() {
            if path.file_name() == Some(OsStr::new(".sky")) {
                continue;
            }
            collect_recursive(&path, files, only_meta)?;
            continue;
        }

        let is_meta = path
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.ends_with(".meta"))
            .unwrap_or(false);

        if only_meta {
            if is_meta {
                files.push(path);
            }
            continue;
        }

        if is_meta {
            continue;
        }

        if source_asset_kind(&path).is_some() {
            files.push(path);
        }
    }

    Ok(())
}

fn resolve_source_path(asset_root: &Path, path: &Path) -> Result<PathBuf, AssetError> {
    let candidate = if path.is_absolute() {
        path.to_path_buf()
    } else {
        asset_root.join(path)
    };
    if !candidate.exists() {
        return Err(AssetError::AssetPathNotFound { path: candidate });
    }
    Ok(candidate)
}

fn source_key(asset_root: &Path, source: &Path) -> Result<String, AssetError> {
    let relative = source
        .strip_prefix(asset_root)
        .map_err(|_| AssetError::InvalidConfig {
            message: format!(
                "source {:?} is not inside asset root {:?}",
                source, asset_root
            ),
        })?;
    Ok(normalize_source_key(&relative.to_string_lossy()))
}

fn meta_path_for(source: &Path) -> PathBuf {
    let file_name = source
        .file_name()
        .expect("source files should have file names")
        .to_string_lossy();
    source.with_file_name(format!("{file_name}.meta"))
}

fn default_meta_for_source(source_key: &str) -> AssetMeta {
    match asset_kind_from_source_key(source_key) {
        AssetKind::Texture => AssetMeta {
            asset_id: AssetId::new(),
            asset_type: "texture".to_string(),
            importer: "texture.image".to_string(),
            cooker: "texture.rgba8".to_string(),
            version: 1,
            source_path: source_key.to_string(),
            dependencies: Vec::new(),
            import_settings: serde_json::json!({ "srgb": true }),
        },
        AssetKind::SoundClip => AssetMeta {
            asset_id: AssetId::new(),
            asset_type: "sound_clip".to_string(),
            importer: "audio.symphonia".to_string(),
            cooker: "audio.copy".to_string(),
            version: 1,
            source_path: source_key.to_string(),
            dependencies: Vec::new(),
            import_settings: serde_json::json!({ "stream": false }),
        },
        AssetKind::MusicTrack => AssetMeta {
            asset_id: AssetId::new(),
            asset_type: "music_track".to_string(),
            importer: "audio.symphonia".to_string(),
            cooker: "audio.copy".to_string(),
            version: 1,
            source_path: source_key.to_string(),
            dependencies: Vec::new(),
            import_settings: serde_json::json!({ "stream": true }),
        },
    }
}

fn source_asset_kind(path: &Path) -> Option<AssetKind> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    match extension.as_str() {
        "png" => Some(AssetKind::Texture),
        "wav" | "ogg" | "mp3" => Some(AssetKind::SoundClip),
        _ => None,
    }
}

fn asset_kind_from_source_key(source_key: &str) -> AssetKind {
    let lower = source_key.to_ascii_lowercase();
    if lower.ends_with(".png") {
        AssetKind::Texture
    } else if lower.contains("/music/")
        || lower.contains("/bgm/")
        || lower.contains("music")
        || lower.contains("bgm")
    {
        AssetKind::MusicTrack
    } else {
        AssetKind::SoundClip
    }
}

fn asset_kind(meta: &AssetMeta) -> Result<AssetKind, AssetError> {
    match meta.asset_type.as_str() {
        "texture" => Ok(AssetKind::Texture),
        "sound_clip" => Ok(AssetKind::SoundClip),
        "music_track" => Ok(AssetKind::MusicTrack),
        other => Err(AssetError::Unsupported {
            message: format!("unsupported asset type `{other}`"),
        }),
    }
}

fn cooked_relative_path(meta: &AssetMeta) -> String {
    match meta.asset_type.as_str() {
        "texture" => format!("texture/{}.skytx", meta.asset_id),
        "sound_clip" | "music_track" => format!("audio/{}.skyaudio", meta.asset_id),
        _ => format!("misc/{}.skyasset", meta.asset_id),
    }
}

fn matches_query(
    query: &str,
    meta: &AssetMeta,
    source: &Path,
    asset_root: &Path,
) -> Result<bool, AssetError> {
    if let Ok(asset_id) = AssetId::parse_str(query) {
        return Ok(meta.asset_id == asset_id);
    }

    let query_path = resolve_query_path(asset_root, query);
    let normalized_query = if query_path.is_absolute() {
        source_key(asset_root, &query_path)?
    } else {
        normalize_source_key(query)
    };
    let normalized_source = normalize_source_key(&source_key(asset_root, source)?);
    Ok(normalized_query == normalized_source)
}

fn resolve_query_path(asset_root: &Path, query: &str) -> PathBuf {
    let path = Path::new(query);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        asset_root.join(path)
    }
}

fn is_asset_dirty(source: &Path, meta_path: &Path, cooked_path: &Path) -> Result<bool, AssetError> {
    if !cooked_path.exists() {
        return Ok(true);
    }
    let source_time = modified_time(source)?;
    let meta_time = modified_time(meta_path)?;
    let cooked_time = modified_time(cooked_path)?;
    Ok(cooked_time < source_time || cooked_time < meta_time)
}

fn modified_time(path: &Path) -> Result<SystemTime, AssetError> {
    std::fs::metadata(path)
        .map_err(|error| AssetError::Io {
            path: path.to_path_buf(),
            message: error.to_string(),
        })?
        .modified()
        .map_err(|error| AssetError::Io {
            path: path.to_path_buf(),
            message: error.to_string(),
        })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AssetKind {
    Texture,
    SoundClip,
    MusicTrack,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn write_png(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        let image = image::RgbaImage::from_raw(1, 1, vec![255, 0, 0, 255]).unwrap();
        image.save(path)?;
        Ok(())
    }

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
    fn verify_reports_missing_cooked_output() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        let source = dir.path().join("hero.png");
        write_png(&source)?;
        let config = AssetConfig::new(dir.path(), "native");
        let meta = import_path(dir.path(), &source)?;

        let manifest = AssetRegistryManifest {
            version: ASSET_SYSTEM_VERSION,
            target: "native".to_string(),
            assets: vec![build_manifest_entry(&meta)],
        };
        write_manifest(&config, &manifest)?;

        let result = verify(&config);
        assert!(matches!(result, Err(AssetError::VerificationFailed { .. })));
        Ok(())
    }
}
