use std::path::Path;

use super::import_path;
use super::util::normalize_dependencies;
use super::{cooked_relative_path, source_asset_kind, AssetKind, CookRegistry};
use crate::asset::types::{AssetConfig, AssetError, AssetId, AssetManifestEntry, AssetMeta};
pub(super) fn cook_video_clip_registered(
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

pub(super) fn update_video_clip_dependencies(
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

pub(crate) fn build_manifest_entry(
    meta: &AssetMeta,
    registry: &CookRegistry,
) -> AssetManifestEntry {
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
