use std::path::Path;

use super::{
    default_meta_for_source, meta_path_for, normalize_meta_for_source, read_meta,
    resolve_source_path, source_key, update_source_and_meta_hashes, write_meta, CookRegistry,
};
use crate::asset::types::{AssetError, AssetMeta};

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
