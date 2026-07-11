use std::any::Any;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::{
    hash_bytes, AssetLoadPhaseTracker, AssetLoadTimingSample, AssetSourceLoadPhase,
    LoadedSourceAsset, TimedAssetLoadError,
};
use crate::asset::font::FontAsset;
use crate::asset::provider::{AssetSourceLocation, ResolvedAssetSource};
use crate::asset::registry::ErasedAssetFactory;
use crate::asset::texture::{decode_texture_source_bytes, TextureAsset, TextureColorSpace};
use crate::asset::types::{
    Asset, AssetError, AssetId, AssetLoadContext, AssetManifestEntry, LoadedAsset,
};

pub(crate) fn load_resolved_source_asset(
    id: AssetId,
    source: &ResolvedAssetSource,
    factory: &dyn ErasedAssetFactory,
    asset_root: &Path,
    cooked_root: &Path,
) -> Result<LoadedSourceAsset, TimedAssetLoadError> {
    load_resolved_source_asset_inner(id, source, factory, asset_root, cooked_root, None)
}

pub(crate) fn load_resolved_source_asset_with_phase(
    id: AssetId,
    source: &ResolvedAssetSource,
    factory: &dyn ErasedAssetFactory,
    asset_root: &Path,
    cooked_root: &Path,
    phase: &AssetLoadPhaseTracker,
) -> Result<LoadedSourceAsset, TimedAssetLoadError> {
    load_resolved_source_asset_inner(id, source, factory, asset_root, cooked_root, Some(phase))
}

fn load_resolved_source_asset_inner(
    id: AssetId,
    source: &ResolvedAssetSource,
    factory: &dyn ErasedAssetFactory,
    asset_root: &Path,
    cooked_root: &Path,
    phase: Option<&AssetLoadPhaseTracker>,
) -> Result<LoadedSourceAsset, TimedAssetLoadError> {
    validate_runtime_cooked_schema(factory, source.entry(), source.location())?;
    let total_start = Instant::now();
    let read_start = Instant::now();
    if let Some(phase) = phase {
        phase.set(AssetSourceLoadPhase::Reading);
    }
    let bytes = source.read_bytes(id).map_err(|error| TimedAssetLoadError {
        error: Box::new(error),
        timings: AssetLoadTimingSample {
            sampled: true,
            read_time: read_start.elapsed(),
            decode_time: Duration::ZERO,
            total_time: total_start.elapsed(),
        },
    })?;
    let read_time = read_start.elapsed();
    let content_hash = hash_bytes(&bytes);
    let decode_start = Instant::now();
    if let Some(phase) = phase {
        phase.set(AssetSourceLoadPhase::Decoding);
    }
    let loaded = match source.location() {
        AssetSourceLocation::Raw(path) => load_raw_source_asset(source.entry(), path, &bytes),
        AssetSourceLocation::Cooked(_)
        | AssetSourceLocation::Package(_)
        | AssetSourceLocation::Bundle { .. } => factory.load(AssetLoadContext {
            asset_id: id,
            entry: source.entry(),
            bytes: &bytes,
            asset_root,
            cooked_root,
        }),
        #[cfg(test)]
        AssetSourceLocation::Memory { .. } => factory.load(AssetLoadContext {
            asset_id: id,
            entry: source.entry(),
            bytes: &bytes,
            asset_root,
            cooked_root,
        }),
    };
    let timings = AssetLoadTimingSample {
        sampled: true,
        read_time,
        decode_time: decode_start.elapsed(),
        total_time: total_start.elapsed(),
    };
    match loaded {
        Ok(loaded) => Ok(LoadedSourceAsset {
            loaded,
            content_hash,
            timings,
        }),
        Err(error) => Err(TimedAssetLoadError {
            error: Box::new(error),
            timings,
        }),
    }
}

fn validate_runtime_cooked_schema(
    factory: &dyn ErasedAssetFactory,
    entry: &AssetManifestEntry,
    location: &AssetSourceLocation,
) -> Result<(), AssetError> {
    if matches!(location, AssetSourceLocation::Raw(_)) {
        return Ok(());
    }
    let Some(schema) = factory.cooked_schema() else {
        return Ok(());
    };
    if entry.cooker == schema.cooker && entry.version == schema.version {
        return Ok(());
    }
    Err(AssetError::CookedSchemaMismatch {
        id: entry.asset_id,
        expected_cooker: schema.cooker.to_string(),
        expected_version: schema.version,
        actual_cooker: entry.cooker.clone(),
        actual_version: entry.version,
    })
}

fn load_raw_source_asset(
    entry: &AssetManifestEntry,
    raw_source_path: &Path,
    bytes: &[u8],
) -> Result<LoadedAsset<Arc<dyn Any + Send + Sync>>, AssetError> {
    if entry.asset_type == TextureAsset::TYPE {
        let texture = decode_texture_source_bytes(raw_source_path, bytes, TextureColorSpace::Srgb)?;
        return Ok(LoadedAsset::new(
            Arc::new(texture) as Arc<dyn Any + Send + Sync>
        ));
    }
    if entry.asset_type == FontAsset::TYPE {
        return Ok(LoadedAsset::new(
            Arc::new(FontAsset::new(Arc::<[u8]>::from(bytes.to_vec())))
                as Arc<dyn Any + Send + Sync>,
        ));
    }
    Err(AssetError::Unsupported {
        message: format!(
            "raw source loading is not supported for asset type `{}`",
            entry.asset_type
        ),
    })
}
