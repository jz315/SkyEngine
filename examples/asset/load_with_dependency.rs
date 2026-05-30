mod common;

use sky_engine::asset::{
    Asset, AssetError, AssetId, AssetInstallContext, AssetInstallResult, AssetLoadContext,
    AssetRuntimeFactory, AssetState, Assets, LoadedAsset,
};

#[derive(Clone, Debug)]
struct PaletteAsset {
    name: String,
}

impl Asset for PaletteAsset {
    const TYPE: &'static str = "example.palette";
}

#[derive(Clone, Debug)]
struct SpriteDefinition {
    name: String,
    palette: AssetId,
}

impl Asset for SpriteDefinition {
    const TYPE: &'static str = "example.sprite_definition";
}

struct PaletteFactory;

impl AssetRuntimeFactory for PaletteFactory {
    type Asset = PaletteAsset;
    type Loaded = String;

    fn load(&self, ctx: AssetLoadContext<'_>) -> Result<LoadedAsset<Self::Loaded>, AssetError> {
        let name =
            std::str::from_utf8(ctx.bytes).map_err(|error| AssetError::InvalidCookedAsset {
                id: Some(ctx.asset_id),
                message: error.to_string(),
            })?;
        Ok(LoadedAsset::new(name.trim().to_string()))
    }

    fn begin_install(
        &self,
        loaded: &Self::Loaded,
        _ctx: AssetInstallContext<'_>,
    ) -> Result<AssetInstallResult<Self::Asset>, AssetError> {
        Ok(AssetInstallResult::Ready(PaletteAsset {
            name: loaded.clone(),
        }))
    }
}

#[derive(Clone, Debug)]
struct LoadedSpriteDefinition {
    name: String,
    palette: AssetId,
}

struct SpriteDefinitionFactory;

impl AssetRuntimeFactory for SpriteDefinitionFactory {
    type Asset = SpriteDefinition;
    type Loaded = LoadedSpriteDefinition;

    fn load(&self, ctx: AssetLoadContext<'_>) -> Result<LoadedAsset<Self::Loaded>, AssetError> {
        let text =
            std::str::from_utf8(ctx.bytes).map_err(|error| AssetError::InvalidCookedAsset {
                id: Some(ctx.asset_id),
                message: error.to_string(),
            })?;
        let mut name = None;
        let mut palette = None;
        for line in text.lines() {
            if let Some(value) = line.strip_prefix("name=") {
                name = Some(value.trim().to_string());
            } else if let Some(value) = line.strip_prefix("palette=") {
                palette = Some(AssetId::parse_str(value.trim())?);
            }
        }
        let palette = palette.ok_or_else(|| AssetError::InvalidCookedAsset {
            id: Some(ctx.asset_id),
            message: "sprite definition is missing `palette=<asset-id>`".to_string(),
        })?;
        let name = name.ok_or_else(|| AssetError::InvalidCookedAsset {
            id: Some(ctx.asset_id),
            message: "sprite definition is missing `name=<label>`".to_string(),
        })?;
        Ok(LoadedAsset::new(LoadedSpriteDefinition { name, palette })
            .with_dependencies(vec![palette]))
    }

    fn begin_install(
        &self,
        loaded: &Self::Loaded,
        _ctx: AssetInstallContext<'_>,
    ) -> Result<AssetInstallResult<Self::Asset>, AssetError> {
        Ok(AssetInstallResult::Ready(SpriteDefinition {
            name: loaded.name.clone(),
            palette: loaded.palette,
        }))
    }
}

fn main() -> common::ExampleResult<()> {
    let (_temp, config) = common::temp_config()?;
    let palette_id = AssetId::new();
    let sprite_id = AssetId::new();

    common::write_cooked_file(&config, "palettes/warm.palette", b"warm sunset")?;
    common::write_cooked_file(
        &config,
        "sprites/hero.sprite",
        format!("name=hero\npalette={palette_id}\n"),
    )?;
    common::write_manifest_entries(
        &config,
        vec![
            common::manifest_entry(
                palette_id,
                PaletteAsset::TYPE,
                "palettes/warm.palette",
                "palettes/warm.palette",
                Vec::new(),
            ),
            common::manifest_entry(
                sprite_id,
                SpriteDefinition::TYPE,
                "sprites/hero.sprite",
                "sprites/hero.sprite",
                Vec::new(),
            ),
        ],
    )?;

    let assets = Assets::new(config)?;
    assets.register_factory(PaletteFactory);
    assets.register_factory(SpriteDefinitionFactory);

    let sprite_handle = assets.load_id::<SpriteDefinition>(sprite_id)?;
    let sprite = common::wait_until_ready(&assets, &sprite_handle)?;
    let palette = assets.get_id::<PaletteAsset>(palette_id)?;

    assert_eq!(assets.state_untyped(palette_id), AssetState::Installed);
    println!(
        "loaded sprite `{}` with palette `{}` ({})",
        sprite.name, palette.name, sprite.palette
    );
    Ok(())
}
