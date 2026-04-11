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

    let mut meta = if meta_path.exists() {
        let mut meta = read_meta(&meta_path)?;
        meta.source_path = source_key.clone();
        meta
    } else {
        default_meta_for_source(&source_key)
    };
    normalize_meta_for_source(&source_key, &mut meta);
    update_source_and_meta_hashes(&source, &mut meta)?;

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
    let mut seen_meta_ids = std::collections::HashMap::new();
    let mut metas = Vec::new();
    for meta_path in meta_files {
        match read_meta(&meta_path) {
            Ok(meta) => {
                known_ids.insert(meta.asset_id);
                if let Some(previous) = seen_meta_ids.insert(meta.asset_id, meta_path.clone()) {
                    report.issues.push(format!(
                        "duplicate asset id {} in {:?} and {:?}",
                        meta.asset_id, previous, meta_path
                    ));
                }
                metas.push((meta_path, meta));
            }
            Err(error) => report.issues.push(error.to_string()),
        }
    }

    for (meta_path, meta) in &metas {
        let source = config.asset_root.join(&meta.source_path);
        let source_exists = source.exists();
        if !source_exists {
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
        } else if source_exists && is_asset_dirty(&source, meta_path, &cooked_path, meta)? {
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

            let expected = build_manifest_entry(meta);
            if entry.asset_type != expected.asset_type
                || entry.importer != expected.importer
                || entry.cooker != expected.cooker
                || entry.version != expected.version
                || entry.source_path != expected.source_path
                || entry.cooked_path != expected.cooked_path
                || entry.dependencies != expected.dependencies
                || entry.import_settings != expected.import_settings
            {
                report.issues.push(format!(
                    "manifest entry mismatch for asset {}",
                    meta.asset_id
                ));
            }
        }
    }

    for cycle in dependency_cycles(&metas) {
        report.issues.push(format!(
            "dependency cycle detected: {}",
            cycle
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(" -> ")
        ));
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
    if !cooked_path.exists() || is_asset_dirty(source, &meta_path_for(source), &cooked_path, meta)?
    {
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
    let mut updated_meta = meta.clone();

    if !cooked_path.exists() || is_asset_dirty(source, &meta_path, &cooked_path, meta)? {
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

    update_source_and_meta_hashes(source, &mut updated_meta)?;
    updated_meta.cooked_hash = Some(file_hash(&cooked_path)?);
    write_meta(&meta_path, &updated_meta)?;

    let mut entry = build_manifest_entry(&updated_meta);
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

fn import_settings_object_mut(
    import_settings: &mut serde_json::Value,
) -> &mut serde_json::Map<String, serde_json::Value> {
    if !import_settings.is_object() {
        *import_settings = serde_json::Value::Object(serde_json::Map::new());
    }
    import_settings
        .as_object_mut()
        .expect("import_settings should be an object after normalization")
}

fn set_import_setting_bool(import_settings: &mut serde_json::Value, key: &str, value: bool) {
    import_settings_object_mut(import_settings)
        .insert(key.to_string(), serde_json::Value::Bool(value));
}

fn infer_default_audio_stream(source_key: &str) -> bool {
    let lower = normalize_source_key(source_key);
    Path::new(&lower).components().any(|component| {
        let std::path::Component::Normal(part) = component else {
            return false;
        };
        let Some(part) = part.to_str() else {
            return false;
        };
        part.split(|ch: char| !ch.is_ascii_alphanumeric())
            .filter(|token| !token.is_empty())
            .any(|token| matches!(token, "music" | "bgm" | "stream" | "streaming"))
    })
}

fn normalize_meta_for_source(source_key: &str, meta: &mut AssetMeta) {
    meta.source_path = source_key.to_string();

    match source_asset_kind(Path::new(source_key)) {
        Some(AssetKind::Texture) => {
            meta.asset_type = "texture".to_string();
            meta.importer = "texture.image".to_string();
            meta.cooker = "texture.rgba8".to_string();
            let srgb = meta
                .import_settings
                .get("srgb")
                .and_then(|value| value.as_bool())
                .unwrap_or(true);
            set_import_setting_bool(&mut meta.import_settings, "srgb", srgb);
        }
        Some(AssetKind::SoundClip) => {
            let stream = meta
                .import_settings
                .get("stream")
                .and_then(|value| value.as_bool())
                .unwrap_or_else(|| infer_default_audio_stream(source_key));
            meta.asset_type = if stream {
                "music_track".to_string()
            } else {
                "sound_clip".to_string()
            };
            meta.importer = "audio.symphonia".to_string();
            meta.cooker = "audio.copy".to_string();
            set_import_setting_bool(&mut meta.import_settings, "stream", stream);
        }
        Some(AssetKind::MusicTrack) | None => {}
    }
}

fn update_source_and_meta_hashes(source: &Path, meta: &mut AssetMeta) -> Result<(), AssetError> {
    let previous_source_hash = meta.source_hash.clone();
    let previous_meta_hash = meta.meta_hash.clone();

    let source_hash = file_hash(source)?;
    meta.source_hash = Some(source_hash.clone());
    let meta_hash = meta_fingerprint_hash(meta)?;
    meta.meta_hash = Some(meta_hash.clone());

    if previous_source_hash.as_deref() != Some(source_hash.as_str())
        || previous_meta_hash.as_deref() != Some(meta_hash.as_str())
    {
        meta.cooked_hash = None;
    }

    Ok(())
}

fn default_meta_for_source(source_key: &str) -> AssetMeta {
    match source_asset_kind(Path::new(source_key))
        .expect("default_meta_for_source should only run for supported source files")
    {
        AssetKind::Texture => AssetMeta {
            asset_id: AssetId::new(),
            asset_type: "texture".to_string(),
            importer: "texture.image".to_string(),
            cooker: "texture.rgba8".to_string(),
            version: 1,
            source_path: source_key.to_string(),
            source_hash: None,
            meta_hash: None,
            cooked_hash: None,
            dependencies: Vec::new(),
            import_settings: serde_json::json!({ "srgb": true }),
        },
        AssetKind::SoundClip => {
            let stream = infer_default_audio_stream(source_key);
            AssetMeta {
                asset_id: AssetId::new(),
                asset_type: if stream {
                    "music_track".to_string()
                } else {
                    "sound_clip".to_string()
                },
                importer: "audio.symphonia".to_string(),
                cooker: "audio.copy".to_string(),
                version: 1,
                source_path: source_key.to_string(),
                source_hash: None,
                meta_hash: None,
                cooked_hash: None,
                dependencies: Vec::new(),
                import_settings: serde_json::json!({ "stream": stream }),
            }
        }
        AssetKind::MusicTrack => unreachable!("source files never map directly to music_track"),
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

fn is_asset_dirty(
    source: &Path,
    meta_path: &Path,
    cooked_path: &Path,
    meta: &AssetMeta,
) -> Result<bool, AssetError> {
    if !cooked_path.exists() {
        return Ok(true);
    }
    let Some(source_hash) = &meta.source_hash else {
        return Ok(true);
    };
    let Some(meta_hash) = &meta.meta_hash else {
        return Ok(true);
    };
    let Some(cooked_hash) = &meta.cooked_hash else {
        return Ok(true);
    };

    if &file_hash(source)? != source_hash {
        return Ok(true);
    }
    if &meta_fingerprint_hash(meta)? != meta_hash {
        return Ok(true);
    }
    if &file_hash(cooked_path)? != cooked_hash {
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

fn file_hash(path: &Path) -> Result<String, AssetError> {
    let bytes = std::fs::read(path).map_err(|error| AssetError::Io {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;
    Ok(hash_bytes(&bytes))
}

fn meta_fingerprint_hash(meta: &AssetMeta) -> Result<String, AssetError> {
    let fingerprint = serde_json::json!({
        "asset_id": meta.asset_id.to_string(),
        "asset_type": meta.asset_type,
        "importer": meta.importer,
        "cooker": meta.cooker,
        "version": meta.version,
        "source_path": meta.source_path,
        "dependencies": meta
            .dependencies
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        "import_settings": meta.import_settings,
    });
    let bytes = serde_json::to_vec(&fingerprint).map_err(|error| AssetError::Internal {
        message: format!("failed to serialize asset meta fingerprint: {error}"),
    })?;
    Ok(hash_bytes(&bytes))
}

fn hash_bytes(bytes: &[u8]) -> String {
    const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;

    let mut hash = FNV_OFFSET_BASIS;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    format!("{hash:016x}")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AssetKind {
    Texture,
    SoundClip,
    MusicTrack,
}

fn dependency_cycles(metas: &[(PathBuf, AssetMeta)]) -> Vec<Vec<AssetId>> {
    let graph: std::collections::HashMap<AssetId, Vec<AssetId>> = metas
        .iter()
        .map(|(_, meta)| (meta.asset_id, meta.dependencies.clone()))
        .collect();
    let mut seen = std::collections::HashSet::new();
    let mut cycles = Vec::new();

    for asset_id in graph.keys().copied() {
        if let Some(cycle) = dependency_cycle_from_graph(&graph, asset_id) {
            let key = canonical_cycle_key(&cycle);
            if seen.insert(key) {
                cycles.push(cycle);
            }
        }
    }

    cycles
}

fn dependency_cycle_from_graph(
    graph: &std::collections::HashMap<AssetId, Vec<AssetId>>,
    start: AssetId,
) -> Option<Vec<AssetId>> {
    fn visit(
        graph: &std::collections::HashMap<AssetId, Vec<AssetId>>,
        current: AssetId,
        stack: &mut Vec<AssetId>,
        visited: &mut std::collections::HashSet<AssetId>,
    ) -> Option<Vec<AssetId>> {
        if let Some(index) = stack.iter().position(|asset_id| *asset_id == current) {
            let mut cycle = stack[index..].to_vec();
            cycle.push(current);
            return Some(cycle);
        }

        if !visited.insert(current) {
            return None;
        }

        stack.push(current);
        for dependency in graph.get(&current).into_iter().flatten().copied() {
            if let Some(cycle) = visit(graph, dependency, stack, visited) {
                return Some(cycle);
            }
        }
        stack.pop();
        None
    }

    let mut stack = Vec::new();
    let mut visited = std::collections::HashSet::new();
    visit(graph, start, &mut stack, &mut visited)
}

fn canonical_cycle_key(cycle: &[AssetId]) -> String {
    let cycle = if cycle.len() > 1 && cycle.first() == cycle.last() {
        &cycle[..cycle.len() - 1]
    } else {
        cycle
    };

    if cycle.is_empty() {
        return String::new();
    }

    let labels: Vec<_> = cycle.iter().map(ToString::to_string).collect();
    let mut best = None::<String>;
    for start in 0..labels.len() {
        let mut rotated = Vec::with_capacity(labels.len() + 1);
        for offset in 0..labels.len() {
            rotated.push(labels[(start + offset) % labels.len()].clone());
        }
        rotated.push(rotated[0].clone());
        let candidate = rotated.join(" -> ");
        if best.as_ref().map_or(true, |current| candidate < *current) {
            best = Some(candidate);
        }
    }
    best.unwrap_or_default()
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

    fn write_audio_placeholder(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        std::fs::write(path, b"placeholder audio")?;
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
                .get("stream")
                .and_then(|value| value.as_bool()),
            Some(true)
        );
        Ok(())
    }

    #[test]
    fn import_does_not_treat_partial_audio_names_as_music() -> Result<(), Box<dyn std::error::Error>>
    {
        let dir = tempdir()?;
        let source = dir.path().join("musicbox.wav");
        write_audio_placeholder(&source)?;

        let meta = import_path(dir.path(), &source)?;

        assert_eq!(meta.asset_type, "sound_clip");
        assert_eq!(
            meta.import_settings
                .get("stream")
                .and_then(|value| value.as_bool()),
            Some(false)
        );
        Ok(())
    }

    #[test]
    fn import_normalizes_audio_meta_when_stream_flag_changes(
    ) -> Result<(), Box<dyn std::error::Error>> {
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
                .get("stream")
                .and_then(|value| value.as_bool()),
            Some(true)
        );
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
            assets: vec![
                build_manifest_entry(&updated_a),
                build_manifest_entry(&updated_b),
            ],
        };
        write_manifest(&config, &manifest)?;
        let cooked_a = config.cooked_root().join(cooked_relative_path(&updated_a));
        let cooked_b = config.cooked_root().join(cooked_relative_path(&updated_b));
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
}
