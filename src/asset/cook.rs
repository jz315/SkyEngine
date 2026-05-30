use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use super::font::{encode_font_cooked, FontAsset};
use super::texture::{decode_texture_source_bytes, encode_texture_cooked, TextureColorSpace};
use super::types::{
    normalize_source_key, AssetConfig, AssetCookedSchema, AssetError, AssetId, AssetManifestEntry,
    AssetManifestProvenance, AssetMeta, AssetRegistryManifest, ASSET_SYSTEM_VERSION,
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

pub type CookFn = fn(&AssetConfig, &Path, &AssetMeta, &Path) -> Result<(), AssetError>;
pub type DependencyFn = fn(&AssetConfig, &Path, &mut AssetMeta) -> Result<(), AssetError>;
pub type ImportSettingsFn = fn(&str) -> serde_json::Value;
pub type NormalizeImportSettingsFn = fn(&str, &mut serde_json::Value);

#[derive(Clone, Copy, Debug)]
pub struct CookerDescriptor {
    pub asset_type: &'static str,
    pub importer: &'static str,
    pub cooker: &'static str,
    pub version: u32,
    pub dependency_schema: Option<&'static str>,
    pub source_extensions: &'static [&'static str],
    pub cooked_dir: &'static str,
    pub cooked_extension: &'static str,
    pub cook: CookFn,
    pub update_dependencies: Option<DependencyFn>,
    pub default_import_settings: Option<ImportSettingsFn>,
    pub normalize_import_settings: Option<NormalizeImportSettingsFn>,
}

impl CookerDescriptor {
    fn cooked_relative_path(&self, asset_id: AssetId) -> String {
        format!("{}/{}.{}", self.cooked_dir, asset_id, self.cooked_extension)
    }

    #[must_use]
    pub fn cooked_schema(&self) -> AssetCookedSchema {
        let schema = AssetCookedSchema::new(self.cooker, self.version);
        match self.dependency_schema {
            Some(dependency_schema) => schema.with_dependency_schema(dependency_schema),
            None => schema,
        }
    }
}

#[derive(Clone, Debug)]
pub struct CookRegistry {
    cookers: Vec<&'static CookerDescriptor>,
}

impl CookRegistry {
    #[must_use]
    pub fn with_builtins() -> Self {
        Self {
            cookers: BUILTIN_COOKERS.to_vec(),
        }
    }

    #[must_use]
    pub fn empty() -> Self {
        Self {
            cookers: Vec::new(),
        }
    }

    pub fn register(&mut self, descriptor: &'static CookerDescriptor) {
        self.cookers.push(descriptor);
    }

    #[must_use]
    pub fn with_registered(mut self, descriptor: &'static CookerDescriptor) -> Self {
        self.register(descriptor);
        self
    }

    fn cooker_for_asset_type(&self, asset_type: &str) -> Option<&'static CookerDescriptor> {
        self.cookers
            .iter()
            .copied()
            .rev()
            .find(|descriptor| descriptor.asset_type == asset_type)
    }

    fn supports_source(&self, path: &Path) -> bool {
        let Some(extension) = path.extension().and_then(OsStr::to_str) else {
            return false;
        };
        self.cookers.iter().copied().any(|descriptor| {
            descriptor
                .source_extensions
                .iter()
                .any(|candidate| candidate.eq_ignore_ascii_case(extension))
        })
    }

    fn default_cooker_for_source(
        &self,
        source_key: &str,
        import_settings: Option<&serde_json::Value>,
    ) -> Option<&'static CookerDescriptor> {
        let extension = Path::new(source_key).extension()?.to_str()?;
        let candidates = self
            .cookers
            .iter()
            .copied()
            .filter(|descriptor| {
                descriptor
                    .source_extensions
                    .iter()
                    .any(|candidate| candidate.eq_ignore_ascii_case(extension))
            })
            .collect::<Vec<_>>();

        if let Some(asset_type) = requested_asset_type(import_settings) {
            return candidates
                .iter()
                .copied()
                .rev()
                .find(|descriptor| descriptor.asset_type == asset_type);
        }

        if let Some(import_settings) = import_settings {
            let mut normalized_settings = import_settings.clone();
            for descriptor in &candidates {
                if let Some(normalize_import_settings) = descriptor.normalize_import_settings {
                    normalize_import_settings(source_key, &mut normalized_settings);
                    if let Some(asset_type) = requested_asset_type(Some(&normalized_settings)) {
                        return candidates
                            .iter()
                            .copied()
                            .rev()
                            .find(|descriptor| descriptor.asset_type == asset_type);
                    }
                }
            }
        } else {
            for descriptor in &candidates {
                if let Some(default_import_settings) = descriptor.default_import_settings {
                    let settings = default_import_settings(source_key);
                    if let Some(asset_type) = requested_asset_type(Some(&settings)) {
                        return candidates
                            .iter()
                            .copied()
                            .rev()
                            .find(|descriptor| descriptor.asset_type == asset_type);
                    }
                }
            }
        }

        candidates.into_iter().next()
    }

    fn default_import_settings_for_source(&self, source_key: &str) -> serde_json::Value {
        self.default_cooker_for_source(source_key, None)
            .and_then(|descriptor| descriptor.default_import_settings)
            .map(|default_import_settings| default_import_settings(source_key))
            .unwrap_or_else(|| serde_json::json!({}))
    }
}

impl Default for CookRegistry {
    fn default() -> Self {
        Self::with_builtins()
    }
}

const TEXTURE_COOKER: CookerDescriptor = CookerDescriptor {
    asset_type: "texture",
    importer: "texture.image",
    cooker: "texture.rgba8",
    version: 1,
    dependency_schema: None,
    source_extensions: &["png", "jpg", "jpeg"],
    cooked_dir: "texture",
    cooked_extension: "skytx",
    cook: cook_texture_registered,
    update_dependencies: None,
    default_import_settings: Some(default_texture_import_settings),
    normalize_import_settings: Some(normalize_texture_import_settings),
};

const FONT_COOKER: CookerDescriptor = CookerDescriptor {
    asset_type: "font",
    importer: "font.raw",
    cooker: "font.raw_bytes",
    version: 1,
    dependency_schema: None,
    source_extensions: &["ttf", "otf"],
    cooked_dir: "misc",
    cooked_extension: "skyasset",
    cook: cook_font_registered,
    update_dependencies: None,
    default_import_settings: None,
    normalize_import_settings: None,
};

const SOUND_CLIP_COOKER: CookerDescriptor = CookerDescriptor {
    asset_type: "sound_clip",
    importer: "audio.symphonia",
    cooker: "audio.copy",
    version: 1,
    dependency_schema: None,
    source_extensions: &["wav", "ogg", "mp3"],
    cooked_dir: "audio",
    cooked_extension: "skyaudio",
    cook: cook_audio_registered,
    update_dependencies: None,
    default_import_settings: Some(default_audio_import_settings),
    normalize_import_settings: Some(normalize_audio_import_settings),
};

const MUSIC_TRACK_COOKER: CookerDescriptor = CookerDescriptor {
    asset_type: "music_track",
    importer: "audio.symphonia",
    cooker: "audio.copy",
    version: 1,
    dependency_schema: None,
    source_extensions: &["wav", "ogg", "mp3"],
    cooked_dir: "audio",
    cooked_extension: "skyaudio",
    cook: cook_audio_registered,
    update_dependencies: None,
    default_import_settings: Some(default_audio_import_settings),
    normalize_import_settings: Some(normalize_audio_import_settings),
};

const VIDEO_CLIP_COOKER: CookerDescriptor = CookerDescriptor {
    asset_type: "video_clip",
    importer: "video.frame_sequence",
    cooker: "video.clip_json",
    version: 1,
    dependency_schema: Some("video.frames.texture"),
    source_extensions: &["skyvideo"],
    cooked_dir: "video",
    cooked_extension: "skyvideo",
    cook: cook_video_clip_registered,
    update_dependencies: Some(update_video_clip_dependencies),
    default_import_settings: None,
    normalize_import_settings: None,
};

const BUILTIN_COOKERS: &[&CookerDescriptor] = &[
    &TEXTURE_COOKER,
    &FONT_COOKER,
    &SOUND_CLIP_COOKER,
    &MUSIC_TRACK_COOKER,
    &VIDEO_CLIP_COOKER,
];

fn default_texture_import_settings(_source_key: &str) -> serde_json::Value {
    serde_json::json!({ "srgb": true })
}

fn normalize_texture_import_settings(_source_key: &str, import_settings: &mut serde_json::Value) {
    let srgb = import_settings
        .get("srgb")
        .and_then(|value| value.as_bool())
        .unwrap_or(true);
    set_import_setting_bool(import_settings, "srgb", srgb);
}

fn default_audio_import_settings(source_key: &str) -> serde_json::Value {
    let stream = infer_default_audio_stream(source_key);
    serde_json::json!({
        "asset_type": audio_asset_type_for_stream(stream),
        "stream": stream,
    })
}

fn requested_asset_type(import_settings: Option<&serde_json::Value>) -> Option<&str> {
    import_settings
        .and_then(|settings| settings.get("asset_type"))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn normalize_audio_import_settings(source_key: &str, import_settings: &mut serde_json::Value) {
    let stream = requested_asset_type(Some(import_settings))
        .and_then(stream_for_audio_asset_type)
        .or_else(|| {
            import_settings
                .get("stream")
                .and_then(|value| value.as_bool())
        })
        .unwrap_or_else(|| infer_default_audio_stream(source_key));
    set_import_setting_string(
        import_settings,
        "asset_type",
        audio_asset_type_for_stream(stream),
    );
    set_import_setting_bool(import_settings, "stream", stream);
}

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

pub fn import_path(
    asset_root: impl AsRef<Path>,
    path: impl AsRef<Path>,
) -> Result<AssetMeta, AssetError> {
    let registry = CookRegistry::default();
    import_path_with_registry(asset_root, path, &registry)
}

pub fn import_path_with_registry(
    asset_root: impl AsRef<Path>,
    path: impl AsRef<Path>,
    registry: &CookRegistry,
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
        default_meta_for_source(&source_key, registry)?
    };
    normalize_meta_for_source(&source_key, &mut meta, registry)?;
    update_source_and_meta_hashes(&source, &mut meta)?;

    write_meta(&meta_path, &meta)?;
    Ok(meta)
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

pub fn verify(config: &AssetConfig) -> Result<VerifyReport, AssetError> {
    let registry = CookRegistry::default();
    verify_with_registry(config, &registry)
}

pub fn verify_with_registry(
    config: &AssetConfig,
    registry: &CookRegistry,
) -> Result<VerifyReport, AssetError> {
    let mut report = VerifyReport::default();
    let source_files = collect_source_files(&config.asset_root, registry)?;
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
        if let Some(issue) = meta_cooker_drift(registry, meta) {
            report.issues.push(format!("{:?}: {issue}", meta_path));
        }

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

        let cooked_path = config
            .cooked_root()
            .join(cooked_relative_path(meta, registry));
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

            let expected = build_manifest_entry(meta, registry);
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

            let expected_provenance = build_manifest_provenance(config, meta, registry)?;
            let Some(provenance) = manifest
                .provenance
                .iter()
                .find(|provenance| provenance.asset_id == meta.asset_id)
            else {
                report.issues.push(format!(
                    "manifest missing provenance for asset {}",
                    meta.asset_id
                ));
                continue;
            };
            if provenance != &expected_provenance {
                report.issues.push(format!(
                    "manifest provenance mismatch for asset {}",
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

fn cook_video_clip_registered(
    config: &AssetConfig,
    source: &Path,
    _meta: &AssetMeta,
    cooked_path: &Path,
) -> Result<(), AssetError> {
    cook_video_clip(config, source, cooked_path)
}

fn cook_video_clip(
    config: &AssetConfig,
    source: &Path,
    cooked_path: &Path,
) -> Result<(), AssetError> {
    let descriptor = load_video_clip_descriptor(config, source)?;
    let bytes = serde_json::to_vec_pretty(&descriptor).map_err(|error| AssetError::Json {
        path: cooked_path.to_path_buf(),
        message: error.to_string(),
    })?;
    std::fs::write(cooked_path, bytes).map_err(|error| AssetError::Io {
        path: cooked_path.to_path_buf(),
        message: error.to_string(),
    })
}

fn update_video_clip_dependencies(
    config: &AssetConfig,
    source: &Path,
    meta: &mut AssetMeta,
) -> Result<(), AssetError> {
    let descriptor = load_video_clip_descriptor(config, source)?;
    meta.dependencies = normalize_dependencies(
        descriptor
            .frames
            .iter()
            .map(|frame| frame.texture)
            .collect(),
    );
    Ok(())
}

fn load_video_clip_descriptor(
    config: &AssetConfig,
    source: &Path,
) -> Result<CookedVideoClipDescriptor, AssetError> {
    let bytes = std::fs::read(source).map_err(|error| AssetError::Io {
        path: source.to_path_buf(),
        message: error.to_string(),
    })?;
    let source_descriptor: SourceVideoClipDescriptor =
        serde_json::from_slice(&bytes).map_err(|error| AssetError::Json {
            path: source.to_path_buf(),
            message: error.to_string(),
        })?;

    if source_descriptor.width == 0 || source_descriptor.height == 0 {
        return Err(AssetError::InvalidCookedAsset {
            id: None,
            message: format!(
                "video descriptor {:?} must use non-zero width and height",
                source
            ),
        });
    }
    if source_descriptor.frames.is_empty() {
        return Err(AssetError::InvalidCookedAsset {
            id: None,
            message: format!(
                "video descriptor {:?} must contain at least one frame",
                source
            ),
        });
    }
    if let Some(fps) = source_descriptor.fps {
        validate_positive_finite(fps, "fps", source)?;
    }

    let default_duration = source_descriptor
        .fps
        .map(|fps| 1.0 / fps)
        .unwrap_or(1.0 / 30.0);
    let mut frames = Vec::with_capacity(source_descriptor.frames.len());
    for frame in source_descriptor.frames {
        let texture_ref = frame.texture_ref();
        let texture = resolve_video_texture_dependency(config, source, texture_ref)?;
        let duration_seconds = frame.duration_seconds().unwrap_or(default_duration);
        validate_positive_finite(duration_seconds, "frame duration", source)?;
        frames.push(CookedVideoFrameDescriptor {
            texture,
            duration_ms: None,
            duration_seconds: Some(duration_seconds),
        });
    }

    Ok(CookedVideoClipDescriptor {
        width: source_descriptor.width,
        height: source_descriptor.height,
        frames,
    })
}

fn resolve_video_texture_dependency(
    config: &AssetConfig,
    video_source: &Path,
    texture_ref: &str,
) -> Result<AssetId, AssetError> {
    if let Ok(asset_id) = AssetId::parse_str(texture_ref) {
        return Ok(asset_id);
    }

    let texture_path = Path::new(texture_ref);
    let candidate = if texture_path.is_absolute() {
        texture_path.to_path_buf()
    } else {
        video_source
            .parent()
            .unwrap_or(&config.asset_root)
            .join(texture_path)
    };
    let source = if candidate.exists() {
        candidate
    } else {
        config.asset_root.join(texture_path)
    };

    if source_asset_kind(&source) != Some(AssetKind::Texture) {
        return Err(AssetError::Unsupported {
            message: format!(
                "video frame {:?} must reference a supported texture asset",
                texture_ref
            ),
        });
    }

    let meta = import_path(&config.asset_root, source)?;
    Ok(meta.asset_id)
}

fn validate_positive_finite(value: f64, label: &str, source: &Path) -> Result<(), AssetError> {
    if value.is_finite() && value > 0.0 {
        Ok(())
    } else {
        Err(AssetError::InvalidCookedAsset {
            id: None,
            message: format!("video descriptor {:?} has invalid {label}: {value}", source),
        })
    }
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

#[derive(serde::Serialize)]
struct CookedVideoClipDescriptor {
    width: u32,
    height: u32,
    frames: Vec<CookedVideoFrameDescriptor>,
}

#[derive(serde::Serialize)]
struct CookedVideoFrameDescriptor {
    texture: AssetId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    duration_ms: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    duration_seconds: Option<f64>,
}

#[derive(serde::Deserialize)]
struct SourceVideoClipDescriptor {
    width: u32,
    height: u32,
    #[serde(default)]
    fps: Option<f64>,
    frames: Vec<SourceVideoFrameDescriptor>,
}

#[derive(serde::Deserialize)]
#[serde(untagged)]
enum SourceVideoFrameDescriptor {
    Path(String),
    Object {
        texture: String,
        #[serde(default)]
        duration_ms: Option<f64>,
        #[serde(default)]
        duration_seconds: Option<f64>,
    },
}

impl SourceVideoFrameDescriptor {
    fn texture_ref(&self) -> &str {
        match self {
            Self::Path(path) => path,
            Self::Object { texture, .. } => texture,
        }
    }

    fn duration_seconds(&self) -> Option<f64> {
        match self {
            Self::Path(_) => None,
            Self::Object {
                duration_ms,
                duration_seconds,
                ..
            } => duration_seconds.or_else(|| duration_ms.map(|ms| ms / 1000.0)),
        }
    }
}

fn build_manifest_entry(meta: &AssetMeta, registry: &CookRegistry) -> AssetManifestEntry {
    AssetManifestEntry {
        asset_id: meta.asset_id,
        asset_type: meta.asset_type.clone(),
        importer: meta.importer.clone(),
        cooker: meta.cooker.clone(),
        version: meta.version,
        source_path: meta.source_path.clone(),
        cooked_path: cooked_relative_path(meta, registry),
        dependencies: meta.dependencies.clone(),
        import_settings: meta.import_settings.clone(),
    }
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

fn set_import_setting_string(import_settings: &mut serde_json::Value, key: &str, value: &str) {
    import_settings_object_mut(import_settings).insert(
        key.to_string(),
        serde_json::Value::String(value.to_string()),
    );
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

fn audio_asset_type_for_stream(stream: bool) -> &'static str {
    if stream {
        "music_track"
    } else {
        "sound_clip"
    }
}

fn stream_for_audio_asset_type(asset_type: &str) -> Option<bool> {
    match asset_type {
        "music_track" => Some(true),
        "sound_clip" => Some(false),
        _ => None,
    }
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

fn cooked_relative_path(meta: &AssetMeta, registry: &CookRegistry) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;
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
    fn cook_all_writes_video_clip_with_texture_dependencies(
    ) -> Result<(), Box<dyn std::error::Error>> {
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
    fn import_does_not_treat_partial_audio_names_as_music() -> Result<(), Box<dyn std::error::Error>>
    {
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
}
