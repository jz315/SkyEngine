use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::asset::{Asset, AssetEventKind, AssetId, AssetState};
use crate::asset::{AssetEvent, AssetEventCursor, Assets, Handle};
#[cfg(feature = "app")]
use crate::ecs::EntityId;

use super::assets::{MusicTrack, SoundClip};
use super::backend::{AudioBackend, BusDefinition};
use super::commands::{AudioCommand, AudioCommands};
#[cfg(feature = "app")]
use super::types::AudioEmitterAsset;
use super::types::{
    AudioBusId, AudioConfig, AudioError, AudioInstanceId, AudioPlaybackSettings, AudioServerStats,
    AudioTween,
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
    pub fn new(config: AudioConfig, assets: Assets) -> Self {
        let next_instance = Arc::new(AtomicU64::new(1));
        let buses = build_bus_definitions(&config);
        let bus_names = build_bus_name_map(&buses);
        let commands = AudioCommands::new(next_instance.clone());
        let asset_event_cursor = assets.event_cursor();

        Self {
            inner: Arc::new(Mutex::new(AudioServerInner {
                assets,
                asset_event_cursor,
                backend: AudioBackend::new(&config, &buses),
                config,
                buses,
                bus_names,
                next_instance,
                instance_sources: HashMap::default(),
                failed_play_requests: 0,
                last_play_failure: None,
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
            match inner.assets.get(&asset) {
                Ok(clip) => clip,
                Err(error) => {
                    let error = AudioError::from(error);
                    drop(inner);
                    self.inner
                        .lock()
                        .expect("audio server mutex poisoned")
                        .record_failed_play_request(&error);
                    return Err(error);
                }
            }
        };
        let mut inner = self.inner.lock().expect("audio server mutex poisoned");
        let instance = inner.reserve_instance();
        inner.play_sound_instance(instance, Some(asset.id()), &clip, settings)?;
        Ok(instance)
    }

    pub fn play_music(
        &self,
        asset: Handle<MusicTrack>,
        settings: AudioPlaybackSettings,
    ) -> Result<AudioInstanceId, AudioError> {
        let track = {
            let inner = self.inner.lock().expect("audio server mutex poisoned");
            match inner.assets.get(&asset) {
                Ok(track) => track,
                Err(error) => {
                    let error = AudioError::from(error);
                    drop(inner);
                    self.inner
                        .lock()
                        .expect("audio server mutex poisoned")
                        .record_failed_play_request(&error);
                    return Err(error);
                }
            }
        };
        let mut inner = self.inner.lock().expect("audio server mutex poisoned");
        let instance = inner.reserve_instance();
        inner.play_music_instance(instance, Some(asset.id()), &track, settings)?;
        Ok(instance)
    }

    pub fn stop(&self, instance: AudioInstanceId, tween: AudioTween) -> Result<(), AudioError> {
        let mut inner = self.inner.lock().expect("audio server mutex poisoned");
        inner.backend.stop(instance, tween)?;
        inner.instance_sources.remove(&instance);
        Ok(())
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
        let mut inner = self.inner.lock().expect("audio server mutex poisoned");
        inner.consume_asset_events();
        inner.backend.update();
        inner.prune_finished_direct_instances();
    }

    #[must_use]
    pub fn stats(&self) -> AudioServerStats {
        let inner = self.inner.lock().expect("audio server mutex poisoned");
        let backend = inner.backend.stats();
        AudioServerStats {
            backend_available: backend.available,
            disabled_reason: inner.backend.disabled_reason().map(ToOwned::to_owned),
            configured_buses: inner.buses.len(),
            backend_instances: backend.live_instances,
            backend_spatial_instances: backend.spatial_instances,
            direct_instances: inner.instance_sources.len(),
            emitter_instances: inner.emitter_instance_count(),
            failed_play_requests: inner.failed_play_requests,
            last_play_failure: inner.last_play_failure.clone(),
        }
    }

    #[cfg(feature = "app")]
    pub(crate) fn consume_asset_events(&self) {
        self.inner
            .lock()
            .expect("audio server mutex poisoned")
            .consume_asset_events();
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
                    let mut inner = self.inner.lock().expect("audio server mutex poisoned");
                    inner.play_sound_instance(instance, Some(asset.id()), &clip, settings)?;
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
                    let mut inner = self.inner.lock().expect("audio server mutex poisoned");
                    inner.play_music_instance(instance, Some(asset.id()), &track, settings)?;
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
                inner.play_sound_instance(instance, None, &clip, settings)?;
                instance
            }
            AudioEmitterAsset::Music(handle) => {
                let track = inner.assets.get(handle)?;
                let instance = inner.reserve_instance();
                inner.play_music_instance(instance, None, &track, settings)?;
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
    assets: Assets,
    asset_event_cursor: AssetEventCursor,
    backend: AudioBackend,
    #[allow(dead_code)]
    config: AudioConfig,
    #[allow(dead_code)]
    buses: Vec<BusDefinition>,
    bus_names: HashMap<String, AudioBusId>,
    next_instance: Arc<AtomicU64>,
    instance_sources: HashMap<AudioInstanceId, AudioAssetSource>,
    failed_play_requests: u64,
    last_play_failure: Option<String>,
    #[cfg(feature = "app")]
    emitter_instances: HashMap<EntityId, EmitterBinding>,
}

impl AudioServerInner {
    fn reserve_instance(&self) -> AudioInstanceId {
        AudioInstanceId(self.next_instance.fetch_add(1, Ordering::Relaxed))
    }

    fn record_failed_play_request(&mut self, error: &AudioError) {
        self.failed_play_requests = self.failed_play_requests.saturating_add(1);
        self.last_play_failure = Some(error.to_string());
    }

    fn play_sound_instance(
        &mut self,
        instance: AudioInstanceId,
        direct_asset_id: Option<AssetId>,
        clip: &SoundClip,
        settings: AudioPlaybackSettings,
    ) -> Result<(), AudioError> {
        match self.backend.play_sound(instance, clip, settings) {
            Ok(()) => {
                if let Some(asset_id) = direct_asset_id {
                    self.instance_sources
                        .insert(instance, AudioAssetSource::Sound(asset_id));
                }
                Ok(())
            }
            Err(error) => {
                self.record_failed_play_request(&error);
                Err(error)
            }
        }
    }

    fn play_music_instance(
        &mut self,
        instance: AudioInstanceId,
        direct_asset_id: Option<AssetId>,
        track: &MusicTrack,
        settings: AudioPlaybackSettings,
    ) -> Result<(), AudioError> {
        match self.backend.play_music(instance, track, settings) {
            Ok(()) => {
                if let Some(asset_id) = direct_asset_id {
                    self.instance_sources
                        .insert(instance, AudioAssetSource::Music(asset_id));
                }
                Ok(())
            }
            Err(error) => {
                self.record_failed_play_request(&error);
                Err(error)
            }
        }
    }

    fn consume_asset_events(&mut self) {
        let events = self.assets.events_since(&mut self.asset_event_cursor);
        for event in &events {
            self.handle_asset_event(event);
        }
    }

    fn handle_asset_event(&mut self, event: &AssetEvent) {
        if !event_can_affect_audio(event) {
            return;
        }

        match event.kind {
            AssetEventKind::Loaded => {}
            AssetEventKind::Installed | AssetEventKind::Reloaded => {
                self.invalidate_emitter_bindings_for_event(event);
            }
            AssetEventKind::ReloadQueued => {}
            AssetEventKind::Unloaded => {
                self.stop_direct_instances_for_event(event);
                self.invalidate_emitter_bindings_for_event(event);
            }
            AssetEventKind::Failed if event.state == AssetState::Installed => {}
            AssetEventKind::Failed => {
                self.stop_direct_instances_for_event(event);
                self.invalidate_emitter_bindings_for_event(event);
            }
        }
    }

    fn stop_direct_instances_for_event(&mut self, event: &AssetEvent) {
        let stale = self
            .instance_sources
            .iter()
            .filter_map(|(instance, source)| source.matches_event(event).then_some(*instance))
            .collect::<Vec<_>>();

        for instance in stale {
            self.instance_sources.remove(&instance);
            let _ = self.backend.stop(instance, AudioTween::default());
        }
    }

    fn prune_finished_direct_instances(&mut self) {
        let active = self
            .instance_sources
            .keys()
            .copied()
            .filter(|instance| self.backend.contains_instance(*instance))
            .collect::<std::collections::HashSet<_>>();
        self.instance_sources
            .retain(|instance, _| active.contains(instance));
    }

    #[cfg(feature = "app")]
    fn invalidate_emitter_bindings_for_event(&mut self, event: &AssetEvent) {
        let stale = self
            .emitter_instances
            .iter()
            .filter_map(|(entity, binding)| binding.matches_event(event).then_some(*entity))
            .collect::<Vec<_>>();

        for entity in stale {
            if let Some(binding) = self.emitter_instances.remove(&entity) {
                let _ = self.backend.stop(binding.instance, AudioTween::default());
            }
        }
    }

    #[cfg(not(feature = "app"))]
    fn invalidate_emitter_bindings_for_event(&mut self, _event: &AssetEvent) {}

    #[cfg(feature = "app")]
    fn emitter_instance_count(&self) -> usize {
        self.emitter_instances.len()
    }

    #[cfg(not(feature = "app"))]
    fn emitter_instance_count(&self) -> usize {
        0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AudioAssetSource {
    Sound(AssetId),
    Music(AssetId),
}

impl AudioAssetSource {
    fn matches_event(self, event: &AssetEvent) -> bool {
        match self {
            Self::Sound(id) => {
                id == event.id
                    && (event.asset_type.is_empty() || event.asset_type == SoundClip::TYPE)
            }
            Self::Music(id) => {
                id == event.id
                    && (event.asset_type.is_empty() || event.asset_type == MusicTrack::TYPE)
            }
        }
    }
}

fn event_can_affect_audio(event: &AssetEvent) -> bool {
    event.asset_type.is_empty()
        || event.asset_type == SoundClip::TYPE
        || event.asset_type == MusicTrack::TYPE
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

    fn matches_event(&self, event: &AssetEvent) -> bool {
        match &self.asset {
            AudioEmitterAsset::Sound(handle) => {
                AudioAssetSource::Sound(handle.id()).matches_event(event)
            }
            AudioEmitterAsset::Music(handle) => {
                AudioAssetSource::Music(handle.id()).matches_event(event)
            }
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
    use std::sync::Arc;

    #[cfg(feature = "app")]
    use crate::asset::AssetConfig;
    #[cfg(feature = "app")]
    use crate::ecs::World;

    use super::*;

    fn audio_event(id: AssetId, asset_type: &'static str, kind: AssetEventKind) -> AssetEvent {
        AssetEvent {
            sequence: 0,
            id,
            kind,
            state: match kind {
                AssetEventKind::Unloaded => AssetState::Unloaded,
                AssetEventKind::Failed => AssetState::Failed,
                AssetEventKind::ReloadQueued => AssetState::Loading,
                AssetEventKind::Loaded => AssetState::Loaded,
                AssetEventKind::Installed | AssetEventKind::Reloaded => AssetState::Installed,
            },
            generation: 0,
            asset_type: asset_type.to_string(),
            failure_phase: None,
            manifest_fingerprint: None,
            content_hash: None,
            dependencies: Vec::new(),
            reload_pending: false,
        }
    }

    #[test]
    fn default_bus_topology_is_registered() {
        let assets = Assets::with_empty_manifest(crate::asset::AssetConfig::default());
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

    #[test]
    fn stats_report_backend_local_audio_tracking() {
        let assets = Assets::with_empty_manifest(crate::asset::AssetConfig::default());
        let track = assets.insert_runtime(MusicTrack {
            bytes: Arc::<[u8]>::from(Vec::<u8>::new()),
        });
        let audio = AudioServer::new(
            AudioConfig {
                enabled: false,
                extra_buses: vec!["ambience".to_string()],
                ..Default::default()
            },
            assets,
        );

        {
            let mut inner = audio.inner.lock().expect("audio server mutex poisoned");
            inner
                .instance_sources
                .insert(AudioInstanceId(11), AudioAssetSource::Music(track.id()));

            #[cfg(feature = "app")]
            inner.emitter_instances.insert(
                EntityId::new(3, 0),
                EmitterBinding::new(
                    AudioInstanceId(12),
                    AudioEmitterAsset::Music(track.clone()),
                    AudioBusId::MUSIC,
                    true,
                ),
            );
        }

        let stats = audio.stats();
        assert!(!stats.backend_available);
        assert_eq!(
            stats.disabled_reason.as_deref(),
            Some("audio explicitly disabled in config")
        );
        assert_eq!(stats.configured_buses, 5);
        assert_eq!(stats.backend_instances, 0);
        assert_eq!(stats.backend_spatial_instances, 0);
        assert_eq!(stats.direct_instances, 1);
        assert_eq!(stats.failed_play_requests, 0);
        assert_eq!(stats.last_play_failure, None);
        #[cfg(feature = "app")]
        assert_eq!(stats.emitter_instances, 1);
        #[cfg(not(feature = "app"))]
        assert_eq!(stats.emitter_instances, 0);
    }

    #[test]
    fn stats_count_failed_audio_play_requests() {
        let assets = Assets::with_empty_manifest(crate::asset::AssetConfig::default());
        let track = assets.insert_runtime(MusicTrack {
            bytes: Arc::<[u8]>::from(Vec::<u8>::new()),
        });
        let audio = AudioServer::new(
            AudioConfig {
                enabled: false,
                ..Default::default()
            },
            assets,
        );

        let error = audio
            .play_music(track, AudioPlaybackSettings::default())
            .expect_err("disabled backend should reject play request");

        assert!(matches!(error, AudioError::BackendUnavailable { .. }));
        let stats = audio.stats();
        assert_eq!(stats.failed_play_requests, 1);
        assert_eq!(stats.direct_instances, 0);
        assert!(stats
            .last_play_failure
            .as_deref()
            .is_some_and(|message| message.contains("Audio backend unavailable")));
    }

    #[test]
    fn installed_audio_event_keeps_direct_playback_tracking() {
        let assets = Assets::with_empty_manifest(crate::asset::AssetConfig::default());
        let track = assets.insert_runtime(MusicTrack {
            bytes: Arc::<[u8]>::from(Vec::<u8>::new()),
        });
        let audio = AudioServer::new(
            AudioConfig {
                enabled: false,
                ..Default::default()
            },
            assets,
        );
        let instance = AudioInstanceId(9);

        let mut inner = audio.inner.lock().expect("audio server mutex poisoned");
        inner
            .instance_sources
            .insert(instance, AudioAssetSource::Music(track.id()));
        inner.handle_asset_event(&audio_event(
            track.id(),
            MusicTrack::TYPE,
            AssetEventKind::Installed,
        ));

        assert_eq!(
            inner.instance_sources.get(&instance),
            Some(&AudioAssetSource::Music(track.id()))
        );
    }

    #[test]
    fn unloaded_audio_event_forgets_direct_playback_tracking() {
        let assets = Assets::with_empty_manifest(crate::asset::AssetConfig::default());
        let clip_id = AssetId::new();
        let audio = AudioServer::new(
            AudioConfig {
                enabled: false,
                ..Default::default()
            },
            assets,
        );
        let instance = AudioInstanceId(10);

        let mut inner = audio.inner.lock().expect("audio server mutex poisoned");
        inner
            .instance_sources
            .insert(instance, AudioAssetSource::Sound(clip_id));
        inner.handle_asset_event(&audio_event(
            clip_id,
            SoundClip::TYPE,
            AssetEventKind::Unloaded,
        ));

        assert!(!inner.instance_sources.contains_key(&instance));
    }

    #[cfg(feature = "app")]
    #[test]
    fn installed_audio_event_invalidates_matching_emitter_binding() {
        #[derive(Clone, Copy)]
        struct Marker;

        let assets = Assets::with_empty_manifest(AssetConfig::default());
        let track = assets.insert_runtime(MusicTrack {
            bytes: Arc::<[u8]>::from(Vec::<u8>::new()),
        });
        let audio = AudioServer::new(
            AudioConfig {
                enabled: false,
                ..Default::default()
            },
            assets.clone(),
        );
        let mut world = World::new();
        let entity = world.spawn((Marker,));

        {
            let mut inner = audio.inner.lock().expect("audio server mutex poisoned");
            inner.emitter_instances.insert(
                entity,
                EmitterBinding::new(
                    AudioInstanceId(7),
                    AudioEmitterAsset::Music(track.clone()),
                    AudioBusId::MUSIC,
                    true,
                ),
            );
        }

        assets
            .replace_runtime(
                &track,
                MusicTrack {
                    bytes: Arc::<[u8]>::from(vec![1]),
                },
            )
            .unwrap();
        audio.consume_asset_events();

        let inner = audio.inner.lock().expect("audio server mutex poisoned");
        assert!(inner.emitter_instances.is_empty());
    }
}
