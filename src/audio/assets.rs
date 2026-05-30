use std::io::Cursor;
use std::sync::Arc;

use kira::sound::static_sound::StaticSoundData;
use kira::sound::streaming::StreamingSoundData;

use crate::asset::{
    Asset, AssetCookedSchema, AssetError, AssetInstallBudget, AssetInstallContext,
    AssetInstallPoll, AssetInstallResult, AssetInstallTask, AssetLoadContext, AssetRequestProgress,
    AssetRuntimeFactory, Assets, LoadedAsset,
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

pub fn register_audio_asset_factories(asset_server: &Assets) {
    asset_server.register_factory(SoundClipFactory);
    asset_server.register_factory(MusicTrackFactory);
}

pub(crate) struct SoundClipFactory;

impl AssetRuntimeFactory for SoundClipFactory {
    type Asset = SoundClip;
    type Loaded = Arc<[u8]>;

    fn cooked_schema(&self) -> Option<AssetCookedSchema> {
        Some(AssetCookedSchema::new("audio.copy", 1))
    }

    fn load(&self, ctx: AssetLoadContext<'_>) -> Result<LoadedAsset<Self::Loaded>, AssetError> {
        Ok(LoadedAsset::new(Arc::<[u8]>::from(ctx.bytes.to_vec()))
            .with_dependencies(ctx.entry.dependencies.clone()))
    }

    fn begin_install(
        &self,
        loaded: &Self::Loaded,
        _ctx: AssetInstallContext<'_>,
    ) -> Result<AssetInstallResult<Self::Asset>, AssetError> {
        Ok(AssetInstallResult::Pending(Box::new(
            SoundClipInstallTask {
                bytes: loaded.clone(),
            },
        )))
    }
}

struct SoundClipInstallTask {
    bytes: Arc<[u8]>,
}

impl AssetInstallTask for SoundClipInstallTask {
    type Output = SoundClip;

    fn progress(&self) -> Option<AssetRequestProgress> {
        Some(AssetRequestProgress::new(3, 4, "decoding sound"))
    }

    fn poll_install(
        &mut self,
        _ctx: AssetInstallContext<'_>,
        budget: AssetInstallBudget,
    ) -> Result<AssetInstallPoll<Self::Output>, AssetError> {
        if budget.should_yield() {
            return Ok(AssetInstallPoll::Pending);
        }

        let data =
            StaticSoundData::from_cursor(Cursor::new(self.bytes.clone())).map_err(|error| {
                AssetError::InvalidCookedAsset {
                    id: None,
                    message: error.to_string(),
                }
            })?;
        Ok(AssetInstallPoll::Ready(SoundClip {
            data: Arc::new(data),
        }))
    }
}

pub(crate) struct MusicTrackFactory;

impl AssetRuntimeFactory for MusicTrackFactory {
    type Asset = MusicTrack;
    type Loaded = Arc<[u8]>;

    fn cooked_schema(&self) -> Option<AssetCookedSchema> {
        Some(AssetCookedSchema::new("audio.copy", 1))
    }

    fn load(&self, ctx: AssetLoadContext<'_>) -> Result<LoadedAsset<Self::Loaded>, AssetError> {
        Ok(LoadedAsset::new(Arc::<[u8]>::from(ctx.bytes.to_vec()))
            .with_dependencies(ctx.entry.dependencies.clone()))
    }

    fn begin_install(
        &self,
        loaded: &Self::Loaded,
        _ctx: AssetInstallContext<'_>,
    ) -> Result<AssetInstallResult<Self::Asset>, AssetError> {
        Ok(AssetInstallResult::Pending(Box::new(
            MusicTrackInstallTask {
                bytes: loaded.clone(),
            },
        )))
    }
}

struct MusicTrackInstallTask {
    bytes: Arc<[u8]>,
}

impl AssetInstallTask for MusicTrackInstallTask {
    type Output = MusicTrack;

    fn progress(&self) -> Option<AssetRequestProgress> {
        Some(AssetRequestProgress::new(3, 4, "validating music stream"))
    }

    fn poll_install(
        &mut self,
        _ctx: AssetInstallContext<'_>,
        budget: AssetInstallBudget,
    ) -> Result<AssetInstallPoll<Self::Output>, AssetError> {
        if budget.should_yield() {
            return Ok(AssetInstallPoll::Pending);
        }

        StreamingSoundData::from_cursor(Cursor::new(self.bytes.clone())).map_err(|error| {
            AssetError::InvalidCookedAsset {
                id: None,
                message: error.to_string(),
            }
        })?;
        Ok(AssetInstallPoll::Ready(MusicTrack {
            bytes: self.bytes.clone(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    use crate::asset::{
        AssetConfig, AssetId, AssetManifestEntry, AssetRegistryManifest, AssetState,
        ASSET_SYSTEM_VERSION,
    };
    use tempfile::tempdir;

    fn test_entry(id: AssetId, asset_type: &str) -> AssetManifestEntry {
        AssetManifestEntry {
            asset_id: id,
            asset_type: asset_type.to_string(),
            importer: "audio.importer".to_string(),
            cooker: "audio.copy".to_string(),
            version: 1,
            source_path: "clip.wav".to_string(),
            cooked_path: "clip.wav".to_string(),
            dependencies: Vec::new(),
            import_settings: serde_json::Value::Null,
        }
    }

    fn write_manifest(
        root: &Path,
        entry: AssetManifestEntry,
    ) -> Result<AssetConfig, Box<dyn std::error::Error>> {
        let config = AssetConfig::new(root, "native");
        std::fs::create_dir_all(config.cooked_root())?;
        let manifest = AssetRegistryManifest {
            version: ASSET_SYSTEM_VERSION,
            target: "native".to_string(),
            provenance: Vec::new(),
            assets: vec![entry],
        };
        std::fs::write(
            config.manifest_path(),
            serde_json::to_vec_pretty(&manifest)?,
        )?;
        Ok(config)
    }

    fn wav_bytes() -> Arc<[u8]> {
        let sample_rate = 8_000u32;
        let samples = [
            0i16, 11585, 16384, 11585, 0, -11585, -16384, -11585, 0, 11585, 16384, 11585,
        ];
        let mut data = Vec::with_capacity(samples.len() * 2);
        for sample in samples {
            data.extend_from_slice(&sample.to_le_bytes());
        }

        let data_len = data.len() as u32;
        let byte_rate = sample_rate * 2;
        let block_align = 2u16;
        let mut wav = Vec::with_capacity(44 + data.len());
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data_len).to_le_bytes());
        wav.extend_from_slice(b"WAVE");
        wav.extend_from_slice(b"fmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&sample_rate.to_le_bytes());
        wav.extend_from_slice(&byte_rate.to_le_bytes());
        wav.extend_from_slice(&block_align.to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data_len.to_le_bytes());
        wav.extend_from_slice(&data);
        Arc::from(wav)
    }

    #[test]
    fn sound_clip_install_is_deferred_and_budget_aware() {
        let bytes = wav_bytes();
        let id = AssetId::new();
        let entry = test_entry(id, SoundClip::TYPE);
        let result = SoundClipFactory
            .begin_install(&bytes, AssetInstallContext::new(id, &entry))
            .unwrap();
        let AssetInstallResult::Pending(mut task) = result else {
            panic!("sound clip install should defer decoding into an install task");
        };

        assert!(matches!(
            task.poll_install(
                AssetInstallContext::new(id, &entry),
                AssetInstallBudget::from_remaining_time(std::time::Duration::ZERO),
            )
            .unwrap(),
            AssetInstallPoll::Pending
        ));
        assert!(matches!(
            task.poll_install(
                AssetInstallContext::new(id, &entry),
                AssetInstallBudget::unlimited(),
            )
            .unwrap(),
            AssetInstallPoll::Ready(_)
        ));
    }

    #[test]
    fn music_track_install_is_deferred_and_validates_stream_data() {
        let bytes = wav_bytes();
        let id = AssetId::new();
        let entry = test_entry(id, MusicTrack::TYPE);
        let result = MusicTrackFactory
            .begin_install(&bytes, AssetInstallContext::new(id, &entry))
            .unwrap();
        let AssetInstallResult::Pending(mut task) = result else {
            panic!("music track install should defer validation into an install task");
        };

        assert!(matches!(
            task.poll_install(
                AssetInstallContext::new(id, &entry),
                AssetInstallBudget::unlimited(),
            )
            .unwrap(),
            AssetInstallPoll::Ready(_)
        ));
    }

    #[test]
    fn asset_server_advances_sound_clip_deferred_install_across_updates(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        let id = AssetId::new();
        let config = write_manifest(dir.path(), test_entry(id, SoundClip::TYPE))?;
        std::fs::write(config.cooked_root().join("clip.wav"), wav_bytes())?;

        let assets = Assets::new(config)?;
        register_audio_asset_factories(&assets);
        let handle = assets.load_id::<SoundClip>(id)?;

        assets.update()?;
        assert_eq!(assets.state(&handle), AssetState::Installing);
        assert!(assets.try_get(&handle).is_none());

        assets.update()?;
        assert_eq!(assets.state(&handle), AssetState::Installed);
        assert!(assets.try_get(&handle).is_some());
        Ok(())
    }
}
