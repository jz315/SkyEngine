use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use super::font::{encode_font_cooked, FontAsset};
use super::texture::{decode_texture_source_bytes, encode_texture_cooked, TextureColorSpace};
use super::types::{
    normalize_source_key, AssetConfig, AssetError, AssetId, AssetManifestEntry,
    AssetManifestProvenance, AssetMeta, AssetRegistryManifest, ASSET_SYSTEM_VERSION,
};

mod registry;
pub use registry::{
    CookFn, CookRegistry, CookerDescriptor, DependencyFn, ImportSettingsFn,
    NormalizeImportSettingsFn,
};
mod builtins;
mod report;
pub use report::VerifyReport;
mod util;
use util::hash_bytes;
mod import;
pub use import::{import_path, import_path_with_registry};
mod verify;
pub use verify::{verify, verify_with_registry};
mod settings;
use settings::{
    default_audio_import_settings, default_texture_import_settings,
    normalize_audio_import_settings, normalize_texture_import_settings, requested_asset_type,
};
mod video;
pub(crate) use video::build_manifest_entry;
use video::{cook_video_clip_registered, update_video_clip_dependencies};

fn cooker_for_meta(
    registry: &CookRegistry,
    meta: &AssetMeta,
) -> Result<&'static CookerDescriptor, AssetError> {
    registry
        .cooker_for_asset_type(&meta.asset_type)
        .ok_or_else(|| AssetError::Unsupported {
            message: format!("unsupported asset type `{}`", meta.asset_type),
        })
}

fn apply_cooker_defaults(meta: &mut AssetMeta, descriptor: &CookerDescriptor) {
    meta.asset_type = descriptor.asset_type.to_string();
    meta.importer = descriptor.importer.to_string();
    meta.cooker = descriptor.cooker.to_string();
    meta.version = descriptor.version;
}

fn meta_cooker_drift(registry: &CookRegistry, meta: &AssetMeta) -> Option<String> {
    let Some(descriptor) = registry.cooker_for_asset_type(&meta.asset_type) else {
        return Some(format!(
            "asset {} uses unsupported cooker asset type `{}`",
            meta.asset_id, meta.asset_type
        ));
    };

    if meta.importer != descriptor.importer {
        return Some(format!(
            "importer drift for asset {}: meta uses `{}`, registered `{}`",
            meta.asset_id, meta.importer, descriptor.importer
        ));
    }
    if meta.cooker != descriptor.cooker {
        return Some(format!(
            "cooker drift for asset {}: meta uses `{}`, registered `{}`",
            meta.asset_id, meta.cooker, descriptor.cooker
        ));
    }
    if meta.version != descriptor.version {
        return Some(format!(
            "cooker version drift for asset {}: meta uses {}, registered {} for `{}`",
            meta.asset_id, meta.version, descriptor.version, descriptor.cooker
        ));
    }

    None
}

pub fn cook_all(config: &AssetConfig) -> Result<AssetRegistryManifest, AssetError> {
    let registry = CookRegistry::default();
    cook_all_with_registry(config, &registry)
}

pub fn cook_all_with_registry(
    config: &AssetConfig,
    registry: &CookRegistry,
) -> Result<AssetRegistryManifest, AssetError> {
    let source_files = collect_source_files(&config.asset_root, registry)?;
    let mut manifest = AssetRegistryManifest {
        version: ASSET_SYSTEM_VERSION,
        target: config.target.clone(),
        provenance: Vec::new(),
        assets: Vec::new(),
    };

    for source in source_files {
        let meta = import_path_with_registry(&config.asset_root, &source, registry)?;
        let record = cook_meta(config, &source, &meta, registry)?;
        manifest.assets.push(record.entry);
        manifest.provenance.push(record.provenance);
    }

    manifest
        .assets
        .sort_by(|a, b| a.source_path.cmp(&b.source_path));
    manifest
        .provenance
        .sort_by(|a, b| a.asset_id.to_string().cmp(&b.asset_id.to_string()));
    write_manifest(config, &manifest)?;
    Ok(manifest)
}

pub fn cook_target(config: &AssetConfig, query: &str) -> Result<AssetRegistryManifest, AssetError> {
    let registry = CookRegistry::default();
    cook_target_with_registry(config, query, &registry)
}

pub fn cook_target_with_registry(
    config: &AssetConfig,
    query: &str,
    registry: &CookRegistry,
) -> Result<AssetRegistryManifest, AssetError> {
    let source_files = collect_source_files(&config.asset_root, registry)?;
    let mut selected = false;
    let mut manifest = AssetRegistryManifest {
        version: ASSET_SYSTEM_VERSION,
        target: config.target.clone(),
        provenance: Vec::new(),
        assets: Vec::new(),
    };

    for source in source_files {
        let meta = import_path_with_registry(&config.asset_root, &source, registry)?;
        let matches = matches_query(query, &meta, &source, &config.asset_root)?;
        if matches {
            selected = true;
        }
        let record = if matches {
            cook_meta(config, &source, &meta, registry)?
        } else {
            ensure_cooked_entry(config, &source, &meta, registry)?
        };
        manifest.assets.push(record.entry);
        manifest.provenance.push(record.provenance);
    }

    if !selected {
        return Err(AssetError::AssetPathNotFound {
            path: PathBuf::from(query),
        });
    }

    manifest
        .assets
        .sort_by(|a, b| a.source_path.cmp(&b.source_path));
    manifest
        .provenance
        .sort_by(|a, b| a.asset_id.to_string().cmp(&b.asset_id.to_string()));
    write_manifest(config, &manifest)?;
    Ok(manifest)
}

fn ensure_cooked_entry(
    config: &AssetConfig,
    source: &Path,
    meta: &AssetMeta,
    registry: &CookRegistry,
) -> Result<CookedManifestRecord, AssetError> {
    let cooked_path = config
        .cooked_root()
        .join(cooked_relative_path(meta, registry));
    if !cooked_path.exists() || is_asset_dirty(source, &meta_path_for(source), &cooked_path, meta)?
    {
        cook_meta(config, source, meta, registry)
    } else {
        Ok(CookedManifestRecord {
            entry: build_manifest_entry(meta, registry),
            provenance: build_manifest_provenance(config, meta, registry)?,
        })
    }
}

fn cook_meta(
    config: &AssetConfig,
    source: &Path,
    meta: &AssetMeta,
    registry: &CookRegistry,
) -> Result<CookedManifestRecord, AssetError> {
    let mut updated_meta = meta.clone();
    let descriptor = cooker_for_meta(registry, &updated_meta)?;
    if let Some(update_dependencies) = descriptor.update_dependencies {
        update_dependencies(config, source, &mut updated_meta)?;
    }
    let descriptor = cooker_for_meta(registry, &updated_meta)?;

    let cooked_relative = cooked_relative_path(&updated_meta, registry);
    let cooked_path = config.cooked_root().join(&cooked_relative);
    let meta_path = meta_path_for(source);

    if !cooked_path.exists() || is_asset_dirty(source, &meta_path, &cooked_path, &updated_meta)? {
        if let Some(parent) = cooked_path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| AssetError::Io {
                path: parent.to_path_buf(),
                message: error.to_string(),
            })?;
        }

        (descriptor.cook)(config, source, &updated_meta, &cooked_path)?;
    }

    update_source_and_meta_hashes(source, &mut updated_meta)?;
    updated_meta.cooked_hash = Some(file_hash(&cooked_path)?);
    write_meta(&meta_path, &updated_meta)?;

    let mut entry = build_manifest_entry(&updated_meta, registry);
    entry.cooked_path = normalize_source_key(&cooked_relative);
    let provenance = build_manifest_provenance(config, &updated_meta, registry)?;
    Ok(CookedManifestRecord { entry, provenance })
}

fn cook_texture_registered(
    _config: &AssetConfig,
    source: &Path,
    meta: &AssetMeta,
    cooked_path: &Path,
) -> Result<(), AssetError> {
    cook_texture(source, meta, cooked_path)
}

fn cook_texture(source: &Path, meta: &AssetMeta, cooked_path: &Path) -> Result<(), AssetError> {
    let bytes = std::fs::read(source).map_err(|error| AssetError::Io {
        path: source.to_path_buf(),
        message: error.to_string(),
    })?;
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
    let asset = decode_texture_source_bytes(source, &bytes, color_space)?;
    let bytes = encode_texture_cooked(&asset);
    std::fs::write(cooked_path, bytes).map_err(|error| AssetError::Io {
        path: cooked_path.to_path_buf(),
        message: error.to_string(),
    })
}

fn cook_font_registered(
    _config: &AssetConfig,
    source: &Path,
    _meta: &AssetMeta,
    cooked_path: &Path,
) -> Result<(), AssetError> {
    cook_font(source, cooked_path)
}

fn cook_font(source: &Path, cooked_path: &Path) -> Result<(), AssetError> {
    let bytes = std::fs::read(source).map_err(|error| AssetError::Io {
        path: source.to_path_buf(),
        message: error.to_string(),
    })?;
    let asset = FontAsset::new(bytes);
    let bytes = encode_font_cooked(&asset);
    std::fs::write(cooked_path, bytes).map_err(|error| AssetError::Io {
        path: cooked_path.to_path_buf(),
        message: error.to_string(),
    })
}

fn cook_audio_registered(
    _config: &AssetConfig,
    source: &Path,
    _meta: &AssetMeta,
    cooked_path: &Path,
) -> Result<(), AssetError> {
    cook_audio(source, cooked_path)
}

fn cook_audio(source: &Path, cooked_path: &Path) -> Result<(), AssetError> {
    std::fs::copy(source, cooked_path)
        .map_err(|error| AssetError::Io {
            path: cooked_path.to_path_buf(),
            message: error.to_string(),
        })
        .map(|_| ())
}

#[derive(Clone, Debug)]
struct CookedManifestRecord {
    entry: AssetManifestEntry,
    provenance: AssetManifestProvenance,
}

fn build_manifest_provenance(
    config: &AssetConfig,
    meta: &AssetMeta,
    registry: &CookRegistry,
) -> Result<AssetManifestProvenance, AssetError> {
    let dependency_schema = registry
        .cooker_for_asset_type(&meta.asset_type)
        .and_then(|descriptor| descriptor.dependency_schema);
    Ok(AssetManifestProvenance {
        asset_id: meta.asset_id,
        source_hash: meta.source_hash.clone(),
        cooked_hash: meta.cooked_hash.clone(),
        dependency_hash: dependency_hash(meta, dependency_schema)?,
        platform: config.target.clone(),
        profile: config.profile.clone(),
    })
}

fn dependency_hash(
    meta: &AssetMeta,
    dependency_schema: Option<&str>,
) -> Result<String, AssetError> {
    let fingerprint = serde_json::json!({
        "dependency_schema": dependency_schema,
        "dependencies": meta
            .dependencies
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
    });
    let bytes = serde_json::to_vec(&fingerprint).map_err(|error| AssetError::Internal {
        message: format!("failed to serialize asset dependency fingerprint: {error}"),
    })?;
    Ok(hash_bytes(&bytes))
}

pub(crate) fn read_meta(path: &Path) -> Result<AssetMeta, AssetError> {
    let bytes = std::fs::read(path).map_err(|error| AssetError::Io {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;
    serde_json::from_slice(&bytes).map_err(|error| AssetError::Json {
        path: path.to_path_buf(),
        message: error.to_string(),
    })
}

pub(crate) fn write_meta(path: &Path, meta: &AssetMeta) -> Result<(), AssetError> {
    let bytes = serde_json::to_vec_pretty(meta).map_err(|error| AssetError::Json {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;
    std::fs::write(path, bytes).map_err(|error| AssetError::Io {
        path: path.to_path_buf(),
        message: error.to_string(),
    })
}

pub(crate) fn read_manifest(config: &AssetConfig) -> Result<AssetRegistryManifest, AssetError> {
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

pub(crate) fn write_manifest(
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

fn collect_source_files(root: &Path, registry: &CookRegistry) -> Result<Vec<PathBuf>, AssetError> {
    let mut files = Vec::new();
    collect_recursive(root, &mut files, false, Some(registry))?;
    files.sort();
    Ok(files)
}

fn collect_meta_files(root: &Path) -> Result<Vec<PathBuf>, AssetError> {
    let mut files = Vec::new();
    collect_recursive(root, &mut files, true, None)?;
    files.sort();
    Ok(files)
}

fn collect_recursive(
    root: &Path,
    files: &mut Vec<PathBuf>,
    only_meta: bool,
    registry: Option<&CookRegistry>,
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
            collect_recursive(&path, files, only_meta, registry)?;
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

        if registry.is_some_and(|registry| registry.supports_source(&path)) {
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

pub(crate) fn meta_path_for(source: &Path) -> PathBuf {
    let file_name = source
        .file_name()
        .expect("source files should have file names")
        .to_string_lossy();
    source.with_file_name(format!("{file_name}.meta"))
}

fn normalize_meta_for_source(
    source_key: &str,
    meta: &mut AssetMeta,
    registry: &CookRegistry,
) -> Result<(), AssetError> {
    meta.source_path = source_key.to_string();

    if let Some(descriptor) =
        registry.default_cooker_for_source(source_key, Some(&meta.import_settings))
    {
        apply_cooker_defaults(meta, descriptor);
        if let Some(normalize_import_settings) = descriptor.normalize_import_settings {
            normalize_import_settings(source_key, &mut meta.import_settings);
        }
        return Ok(());
    }

    if let Some(asset_type) = requested_asset_type(Some(&meta.import_settings)) {
        return Err(AssetError::Unsupported {
            message: format!(
                "asset source `{source_key}` does not support requested asset type `{asset_type}`"
            ),
        });
    }

    Ok(())
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

fn default_meta_for_source(
    source_key: &str,
    registry: &CookRegistry,
) -> Result<AssetMeta, AssetError> {
    let import_settings = registry.default_import_settings_for_source(source_key);
    let descriptor = registry
        .default_cooker_for_source(source_key, Some(&import_settings))
        .ok_or_else(|| AssetError::Unsupported {
            message: format!("unsupported asset source `{source_key}`"),
        })?;
    Ok(AssetMeta {
        asset_id: AssetId::new(),
        asset_type: descriptor.asset_type.to_string(),
        importer: descriptor.importer.to_string(),
        cooker: descriptor.cooker.to_string(),
        version: descriptor.version,
        source_path: source_key.to_string(),
        source_hash: None,
        meta_hash: None,
        cooked_hash: None,
        dependencies: Vec::new(),
        import_settings,
    })
}

fn source_asset_kind(path: &Path) -> Option<AssetKind> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    match extension.as_str() {
        "png" | "jpg" | "jpeg" => Some(AssetKind::Texture),
        "ttf" | "otf" => Some(AssetKind::Font),
        "wav" | "ogg" | "mp3" => Some(AssetKind::SoundClip),
        "skyvideo" => Some(AssetKind::VideoClip),
        _ => None,
    }
}

pub(crate) fn cooked_relative_path(meta: &AssetMeta, registry: &CookRegistry) -> String {
    registry
        .cooker_for_asset_type(&meta.asset_type)
        .map(|descriptor| descriptor.cooked_relative_path(meta.asset_id))
        .unwrap_or_else(|| format!("misc/{}.skyasset", meta.asset_id))
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
    _meta_path: &Path,
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

    Ok(false)
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AssetKind {
    Texture,
    Font,
    SoundClip,
    VideoClip,
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
