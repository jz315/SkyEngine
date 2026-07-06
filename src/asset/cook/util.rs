use crate::asset::types::AssetId;

pub(super) fn normalize_dependencies(dependencies: Vec<AssetId>) -> Vec<AssetId> {
    let mut unique = Vec::with_capacity(dependencies.len());
    for dependency in dependencies {
        if !unique.contains(&dependency) {
            unique.push(dependency);
        }
    }
    unique
}

pub(super) fn hash_bytes(bytes: &[u8]) -> String {
    const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;

    let mut hash = FNV_OFFSET_BASIS;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    format!("{hash:016x}")
}
