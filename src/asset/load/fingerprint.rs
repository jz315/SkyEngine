use crate::asset::types::{AssetError, AssetId, AssetManifestEntry};

pub(crate) fn manifest_entry_fingerprint(entry: &AssetManifestEntry) -> Result<String, AssetError> {
    let fingerprint = serde_json::json!({
        "asset_id": entry.asset_id.to_string(),
        "asset_type": entry.asset_type,
        "importer": entry.importer,
        "cooker": entry.cooker,
        "version": entry.version,
        "source_path": entry.source_path,
        "cooked_path": entry.cooked_path,
        "dependencies": entry
            .dependencies
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        "import_settings": entry.import_settings,
    });
    let bytes = serde_json::to_vec(&fingerprint).map_err(|error| AssetError::Internal {
        message: format!("failed to serialize asset manifest fingerprint: {error}"),
    })?;
    Ok(hash_bytes(&bytes))
}

pub(crate) fn hash_bytes(bytes: &[u8]) -> String {
    const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;
    let mut hash = FNV_OFFSET_BASIS;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    format!("{hash:016x}")
}

pub(super) fn normalize_dependencies(dependencies: Vec<AssetId>) -> Vec<AssetId> {
    let mut unique = Vec::with_capacity(dependencies.len());
    for dependency in dependencies {
        if !unique.contains(&dependency) {
            unique.push(dependency);
        }
    }
    unique
}
