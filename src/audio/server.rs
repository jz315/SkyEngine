use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::asset::{AssetServer, Handle};
#[cfg(feature = "app")]
use crate::ecs::EntityId;

use super::assets::{register_audio_asset_factories, MusicTrack, SoundClip};
use super::backend::{AudioBackend, BusDefinition};
use super::commands::{AudioCommand, AudioCommands};
#[cfg(feature = "app")]
use super::types::AudioEmitterAsset;
use super::types::{
    AudioBusId, AudioConfig, AudioError, AudioInstanceId, AudioPlaybackSettings, AudioTween,
};

#[derive(Clone)]
pub struct AudioServer {
    inner: Arc<Mutex<AudioServerInner>>,
    commands: AudioCommands,
}

impl std::fmt::Debug for AudioServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioServer").finish_non_exhaustive()
    }
}

impl AudioServer {
    pub fn new(config: AudioConfig, assets: AssetServer) -> Self {
        register_audio_asset_factories(&assets);

        let next_instance = Arc::new(AtomicU64::new(1));
        let buses = build_bus_definitions(&config);
        let bus_names = build_bus_name_map(&buses);
        let commands = AudioCommands::new(next_instance.clone());

        Self {
            inner: Arc::new(Mutex::new(AudioServerInner {
                assets,
                backend: AudioBackend::new(&config, &buses),
                config,
                buses,
                bus_names,
                next_instance,
                #[cfg(feature = "app")]
                emitter_instances: HashMap::default(),
            })),
            commands,
        }
    }

    #[must_use]
    pub fn commands(&self) -> AudioCommands {
        self.commands.clone()
    }

    #[must_use]
    pub fn is_available(&self) -> bool {
        self.inner
            .lock()
            .expect("audio server mutex poisoned")
            .backend
            .is_available()
    }

    #[must_use]
    pub fn disabled_reason(&self) -> Option<String> {
        self.inner
            .lock()
            .expect("audio server mutex poisoned")
            .backend
            .disabled_reason()
            .map(ToOwned::to_owned)
    }

    #[must_use]
    pub fn bus_id(&self, name: &str) -> Option<AudioBusId> {
        self.inner
            .lock()
            .expect("audio server mutex poisoned")
            .bus_names
            .get(&name.to_ascii_lowercase())
            .copied()
    }

    pub fn play_sound(
        &self,
        asset: Handle<SoundClip>,
        settings: AudioPlaybackSettings,
    ) -> Result<AudioInstanceId, AudioError> {
        let clip = {
            let inner = self.inner.lock().expect("audio server mutex poisoned");
            inner.assets.get(&asset)?
        };
        let mut inner = self.inner.lock().expect("audio server mutex poisoned");
        let instance = inner.reserve_instance();
        inner.backend.play_sound(instance, &clip, settings)?;
        Ok(instance)
    }

    pub fn play_music(
        &self,
        asset: Handle<MusicTrack>,
        settings: AudioPlaybackSettings,
    ) -> Result<AudioInstanceId, AudioError> {
        let track = {
            let inner = self.inner.lock().expect("audio server mutex poisoned");
            inner.assets.get(&asset)?
        };
        let mut inner = self.inner.lock().expect("audio server mutex poisoned");
        let instance = inner.reserve_instance();
        inner.backend.play_music(instance, &track, settings)?;
        Ok(instance)
    }

    pub fn stop(&self, instance: AudioInstanceId, tween: AudioTween) -> Result<(), AudioError> {
        self.inner
            .lock()
            .expect("audio server mutex poisoned")
            .backend
            .stop(instance, tween)
    }

    pub fn pause(&self, instance: AudioInstanceId, tween: AudioTween) -> Result<(), AudioError> {
        self.inner
            .lock()
            .expect("audio server mutex poisoned")
            .backend
            .pause(instance, tween)
    }

    pub fn resume(&self, instance: AudioInstanceId, tween: AudioTween) -> Result<(), AudioError> {
        self.inner
            .lock()
            .expect("audio server mutex poisoned")
            .backend
            .resume(instance, tween)
    }

    pub fn set_gain(
        &self,
        instance: AudioInstanceId,
        gain: f32,
        tween: AudioTween,
    ) -> Result<(), AudioError> {
        self.inner
            .lock()
            .expect("audio server mutex poisoned")
            .backend
            .set_gain(instance, gain, tween)
    }

    pub fn set_pitch(
        &self,
        instance: AudioInstanceId,
        pitch: f32,
        tween: AudioTween,
    ) -> Result<(), AudioError> {
        self.inner
            .lock()
            .expect("audio server mutex poisoned")
            .backend
            .set_pitch(instance, pitch, tween)
    }

    pub fn set_pan(
        &self,
        instance: AudioInstanceId,
        pan: f32,
        tween: AudioTween,
    ) -> Result<(), AudioError> {
        self.inner
            .lock()
            .expect("audio server mutex poisoned")
            .backend
            .set_pan(instance, pan, tween)
    }

    pub fn set_bus_gain(
        &self,
        bus: AudioBusId,
        gain: f32,
        tween: AudioTween,
    ) -> Result<(), AudioError> {
        self.inner
            .lock()
            .expect("audio server mutex poisoned")
            .backend
            .set_bus_gain(bus, gain, tween)
    }

    pub fn set_listener_pose(&self, position: [f32; 2], rotation: f32) -> Result<(), AudioError> {
        self.inner
            .lock()
            .expect("audio server mutex poisoned")
            .backend
            .set_listener_pose(position, rotation)
    }

    pub fn update(&self) {
        self.inner
            .lock()
            .expect("audio server mutex poisoned")
            .backend
            .update();
    }

    pub fn apply_commands(&self) -> Result<(), AudioError> {
        let commands = self.commands.drain();
        if commands.is_empty() {
            return Ok(());
        }

        for command in commands {
            match command {
                AudioCommand::PlaySound {
                    instance,
                    asset,
                    settings,
                } => {
                    let clip = {
                        let inner = self.inner.lock().expect("audio server mutex poisoned");
                        inner.assets.get(&asset)?
                    };
                    self.inner
                        .lock()
                        .expect("audio server mutex poisoned")
                        .backend
                        .play_sound(instance, &clip, settings)?;
                }
                AudioCommand::PlayMusic {
                    instance,
                    asset,
                    settings,
                } => {
                    let track = {
                        let inner = self.inner.lock().expect("audio server mutex poisoned");
                        inner.assets.get(&asset)?
                    };
                    self.inner
                        .lock()
                        .expect("audio server mutex poisoned")
                        .backend
                        .play_music(instance, &track, settings)?;
                }
                AudioCommand::Stop { instance, tween } => self.stop(instance, tween)?,
                AudioCommand::Pause { instance, tween } => self.pause(instance, tween)?,
                AudioCommand::Resume { instance, tween } => self.resume(instance, tween)?,
                AudioCommand::SetGain {
                    instance,
                    gain,
                    tween,
                } => self.set_gain(instance, gain, tween)?,
                AudioCommand::SetPitch {
                    instance,
                    pitch,
                    tween,
                } => self.set_pitch(instance, pitch, tween)?,
                AudioCommand::SetPan {
                    instance,
                    pan,
                    tween,
                } => self.set_pan(instance, pan, tween)?,
                AudioCommand::SetBusGain { bus, gain, tween } => {
                    self.set_bus_gain(bus, gain, tween)?
                }
            }
        }

        Ok(())
    }

    #[cfg(feature = "app")]
    pub(crate) fn sync_emitter_binding(
        &self,
        entity: EntityId,
        asset: &AudioEmitterAsset,
        settings: AudioPlaybackSettings,
    ) -> Result<(), AudioError> {
        let mut inner = self.inner.lock().expect("audio server mutex poisoned");
        let restart = match inner.emitter_instances.get(&entity) {
            Some(binding) => !binding.matches(asset, settings.bus, settings.looped),
            None => true,
        };

        if !restart {
            if let Some(instance) = inner
                .emitter_instances
                .get(&entity)
                .map(|binding| binding.instance)
            {
                if let Some(spatial) = settings.spatial {
                    inner.backend.update_spatial_instance(instance, spatial)?;
                }
                inner
                    .backend
                    .set_gain(instance, settings.gain, AudioTween::default())?;
                inner
                    .backend
                    .set_pitch(instance, settings.pitch, AudioTween::default())?;
                return Ok(());
            }
        }

        if let Some(binding) = inner.emitter_instances.remove(&entity) {
            inner
                .backend
                .stop(binding.instance, AudioTween::default())?;
        }

        let instance = match asset {
            AudioEmitterAsset::Sound(handle) => {
                let clip = inner.assets.get(handle)?;
                let instance = inner.reserve_instance();
                inner.backend.play_sound(instance, &clip, settings)?;
                instance
            }
            AudioEmitterAsset::Music(handle) => {
                let track = inner.assets.get(handle)?;
                let instance = inner.reserve_instance();
                inner.backend.play_music(instance, &track, settings)?;
                instance
            }
        };

        inner.emitter_instances.insert(
            entity,
            EmitterBinding::new(instance, asset.clone(), settings.bus, settings.looped),
        );
        Ok(())
    }

    #[cfg(feature = "app")]
    pub(crate) fn stop_emitter_binding(&self, entity: EntityId) -> Result<(), AudioError> {
        let mut inner = self.inner.lock().expect("audio server mutex poisoned");
        if let Some(binding) = inner.emitter_instances.remove(&entity) {
            inner
                .backend
                .stop(binding.instance, AudioTween::default())?;
        }
        Ok(())
    }

    #[cfg(feature = "app")]
    pub(crate) fn finish_emitter_sync(
        &self,
        seen_entities: &std::collections::HashSet<EntityId>,
    ) -> Result<(), AudioError> {
        let stale: Vec<_> = {
            let inner = self.inner.lock().expect("audio server mutex poisoned");
            inner
                .emitter_instances
                .keys()
                .copied()
                .filter(|entity| !seen_entities.contains(entity))
                .collect()
        };

        for entity in stale {
            self.stop_emitter_binding(entity)?;
        }
        Ok(())
    }
}

struct AudioServerInner {
    assets: AssetServer,
    backend: AudioBackend,
    #[allow(dead_code)]
    config: AudioConfig,
    #[allow(dead_code)]
    buses: Vec<BusDefinition>,
    bus_names: HashMap<String, AudioBusId>,
    next_instance: Arc<AtomicU64>,
    #[cfg(feature = "app")]
    emitter_instances: HashMap<EntityId, EmitterBinding>,
}

impl AudioServerInner {
    fn reserve_instance(&self) -> AudioInstanceId {
        AudioInstanceId(self.next_instance.fetch_add(1, Ordering::Relaxed))
    }
}

#[cfg(feature = "app")]
#[derive(Clone)]
struct EmitterBinding {
    instance: AudioInstanceId,
    asset: AudioEmitterAsset,
    bus: AudioBusId,
    looped: bool,
}

#[cfg(feature = "app")]
impl EmitterBinding {
    fn new(
        instance: AudioInstanceId,
        asset: AudioEmitterAsset,
        bus: AudioBusId,
        looped: bool,
    ) -> Self {
        Self {
            instance,
            asset,
            bus,
            looped,
        }
    }

    fn matches(&self, asset: &AudioEmitterAsset, bus: AudioBusId, looped: bool) -> bool {
        self.bus == bus
            && self.looped == looped
            && match (&self.asset, asset) {
                (AudioEmitterAsset::Sound(a), AudioEmitterAsset::Sound(b)) => a == b,
                (AudioEmitterAsset::Music(a), AudioEmitterAsset::Music(b)) => a == b,
                _ => false,
            }
    }
}

fn build_bus_definitions(config: &AudioConfig) -> Vec<BusDefinition> {
    let mut buses = vec![
        BusDefinition {
            id: AudioBusId::MASTER,
            name: "master".to_string(),
        },
        BusDefinition {
            id: AudioBusId::MUSIC,
            name: "music".to_string(),
        },
        BusDefinition {
            id: AudioBusId::SFX,
            name: "sfx".to_string(),
        },
        BusDefinition {
            id: AudioBusId::VOICE,
            name: "voice".to_string(),
        },
    ];

    for (index, name) in config.extra_buses.iter().enumerate() {
        buses.push(BusDefinition {
            id: AudioBusId(4 + index as u32),
            name: name.clone(),
        });
    }

    buses
}

fn build_bus_name_map(buses: &[BusDefinition]) -> HashMap<String, AudioBusId> {
    let mut map = HashMap::default();
    for bus in buses {
        map.insert(bus.name.to_ascii_lowercase(), bus.id);
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_bus_topology_is_registered() {
        let assets = AssetServer::with_empty_manifest(crate::asset::AssetConfig::default());
        let audio = AudioServer::new(
            AudioConfig {
                enabled: false,
                ..Default::default()
            },
            assets,
        );

        assert_eq!(audio.bus_id("master"), Some(AudioBusId::MASTER));
        assert_eq!(audio.bus_id("music"), Some(AudioBusId::MUSIC));
        assert_eq!(audio.bus_id("sfx"), Some(AudioBusId::SFX));
        assert_eq!(audio.bus_id("voice"), Some(AudioBusId::VOICE));
    }
}
