use std::collections::VecDeque;
use std::error::Error;
use std::fmt;
use std::path::Path;
use std::time::Duration;

use rustc_hash::FxHashMap;

use crate::asset::{AssetError, AssetServer, AssetState, Handle};
use crate::audio::{
    AudioBusId, AudioError, AudioInstanceId, AudioPlaybackSettings, AudioServer, AudioTween,
    MusicTrack, SoundClip,
};
use crate::ecs::World;
use crate::vn::audio::{VnAudioIntent, VnAudioVolumes, VnBgmState, VnSfxEvent, VnVoiceState};
use crate::vn::resource::VnResource;

#[derive(Clone, Debug, Default)]
pub struct VnAudioBindings {
    bgm: Option<VnBoundAudioInstance>,
    voice: Option<VnBoundAudioInstance>,
    pending: VecDeque<VnAudioIntent>,
    music: FxHashMap<String, Handle<MusicTrack>>,
    sounds: FxHashMap<String, Handle<SoundClip>>,
}

impl VnAudioBindings {
    #[must_use]
    pub fn current_bgm(&self) -> Option<&VnBoundAudioInstance> {
        self.bgm.as_ref()
    }

    #[must_use]
    pub fn current_voice(&self) -> Option<&VnBoundAudioInstance> {
        self.voice.as_ref()
    }

    #[must_use]
    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }

    #[must_use]
    pub fn is_idle(&self) -> bool {
        self.pending.is_empty()
    }

    pub fn queue_intents(&mut self, intents: impl IntoIterator<Item = VnAudioIntent>) {
        for intent in intents {
            self.queue_intent(intent);
        }
    }

    pub fn sync(
        &mut self,
        assets: &AssetServer,
        audio: &AudioServer,
        volumes: &VnAudioVolumes,
        intents: impl IntoIterator<Item = VnAudioIntent>,
    ) -> Result<VnAudioSyncReport, VnAudioSystemError> {
        self.queue_intents(intents);

        let mut report = VnAudioSyncReport::default();
        let attempts = self.pending.len();
        for _ in 0..attempts {
            let Some(intent) = self.pending.pop_front() else {
                break;
            };
            match self.try_apply_intent(assets, audio, volumes, &intent)? {
                VnAudioApplyStatus::Applied => report.applied += 1,
                VnAudioApplyStatus::Pending => {
                    self.pending.push_back(intent);
                    report.pending += 1;
                }
            }
        }
        report.pending = self.pending.len();
        Ok(report)
    }

    fn queue_intent(&mut self, intent: VnAudioIntent) {
        match &intent {
            VnAudioIntent::PlayBgm(_) => {
                self.pending.retain(|pending| {
                    !matches!(
                        pending,
                        VnAudioIntent::PlayBgm(_) | VnAudioIntent::StopBgm { .. }
                    )
                });
            }
            VnAudioIntent::StopBgm { .. } => {
                self.pending
                    .retain(|pending| !matches!(pending, VnAudioIntent::PlayBgm(_)));
            }
            VnAudioIntent::PlayVoice(_) => {
                self.pending.retain(|pending| {
                    !matches!(
                        pending,
                        VnAudioIntent::PlayVoice(_) | VnAudioIntent::StopVoice
                    )
                });
            }
            VnAudioIntent::StopVoice => {
                self.pending
                    .retain(|pending| !matches!(pending, VnAudioIntent::PlayVoice(_)));
            }
            VnAudioIntent::PlaySfx(_) => {}
        }
        self.pending.push_back(intent);
    }

    fn try_apply_intent(
        &mut self,
        assets: &AssetServer,
        audio: &AudioServer,
        volumes: &VnAudioVolumes,
        intent: &VnAudioIntent,
    ) -> Result<VnAudioApplyStatus, VnAudioSystemError> {
        match intent {
            VnAudioIntent::PlayBgm(bgm) => self.play_bgm(assets, audio, volumes, bgm),
            VnAudioIntent::StopBgm { fade } => {
                self.stop_bgm(audio, *fade)?;
                Ok(VnAudioApplyStatus::Applied)
            }
            VnAudioIntent::PlaySfx(event) => self.play_sfx(assets, audio, volumes, event),
            VnAudioIntent::PlayVoice(voice) => self.play_voice(assets, audio, volumes, voice),
            VnAudioIntent::StopVoice => {
                self.stop_voice(audio)?;
                Ok(VnAudioApplyStatus::Applied)
            }
        }
    }

    fn play_bgm(
        &mut self,
        assets: &AssetServer,
        audio: &AudioServer,
        volumes: &VnAudioVolumes,
        bgm: &VnBgmState,
    ) -> Result<VnAudioApplyStatus, VnAudioSystemError> {
        let handle = self.music_handle(assets, &bgm.asset)?;
        if !asset_ready(assets, &bgm.asset, handle)? {
            return Ok(VnAudioApplyStatus::Pending);
        }

        let gain = bgm.volume * volumes.master * volumes.bgm;
        let tween = tween_seconds(bgm.fade);
        if let Some(current) = &self.bgm {
            if current.asset == bgm.asset {
                audio
                    .set_gain(current.instance, gain, tween)
                    .map_err(|source| VnAudioSystemError::audio("set bgm gain", source))?;
                return Ok(VnAudioApplyStatus::Applied);
            }
            audio
                .stop(current.instance, tween)
                .map_err(|source| VnAudioSystemError::audio("stop previous bgm", source))?;
        }

        let instance = audio
            .play_music(
                handle,
                AudioPlaybackSettings::default()
                    .on_bus(AudioBusId::MUSIC)
                    .gain(gain)
                    .looped(bgm.looping),
            )
            .map_err(|source| VnAudioSystemError::audio("play bgm", source))?;
        self.bgm = Some(VnBoundAudioInstance {
            asset: bgm.asset.clone(),
            instance,
        });
        Ok(VnAudioApplyStatus::Applied)
    }

    fn stop_bgm(&mut self, audio: &AudioServer, fade: f32) -> Result<(), VnAudioSystemError> {
        if let Some(current) = self.bgm.take() {
            audio
                .stop(current.instance, tween_seconds(fade))
                .map_err(|source| VnAudioSystemError::audio("stop bgm", source))?;
        }
        Ok(())
    }

    fn play_sfx(
        &mut self,
        assets: &AssetServer,
        audio: &AudioServer,
        volumes: &VnAudioVolumes,
        event: &VnSfxEvent,
    ) -> Result<VnAudioApplyStatus, VnAudioSystemError> {
        let handle = self.sound_handle(assets, &event.asset)?;
        if !asset_ready(assets, &event.asset, handle)? {
            return Ok(VnAudioApplyStatus::Pending);
        }

        let bus = audio
            .bus_id(&event.bus)
            .ok_or_else(|| VnAudioSystemError::UnknownBus(event.bus.clone()))?;
        audio
            .play_sound(
                handle,
                AudioPlaybackSettings::default()
                    .on_bus(bus)
                    .gain(event.volume * volumes.master * volumes.sfx),
            )
            .map_err(|source| VnAudioSystemError::audio("play sfx", source))?;
        Ok(VnAudioApplyStatus::Applied)
    }

    fn play_voice(
        &mut self,
        assets: &AssetServer,
        audio: &AudioServer,
        volumes: &VnAudioVolumes,
        voice: &VnVoiceState,
    ) -> Result<VnAudioApplyStatus, VnAudioSystemError> {
        let handle = self.sound_handle(assets, &voice.asset)?;
        if !asset_ready(assets, &voice.asset, handle)? {
            return Ok(VnAudioApplyStatus::Pending);
        }

        if let Some(current) = self.voice.take() {
            audio
                .stop(current.instance, AudioTween::default())
                .map_err(|source| VnAudioSystemError::audio("stop previous voice", source))?;
        }

        let instance = audio
            .play_sound(
                handle,
                AudioPlaybackSettings::default()
                    .on_bus(AudioBusId::VOICE)
                    .gain(voice.volume * volumes.master * volumes.voice),
            )
            .map_err(|source| VnAudioSystemError::audio("play voice", source))?;
        self.voice = Some(VnBoundAudioInstance {
            asset: voice.asset.clone(),
            instance,
        });
        Ok(VnAudioApplyStatus::Applied)
    }

    fn stop_voice(&mut self, audio: &AudioServer) -> Result<(), VnAudioSystemError> {
        if let Some(current) = self.voice.take() {
            audio
                .stop(current.instance, AudioTween::default())
                .map_err(|source| VnAudioSystemError::audio("stop voice", source))?;
        }
        Ok(())
    }

    fn music_handle(
        &mut self,
        assets: &AssetServer,
        path: &str,
    ) -> Result<Handle<MusicTrack>, VnAudioSystemError> {
        if let Some(handle) = self.music.get(path) {
            return Ok(*handle);
        }
        let handle = assets
            .load_by_path::<MusicTrack>(Path::new(path))
            .map_err(|source| VnAudioSystemError::asset(path, source))?;
        self.music.insert(path.to_owned(), handle);
        Ok(handle)
    }

    fn sound_handle(
        &mut self,
        assets: &AssetServer,
        path: &str,
    ) -> Result<Handle<SoundClip>, VnAudioSystemError> {
        if let Some(handle) = self.sounds.get(path) {
            return Ok(*handle);
        }
        let handle = assets
            .load_by_path::<SoundClip>(Path::new(path))
            .map_err(|source| VnAudioSystemError::asset(path, source))?;
        self.sounds.insert(path.to_owned(), handle);
        Ok(handle)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VnBoundAudioInstance {
    pub asset: String,
    pub instance: AudioInstanceId,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VnAudioSyncReport {
    pub applied: usize,
    pub pending: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VnAudioSystemError {
    MissingAssetServer,
    MissingAudioServer,
    UnknownBus(String),
    Asset {
        asset: String,
        source: AssetError,
    },
    Audio {
        action: &'static str,
        source: AudioError,
    },
}

impl VnAudioSystemError {
    fn asset(asset: &str, source: AssetError) -> Self {
        Self::Asset {
            asset: asset.to_owned(),
            source,
        }
    }

    fn audio(action: &'static str, source: AudioError) -> Self {
        Self::Audio { action, source }
    }
}

impl fmt::Display for VnAudioSystemError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingAssetServer => {
                f.write_str("VN audio sync requires an AssetServer resource")
            }
            Self::MissingAudioServer => {
                f.write_str("VN audio sync requires an AudioServer resource")
            }
            Self::UnknownBus(bus) => write!(f, "VN audio bus `{bus}` is not registered"),
            Self::Asset { asset, source } => {
                write!(
                    f,
                    "VN audio asset `{asset}` could not be resolved: {source}"
                )
            }
            Self::Audio { action, source } => {
                write!(f, "VN audio action `{action}` failed: {source}")
            }
        }
    }
}

impl Error for VnAudioSystemError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Asset { source, .. } => Some(source),
            Self::Audio { source, .. } => Some(source),
            _ => None,
        }
    }
}

pub fn sync_runtime_audio_to_world(
    world: &mut World,
) -> Result<VnAudioSyncReport, VnAudioSystemError> {
    if !world.contains_resource::<VnResource>() {
        return Ok(VnAudioSyncReport::default());
    }

    let assets = world
        .get_resource::<AssetServer>()
        .cloned()
        .ok_or(VnAudioSystemError::MissingAssetServer)?;
    let audio = world
        .get_resource::<AudioServer>()
        .cloned()
        .ok_or(VnAudioSystemError::MissingAudioServer)?;

    let Some(vn) = world.get_resource_mut::<VnResource>() else {
        return Ok(VnAudioSyncReport::default());
    };
    let Some(runtime) = vn.runtime_mut() else {
        return Ok(VnAudioSyncReport::default());
    };
    let audio_state = runtime.audio_mut();
    let volumes = audio_state.volumes.clone();
    let intents = audio_state.drain_intents().collect::<Vec<_>>();
    vn.audio_bindings.sync(&assets, &audio, &volumes, intents)
}

pub fn sync_audio_intents_to_world(
    world: &mut World,
    volumes: &VnAudioVolumes,
    intents: impl IntoIterator<Item = VnAudioIntent>,
) -> Result<VnAudioSyncReport, VnAudioSystemError> {
    let assets = world
        .get_resource::<AssetServer>()
        .cloned()
        .ok_or(VnAudioSystemError::MissingAssetServer)?;
    let audio = world
        .get_resource::<AudioServer>()
        .cloned()
        .ok_or(VnAudioSystemError::MissingAudioServer)?;
    let Some(vn) = world.get_resource_mut::<VnResource>() else {
        return Ok(VnAudioSyncReport::default());
    };
    vn.audio_bindings.sync(&assets, &audio, volumes, intents)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VnAudioApplyStatus {
    Applied,
    Pending,
}

fn asset_ready<T: crate::asset::Asset>(
    assets: &AssetServer,
    asset: &str,
    handle: Handle<T>,
) -> Result<bool, VnAudioSystemError> {
    match assets.state(&handle) {
        AssetState::Installed => Ok(true),
        AssetState::Failed => {
            let source = assets
                .error(&handle)
                .unwrap_or(AssetError::AssetNotInstalled {
                    id: handle.id(),
                    state: AssetState::Failed,
                });
            Err(VnAudioSystemError::Asset {
                asset: asset.to_owned(),
                source,
            })
        }
        _ => Ok(false),
    }
}

fn tween_seconds(seconds: f32) -> AudioTween {
    if seconds <= 0.0 {
        AudioTween::default()
    } else {
        AudioTween::new(Duration::from_secs_f32(seconds))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latest_bgm_replaces_older_pending_bgm() {
        let mut bindings = VnAudioBindings::default();
        bindings.queue_intents([
            VnAudioIntent::PlayBgm(VnBgmState {
                asset: "audio/old.ogg".to_owned(),
                looping: true,
                volume: 1.0,
                fade: 0.0,
            }),
            VnAudioIntent::PlayBgm(VnBgmState {
                asset: "audio/new.ogg".to_owned(),
                looping: true,
                volume: 1.0,
                fade: 0.0,
            }),
        ]);

        assert_eq!(bindings.pending_len(), 1);
        assert!(matches!(
            bindings.pending.front(),
            Some(VnAudioIntent::PlayBgm(bgm)) if bgm.asset == "audio/new.ogg"
        ));
    }

    #[test]
    fn stop_voice_cancels_pending_voice() {
        let mut bindings = VnAudioBindings::default();
        bindings.queue_intents([
            VnAudioIntent::PlayVoice(VnVoiceState {
                speaker: Some("alice".to_owned()),
                asset: "voice/a.ogg".to_owned(),
                volume: 1.0,
            }),
            VnAudioIntent::StopVoice,
        ]);

        assert_eq!(bindings.pending_len(), 1);
        assert!(matches!(
            bindings.pending.front(),
            Some(VnAudioIntent::StopVoice)
        ));
    }
}
