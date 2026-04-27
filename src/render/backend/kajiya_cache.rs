use std::path::{Path, PathBuf};

pub(crate) fn vendor_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("crates/vendor/kajiya")
}

pub(crate) fn default_cache_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/sky-kajiya-cache")
}

pub(crate) fn configure_vfs(vendor_root: &Path, cache_dir: &Path) {
    ::kajiya::backend::file::set_standard_vfs_mount_points(vendor_root);
    ::kajiya::backend::set_vfs_mount_point("/cache", cache_dir);
}
