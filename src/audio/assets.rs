use std::io::Cursor;
use std::sync::Arc;

use kira::sound::static_sound::StaticSoundData;
use kira::sound::streaming::StreamingSoundData;

use crate::asset::{
    Asset, AssetError, AssetInstallContext, AssetLoadContext, AssetRuntimeFactory, AssetServer,
    LoadedAsset,
};

#[derive(Clone, Debug)]
pub struct SoundClip {
    pub(crate) data: Arc<StaticSoundData>,
}

impl Asset for SoundClip {
    const TYPE: &'static str = "sound_clip";
}

#[derive(Clone, Debug)]
pub struct MusicTrack {
    pub(crate) bytes: Arc<[u8]>,
}

impl Asset for MusicTrack {
    const TYPE: &'static str = "music_track";
}

pub fn register_audio_asset_factories(asset_server: &AssetServer) {
    asset_server.register_factory(SoundClipFactory);
    asset_server.register_factory(MusicTrackFactory);
}

pub(crate) struct SoundClipFactory;

impl AssetRuntimeFactory for SoundClipFactory {
    type Asset = SoundClip;
    type Loaded = Arc<[u8]>;

    fn load(&self, ctx: AssetLoadContext<'_>) -> Result<LoadedAsset<Self::Loaded>, AssetError> {
        Ok(LoadedAsset::new(Arc::<[u8]>::from(ctx.bytes.to_vec()))
            .with_dependencies(ctx.entry.dependencies.clone()))
    }

    fn install(
        &self,
        loaded: &Self::Loaded,
        _ctx: AssetInstallContext<'_>,
    ) -> Result<Self::Asset, AssetError> {
        let data = StaticSoundData::from_cursor(Cursor::new(loaded.clone())).map_err(|error| {
            AssetError::InvalidCookedAsset {
                id: None,
                message: error.to_string(),
            }
        })?;
        Ok(SoundClip {
            data: Arc::new(data),
        })
    }
}

pub(crate) struct MusicTrackFactory;

impl AssetRuntimeFactory for MusicTrackFactory {
    type Asset = MusicTrack;
    type Loaded = Arc<[u8]>;

    fn load(&self, ctx: AssetLoadContext<'_>) -> Result<LoadedAsset<Self::Loaded>, AssetError> {
        Ok(LoadedAsset::new(Arc::<[u8]>::from(ctx.bytes.to_vec()))
            .with_dependencies(ctx.entry.dependencies.clone()))
    }

    fn install(
        &self,
        loaded: &Self::Loaded,
        _ctx: AssetInstallContext<'_>,
    ) -> Result<Self::Asset, AssetError> {
        StreamingSoundData::from_cursor(Cursor::new(loaded.clone())).map_err(|error| {
            AssetError::InvalidCookedAsset {
                id: None,
                message: error.to_string(),
            }
        })?;
        Ok(MusicTrack {
            bytes: loaded.clone(),
        })
    }
}
