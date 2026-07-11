#![allow(dead_code)]

use std::error::Error;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use sky_engine::asset::{
    Asset, AssetConfig, AssetId, AssetManifestEntry, AssetRegistryManifest, AssetState, Assets,
    Handle, ASSET_SYSTEM_VERSION,
};

pub type ExampleResult<T> = Result<T, Box<dyn Error>>;

pub fn temp_config() -> ExampleResult<(tempfile::TempDir, AssetConfig)> {
    let temp = tempfile::tempdir()?;
    let asset_root = temp.path().join("assets");
    std::fs::create_dir_all(&asset_root)?;
    Ok((temp, AssetConfig::new(&asset_root, "native")))
}

pub fn write_png(path: impl AsRef<Path>, color: [u8; 4]) -> ExampleResult<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let image = image::RgbaImage::from_pixel(2, 2, image::Rgba(color));
    image.save(path)?;
    Ok(())
}

pub fn manifest_entry(
    asset_id: AssetId,
    asset_type: &'static str,
    source_path: impl Into<String>,
    cooked_path: impl Into<String>,
    dependencies: Vec<AssetId>,
) -> AssetManifestEntry {
    AssetManifestEntry {
        asset_id,
        asset_type: asset_type.to_string(),
        importer: "example".to_string(),
        cooker: "example".to_string(),
        version: 1,
        source_path: source_path.into(),
        cooked_path: cooked_path.into(),
        dependencies,
        import_settings: serde_json::Value::Null,
    }
}

pub fn write_cooked_file(
    config: &AssetConfig,
    cooked_path: impl AsRef<Path>,
    bytes: impl AsRef<[u8]>,
) -> ExampleResult<()> {
    let path = config.cooked_root().join(cooked_path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, bytes)?;
    Ok(())
}

pub fn write_manifest_entries(
    config: &AssetConfig,
    entries: Vec<AssetManifestEntry>,
) -> ExampleResult<()> {
    std::fs::create_dir_all(config.cooked_root())?;
    let manifest = AssetRegistryManifest {
        version: ASSET_SYSTEM_VERSION,
        target: config.target.clone(),
        provenance: Vec::new(),
        assets: entries,
    };
    std::fs::write(
        config.manifest_path(),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    Ok(())
}

pub fn wait_until_ready<T: Asset>(assets: &Assets, handle: &Handle<T>) -> ExampleResult<Arc<T>> {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        assets.update()?;
        match assets.state(handle) {
            AssetState::Installed => return Ok(assets.get(handle)?),
            AssetState::Failed => {
                let message = match assets.error(handle) {
                    Some(error) => error.to_string(),
                    None => format!("asset {} failed without an error", handle.id()),
                };
                return Err(std::io::Error::other(message).into());
            }
            _ if Instant::now() >= deadline => {
                let message = format!(
                    "asset {} did not install before the example timeout",
                    handle.id()
                );
                return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, message).into());
            }
            _ => std::thread::sleep(Duration::from_millis(1)),
        }
    }
}
