mod common;

use sky_engine::asset::{
    Asset, AssetError, AssetInstallContext, AssetInstallResult, AssetLoadContext,
    AssetRuntimeFactory, Assets, LoadedAsset,
};

#[derive(Clone, Debug)]
struct DialogueLine {
    speaker: String,
    text: String,
}

impl Asset for DialogueLine {
    const TYPE: &'static str = "example.dialogue_line";
}

#[derive(Clone, Debug)]
struct LoadedDialogueLine {
    speaker: String,
    text: String,
}

struct DialogueLineFactory;

impl AssetRuntimeFactory for DialogueLineFactory {
    type Asset = DialogueLine;
    type Loaded = LoadedDialogueLine;

    fn load(&self, ctx: AssetLoadContext<'_>) -> Result<LoadedAsset<Self::Loaded>, AssetError> {
        let text =
            std::str::from_utf8(ctx.bytes).map_err(|error| AssetError::InvalidCookedAsset {
                id: Some(ctx.asset_id),
                message: error.to_string(),
            })?;
        let (speaker, line) =
            text.split_once(':')
                .ok_or_else(|| AssetError::InvalidCookedAsset {
                    id: Some(ctx.asset_id),
                    message: "dialogue line must use `speaker: text`".to_string(),
                })?;
        Ok(LoadedAsset::new(LoadedDialogueLine {
            speaker: speaker.trim().to_string(),
            text: line.trim().to_string(),
        }))
    }

    fn begin_install(
        &self,
        loaded: &Self::Loaded,
        _ctx: AssetInstallContext<'_>,
    ) -> Result<AssetInstallResult<Self::Asset>, AssetError> {
        Ok(AssetInstallResult::Ready(DialogueLine {
            speaker: loaded.speaker.clone(),
            text: loaded.text.clone(),
        }))
    }
}

fn main() -> common::ExampleResult<()> {
    let (_temp, config) = common::temp_config()?;
    let line_id = sky_engine::asset::AssetId::new();

    common::write_cooked_file(&config, "dialogue/intro.line", b"Nia: Welcome to SkyEngine")?;
    common::write_manifest_entries(
        &config,
        vec![common::manifest_entry(
            line_id,
            DialogueLine::TYPE,
            "dialogue/intro.dialogue",
            "dialogue/intro.line",
            Vec::new(),
        )],
    )?;

    let assets = Assets::new(config)?;
    assets.register_factory(DialogueLineFactory);
    let handle = assets.load_id::<DialogueLine>(line_id)?;
    let line = common::wait_until_ready(&assets, &handle)?;

    println!("{}: {}", line.speaker, line.text);
    Ok(())
}
