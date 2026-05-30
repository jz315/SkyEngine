mod common;

use sky_engine::asset::{cook, AssetPath, Assets, TextureAsset};

fn main() -> common::ExampleResult<()> {
    let (_temp, config) = common::temp_config()?;
    let source = config.asset_root.join("sprites/player.png");
    common::write_png(&source, [255, 64, 32, 255])?;

    let _meta = cook::import_path(&config.asset_root, &source)?;
    let manifest = cook::cook_all(&config)?;

    let assets = Assets::new(config)?;
    let path = AssetPath::<TextureAsset>::new("sprites/player.png");
    let handle = assets.load_path(&path)?;
    let texture = common::wait_until_ready(&assets, &handle)?;

    println!("manifest assets: {}", manifest.assets.len());
    println!(
        "loaded {} as {}x{} {:?}",
        path,
        texture.width(),
        texture.height(),
        texture.color_space()
    );
    Ok(())
}
