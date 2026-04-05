use std::path::Path;

use sky_engine::asset::{cook, AssetConfig, AssetServer, TextureAsset};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    let asset_root = temp.path().join("assets");
    std::fs::create_dir_all(&asset_root)?;

    let source = asset_root.join("hero.png");
    write_png(&source)?;

    let config = AssetConfig::new(&asset_root, "native");
    let meta = cook::import_path(&asset_root, &source)?;
    let manifest = cook::cook_all(&config)?;
    let server = AssetServer::new(config)?;
    let texture = server.load_blocking::<TextureAsset>(meta.asset_id)?;

    println!("cooked assets: {}", manifest.assets.len());
    println!(
        "loaded texture {}x{} ({:?})",
        texture.width(),
        texture.height(),
        texture.color_space()
    );
    Ok(())
}

fn write_png(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let image = image::RgbaImage::from_raw(
        2,
        2,
        vec![
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
        ],
    )
    .expect("valid RGBA pixels");
    image.save(path)?;
    Ok(())
}
