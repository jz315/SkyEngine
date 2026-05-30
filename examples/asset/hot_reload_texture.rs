mod common;

use std::time::Duration;

use sky_engine::asset::{cook, AssetPath, Assets, TextureAsset};

fn main() -> common::ExampleResult<()> {
    let (_temp, config) = common::temp_config()?;
    let source = config.asset_root.join("sprites/reload.png");
    let path = AssetPath::<TextureAsset>::new("sprites/reload.png");

    common::write_png(&source, [220, 32, 32, 255])?;
    let original_meta = cook::import_path(&config.asset_root, &source)?;
    cook::cook_all(&config)?;

    let assets = Assets::new(config.clone())?;
    let handle = assets.load_path(&path)?;
    let original = common::wait_until_ready(&assets, &handle)?;
    let original_pixel = original.pixels()[0..4].to_vec();
    drop(original);

    std::thread::sleep(Duration::from_millis(20));
    common::write_png(&source, [32, 96, 240, 255])?;
    let updated_meta = cook::import_path(&config.asset_root, &source)?;
    assert_eq!(original_meta.asset_id, updated_meta.asset_id);
    cook::cook_all(&config)?;

    assets.reload_manifest()?;
    let report = assets.reload_changed_with_report()?;
    let reloaded = common::wait_until_ready(&assets, &handle)?;
    let reloaded_pixel = reloaded.pixels()[0..4].to_vec();

    assert_ne!(original_pixel, reloaded_pixel);
    println!(
        "hot reload roots: {}, impacted: {}, skipped: {}",
        report.changed_roots.len(),
        report.impacted.len(),
        report.skipped.len()
    );
    Ok(())
}
