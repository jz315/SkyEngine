use std::collections::HashMap;
use std::io::Cursor;

use kira::sound::static_sound::StaticSoundHandle;
use kira::sound::streaming::StreamingSoundHandle;
use kira::sound::{PlaybackState, Region};
use kira::track::{SpatialTrackBuilder, TrackBuilder, TrackHandle};
use kira::{AudioManager, AudioManagerSettings, Decibels, DefaultBackend, Panning, Tween};
use mint::{Quaternion, Vector3};

use crate::math;

use super::assets::{MusicTrack, SoundClip};
use super::types::{
    AudioBusId, AudioConfig, AudioError, AudioInstanceId, AudioPlaybackSettings,
    AudioSpatialSettings, AudioTween,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct AudioBackendStats {
    pub available: bool,
    pub live_instances: usize,
    pub spatial_instances: usize,
}

pub(crate) struct AudioBackend {
    state: BackendState,
}

impl AudioBackend {
    pub(crate) fn new(config: &AudioConfig, buses: &[BusDefinition]) -> Self {
        if !config.enabled {
            return Self {
                state: BackendState::Disabled {
                    reason: "audio explicitly disabled in config".to_string(),
                },
            };
        }

        match KiraBackend::new(config, buses) {
            Ok(backend) => Self {
                state: BackendState::Ready(backend),
            },
            Err(error) => Self {
                state: BackendState::Disabled {
                    reason: error.to_string(),
                },
            },
        }
    }

    #[must_use]
    pub(crate) fn is_available(&self) -> bool {
        matches!(self.state, BackendState::Ready(_))
    }

    #[must_use]
    pub(crate) fn disabled_reason(&self) -> Option<&str> {
        match &self.state {
            BackendState::Ready(_) => None,
            BackendState::Disabled { reason } => Some(reason.as_str()),
        }
    }

    pub(crate) fn play_sound(
        &mut self,
        instance: AudioInstanceId,
        clip: &SoundClip,
        settings: AudioPlaybackSettings,
    ) -> Result<(), AudioError> {
        match &mut self.state {
            BackendState::Ready(backend) => backend.play_sound(instance, clip, settings),
            BackendState::Disabled { reason } => Err(AudioError::BackendUnavailable {
                message: reason.clone(),
            }),
        }
    }

    pub(crate) fn play_music(
        &mut self,
        instance: AudioInstanceId,
        track: &MusicTrack,
        settings: AudioPlaybackSettings,
    ) -> Result<(), AudioError> {
        match &mut self.state {
            BackendState::Ready(backend) => backend.play_music(instance, track, settings),
            BackendState::Disabled { reason } => Err(AudioError::BackendUnavailable {
                message: reason.clone(),
            }),
        }
    }

    pub(crate) fn stop(
        &mut self,
        instance: AudioInstanceId,
        tween: AudioTween,
    ) -> Result<(), AudioError> {
        self.with_instance(instance, |record| {
            record.handle.stop(to_kira_tween(tween));
            Ok(())
        })
    }

    pub(crate) fn pause(
        &mut self,
        instance: AudioInstanceId,
        tween: AudioTween,
    ) -> Result<(), AudioError> {
        self.with_instance(instance, |record| {
            record.handle.pause(to_kira_tween(tween));
            Ok(())
        })
    }

    pub(crate) fn resume(
        &mut self,
        instance: AudioInstanceId,
        tween: AudioTween,
    ) -> Result<(), AudioError> {
        self.with_instance(instance, |record| {
            record.handle.resume(to_kira_tween(tween));
            Ok(())
        })
    }

    pub(crate) fn set_gain(
        &mut self,
        instance: AudioInstanceId,
        gain: f32,
        tween: AudioTween,
    ) -> Result<(), AudioError> {
        self.with_instance(instance, |record| {
            record.base_gain = gain.max(0.0);
            record.apply_volume(to_kira_tween(tween));
            Ok(())
        })
    }

    pub(crate) fn set_pitch(
        &mut self,
        instance: AudioInstanceId,
        pitch: f32,
        tween: AudioTween,
    ) -> Result<(), AudioError> {
        self.with_instance(instance, |record| {
            record.pitch = pitch.max(0.01);
            record
                .handle
                .set_playback_rate(record.pitch as f64, to_kira_tween(tween));
            Ok(())
        })
    }

    pub(crate) fn set_pan(
        &mut self,
        instance: AudioInstanceId,
        pan: f32,
        tween: AudioTween,
    ) -> Result<(), AudioError> {
        self.with_instance(instance, |record| {
            record.pan = pan.clamp(-1.0, 1.0);
            record.handle.set_panning(record.pan, to_kira_tween(tween));
            Ok(())
        })
    }

    pub(crate) fn set_bus_gain(
        &mut self,
        bus: AudioBusId,
        gain: f32,
        tween: AudioTween,
    ) -> Result<(), AudioError> {
        match &mut self.state {
            BackendState::Ready(backend) => backend.set_bus_gain(bus, gain, tween),
            BackendState::Disabled { reason } => Err(AudioError::BackendUnavailable {
                message: reason.clone(),
            }),
        }
    }

    pub(crate) fn set_listener_pose(
        &mut self,
        position: [f32; 2],
        rotation: f32,
    ) -> Result<(), AudioError> {
        match &mut self.state {
            BackendState::Ready(backend) => {
                backend.listener_position = position;
                backend.listener_rotation = rotation;
                backend
                    .listener
                    .set_position(vec3(position), Tween::default());
                backend
                    .listener
                    .set_orientation(quat_from_rotation(rotation), Tween::default());
                Ok(())
            }
            BackendState::Disabled { reason } => Err(AudioError::BackendUnavailable {
                message: reason.clone(),
            }),
        }
    }

    #[cfg(feature = "app")]
    pub(crate) fn update_spatial_instance(
        &mut self,
        instance: AudioInstanceId,
        spatial: AudioSpatialSettings,
    ) -> Result<(), AudioError> {
        match &mut self.state {
            BackendState::Ready(backend) => {
                let listener_position = backend.listener_position;
                let record = backend
                    .instances
                    .get_mut(&instance)
                    .ok_or(AudioError::InstanceNotFound { instance })?;
                let Some(spatial_state) = record.spatial.as_mut() else {
                    return Err(AudioError::PlayFailed {
                        message: format!("instance {:?} is not spatial", instance),
                    });
                };

                spatial_state.settings = spatial;
                spatial_state
                    .track
                    .set_position(vec3(spatial.position), Tween::default());
                spatial_state
                    .track
                    .set_spatialization_strength(spatial.spatialization_strength, Tween::default());

                record.spatial_gain = attenuation_gain(listener_position, &spatial);
                record.apply_volume(Tween::default());
                Ok(())
            }
            BackendState::Disabled { reason } => Err(AudioError::BackendUnavailable {
                message: reason.clone(),
            }),
        }
    }

    pub(crate) fn update(&mut self) {
        if let BackendState::Ready(backend) = &mut self.state {
            backend
                .instances
                .retain(|_, record| !record.handle.is_stopped());
        }
    }

    pub(crate) fn contains_instance(&self, instance: AudioInstanceId) -> bool {
        match &self.state {
            BackendState::Ready(backend) => backend.instances.contains_key(&instance),
            BackendState::Disabled { .. } => false,
        }
    }

    pub(crate) fn stats(&self) -> AudioBackendStats {
        match &self.state {
            BackendState::Ready(backend) => backend.stats(),
            BackendState::Disabled { .. } => AudioBackendStats::default(),
        }
    }

    fn with_instance(
        &mut self,
        instance: AudioInstanceId,
        f: impl FnOnce(&mut InstanceRecord) -> Result<(), AudioError>,
    ) -> Result<(), AudioError> {
        match &mut self.state {
            BackendState::Ready(backend) => {
                let record = backend
                    .instances
                    .get_mut(&instance)
                    .ok_or(AudioError::InstanceNotFound { instance })?;
                f(record)
            }
            BackendState::Disabled { reason } => Err(AudioError::BackendUnavailable {
                message: reason.clone(),
            }),
        }
    }
}

enum BackendState {
    Ready(KiraBackend),
    Disabled { reason: String },
}

pub(crate) struct BusDefinition {
    pub id: AudioBusId,
    pub name: String,
}

struct KiraBackend {
    manager: AudioManager<DefaultBackend>,
    buses: HashMap<AudioBusId, BusHandle>,
    listener: kira::listener::ListenerHandle,
    listener_position: [f32; 2],
    listener_rotation: f32,
    instances: HashMap<AudioInstanceId, InstanceRecord>,
}

impl KiraBackend {
    fn new(config: &AudioConfig, buses: &[BusDefinition]) -> Result<Self, AudioError> {
        let mut manager = AudioManager::<DefaultBackend>::new(AudioManagerSettings {
            internal_buffer_size: config.internal_buffer_size,
            ..Default::default()
        })
        .map_err(|error| AudioError::DeviceInitializationFailed {
            message: error.to_string(),
        })?;

        let mut bus_handles = HashMap::default();
        for bus in buses {
            if bus.id == AudioBusId::MASTER {
                continue;
            }
            let handle = manager
                .add_sub_track(TrackBuilder::new())
                .map_err(|error| AudioError::PlayFailed {
                    message: error.to_string(),
                })?;
            bus_handles.insert(bus.id, BusHandle::Track(handle));
        }

        let listener = manager
            .add_listener(vec3([0.0, 0.0]), quat_from_rotation(0.0))
            .map_err(|error| AudioError::PlayFailed {
                message: error.to_string(),
            })?;

        Ok(Self {
            manager,
            buses: bus_handles,
            listener,
            listener_position: [0.0, 0.0],
            listener_rotation: 0.0,
            instances: HashMap::default(),
        })
    }

    fn play_sound(
        &mut self,
        instance: AudioInstanceId,
        clip: &SoundClip,
        settings: AudioPlaybackSettings,
    ) -> Result<(), AudioError> {
        let data = configure_static_sound(clip, settings);
        let spatial = settings.spatial;
        let handle = if let Some(spatial) = spatial {
            let mut track = self.create_spatial_track(settings.bus, spatial)?;
            let handle = track.play(data).map_err(play_error_to_audio)?;
            let spatial_gain = attenuation_gain(self.listener_position, &spatial);
            self.instances.insert(
                instance,
                InstanceRecord::new(
                    InstanceHandle::Static(handle),
                    settings,
                    Some(SpatialState {
                        track,
                        settings: spatial,
                    }),
                    spatial_gain,
                ),
            );
            return Ok(());
        } else if settings.bus == AudioBusId::MASTER {
            self.manager.play(data).map_err(play_error_to_audio)?
        } else {
            self.track_bus_mut(settings.bus)?
                .play(data)
                .map_err(play_error_to_audio)?
        };

        self.instances.insert(
            instance,
            InstanceRecord::new(InstanceHandle::Static(handle), settings, None, 1.0),
        );
        Ok(())
    }

    fn play_music(
        &mut self,
        instance: AudioInstanceId,
        track: &MusicTrack,
        settings: AudioPlaybackSettings,
    ) -> Result<(), AudioError> {
        let data = configure_streaming_sound(track, settings)?;
        let spatial = settings.spatial;
        let handle = if let Some(spatial) = spatial {
            let mut track_handle = self.create_spatial_track(settings.bus, spatial)?;
            let handle = track_handle.play(data).map_err(play_error_to_audio)?;
            let spatial_gain = attenuation_gain(self.listener_position, &spatial);
            self.instances.insert(
                instance,
                InstanceRecord::new(
                    InstanceHandle::Streaming(handle),
                    settings,
                    Some(SpatialState {
                        track: track_handle,
                        settings: spatial,
                    }),
                    spatial_gain,
                ),
            );
            return Ok(());
        } else if settings.bus == AudioBusId::MASTER {
            self.manager.play(data).map_err(play_error_to_audio)?
        } else {
            self.track_bus_mut(settings.bus)?
                .play(data)
                .map_err(play_error_to_audio)?
        };

        self.instances.insert(
            instance,
            InstanceRecord::new(InstanceHandle::Streaming(handle), settings, None, 1.0),
        );
        Ok(())
    }

    fn set_bus_gain(
        &mut self,
        bus: AudioBusId,
        gain: f32,
        tween: AudioTween,
    ) -> Result<(), AudioError> {
        let db = gain_to_decibels(gain);
        let tween = to_kira_tween(tween);
        if bus == AudioBusId::MASTER {
            self.manager.main_track().set_volume(db, tween);
            return Ok(());
        }

        self.track_bus_mut(bus)?.set_volume(db, tween);
        Ok(())
    }

    fn create_spatial_track(
        &mut self,
        bus: AudioBusId,
        spatial: AudioSpatialSettings,
    ) -> Result<kira::track::SpatialTrackHandle, AudioError> {
        let builder = SpatialTrackBuilder::new()
            .distances((spatial.min_distance, spatial.max_distance))
            .attenuation_function(None)
            .spatialization_strength(spatial.spatialization_strength);

        if bus == AudioBusId::MASTER {
            let listener_id = self.listener.id();
            self.manager
                .add_spatial_sub_track(listener_id, vec3(spatial.position), builder)
                .map_err(|error| AudioError::PlayFailed {
                    message: error.to_string(),
                })
        } else {
            let listener_id = self.listener.id();
            self.track_bus_mut(bus)?
                .add_spatial_sub_track(listener_id, vec3(spatial.position), builder)
                .map_err(|error| AudioError::PlayFailed {
                    message: error.to_string(),
                })
        }
    }

    fn track_bus_mut(&mut self, bus: AudioBusId) -> Result<&mut TrackHandle, AudioError> {
        match self.buses.get_mut(&bus) {
            Some(BusHandle::Track(handle)) => Ok(handle),
            None => Err(AudioError::BusNotFound { bus }),
        }
    }

    fn stats(&self) -> AudioBackendStats {
        AudioBackendStats {
            available: true,
            live_instances: self.instances.len(),
            spatial_instances: spatial_instance_count(&self.instances),
        }
    }
}

#[cfg(feature = "app")]
fn spatial_instance_count(instances: &HashMap<AudioInstanceId, InstanceRecord>) -> usize {
    instances
        .values()
        .filter(|record| record.spatial.is_some())
        .count()
}

#[cfg(not(feature = "app"))]
fn spatial_instance_count(_instances: &HashMap<AudioInstanceId, InstanceRecord>) -> usize {
    0
}

enum BusHandle {
    Track(TrackHandle),
}

struct InstanceRecord {
    handle: InstanceHandle,
    base_gain: f32,
    pan: f32,
    pitch: f32,
    spatial_gain: f32,
    #[cfg(feature = "app")]
    spatial: Option<SpatialState>,
}

impl InstanceRecord {
    fn new(
        mut handle: InstanceHandle,
        settings: AudioPlaybackSettings,
        _spatial: Option<SpatialState>,
        spatial_gain: f32,
    ) -> Self {
        handle.set_playback_rate(settings.pitch.max(0.01) as f64, Tween::default());
        handle.set_panning(settings.pan.clamp(-1.0, 1.0), Tween::default());
        handle.set_volume(
            gain_to_decibels(settings.gain * spatial_gain),
            Tween::default(),
        );
        Self {
            handle,
            base_gain: settings.gain,
            pan: settings.pan,
            pitch: settings.pitch,
            spatial_gain,
            #[cfg(feature = "app")]
            spatial: _spatial,
        }
    }

    fn apply_volume(&mut self, tween: Tween) {
        let gain = (self.base_gain * self.spatial_gain).max(0.0);
        self.handle.set_volume(gain_to_decibels(gain), tween);
    }
}

#[allow(dead_code)]
struct SpatialState {
    track: kira::track::SpatialTrackHandle,
    settings: AudioSpatialSettings,
}

enum InstanceHandle {
    Static(StaticSoundHandle),
    Streaming(StreamingSoundHandle<kira::sound::FromFileError>),
}

impl InstanceHandle {
    fn set_volume(&mut self, volume: Decibels, tween: Tween) {
        match self {
            Self::Static(handle) => handle.set_volume(volume, tween),
            Self::Streaming(handle) => handle.set_volume(volume, tween),
        }
    }

    fn set_playback_rate(&mut self, rate: f64, tween: Tween) {
        match self {
            Self::Static(handle) => handle.set_playback_rate(rate, tween),
            Self::Streaming(handle) => handle.set_playback_rate(rate, tween),
        }
    }

    fn set_panning(&mut self, pan: f32, tween: Tween) {
        match self {
            Self::Static(handle) => handle.set_panning(Panning(pan), tween),
            Self::Streaming(handle) => handle.set_panning(Panning(pan), tween),
        }
    }

    fn pause(&mut self, tween: Tween) {
        match self {
            Self::Static(handle) => handle.pause(tween),
            Self::Streaming(handle) => handle.pause(tween),
        }
    }

    fn resume(&mut self, tween: Tween) {
        match self {
            Self::Static(handle) => handle.resume(tween),
            Self::Streaming(handle) => handle.resume(tween),
        }
    }

    fn stop(&mut self, tween: Tween) {
        match self {
            Self::Static(handle) => handle.stop(tween),
            Self::Streaming(handle) => handle.stop(tween),
        }
    }

    fn is_stopped(&self) -> bool {
        match self {
            Self::Static(handle) => handle.state() == PlaybackState::Stopped,
            Self::Streaming(handle) => handle.state() == PlaybackState::Stopped,
        }
    }
}

fn configure_static_sound(
    clip: &SoundClip,
    settings: AudioPlaybackSettings,
) -> kira::sound::static_sound::StaticSoundData {
    let data = clip.data.as_ref();
    let data = if settings.looped {
        data.loop_region(..)
    } else {
        data.clone()
    };
    data.volume(gain_to_decibels(settings.gain))
        .playback_rate(settings.pitch.max(0.01) as f64)
        .panning(Panning(settings.pan.clamp(-1.0, 1.0)))
}

fn configure_streaming_sound(
    track: &MusicTrack,
    settings: AudioPlaybackSettings,
) -> Result<kira::sound::streaming::StreamingSoundData<kira::sound::FromFileError>, AudioError> {
    let data =
        kira::sound::streaming::StreamingSoundData::from_cursor(Cursor::new(track.bytes.clone()))
            .map_err(|error| AudioError::InvalidAudioData {
            path: None,
            message: error.to_string(),
        })?;
    let data = if settings.looped {
        data.loop_region(Region::from(..))
    } else {
        data
    };
    Ok(data
        .volume(gain_to_decibels(settings.gain))
        .playback_rate(settings.pitch.max(0.01) as f64)
        .panning(Panning(settings.pan.clamp(-1.0, 1.0))))
}

fn gain_to_decibels(gain: f32) -> Decibels {
    if gain <= 0.0001 {
        Decibels::SILENCE
    } else {
        Decibels(20.0 * gain.log10())
    }
}

fn attenuation_gain(listener_position: [f32; 2], spatial: &AudioSpatialSettings) -> f32 {
    let dx = spatial.position[0] - listener_position[0];
    let dy = spatial.position[1] - listener_position[1];
    let distance = (dx * dx + dy * dy).sqrt();
    if distance <= spatial.min_distance {
        return 1.0;
    }
    if distance >= spatial.max_distance {
        return 0.0;
    }
    let span = (spatial.max_distance - spatial.min_distance).max(0.001);
    let t = 1.0 - ((distance - spatial.min_distance) / span);
    t.clamp(0.0, 1.0).powf(spatial.attenuation.max(0.01))
}

fn to_kira_tween(tween: AudioTween) -> Tween {
    Tween {
        duration: tween.duration,
        ..Default::default()
    }
}

fn vec3(position: [f32; 2]) -> Vector3<f32> {
    Vector3 {
        x: position[0],
        y: position[1],
        z: 0.0,
    }
}

fn quat_from_rotation(rotation: f32) -> Quaternion<f32> {
    let rotation = math::Quat::from_rotation_z(rotation);
    Quaternion {
        s: rotation.w(),
        v: Vector3 {
            x: rotation.x(),
            y: rotation.y(),
            z: rotation.z(),
        },
    }
}

fn play_error_to_audio<E>(error: kira::PlaySoundError<E>) -> AudioError {
    AudioError::PlayFailed {
        message: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attenuation_is_one_inside_min_distance() {
        let spatial = AudioSpatialSettings {
            min_distance: 2.0,
            max_distance: 10.0,
            attenuation: 2.0,
            ..Default::default()
        };
        assert_eq!(attenuation_gain([0.0, 0.0], &spatial), 1.0);
    }

    #[test]
    fn attenuation_reaches_zero_at_max_distance() {
        let spatial = AudioSpatialSettings {
            position: [10.0, 0.0],
            min_distance: 1.0,
            max_distance: 10.0,
            attenuation: 1.0,
            ..Default::default()
        };
        assert_eq!(attenuation_gain([0.0, 0.0], &spatial), 0.0);
    }

    #[test]
    fn disabled_backend_reports_unavailable() {
        let config = AudioConfig {
            enabled: false,
            ..Default::default()
        };
        let backend = AudioBackend::new(
            &config,
            &[
                BusDefinition {
                    id: AudioBusId::MASTER,
                    name: "master".to_string(),
                },
                BusDefinition {
                    id: AudioBusId::SFX,
                    name: "sfx".to_string(),
                },
            ],
        );
        assert!(!backend.is_available());
        assert!(backend.disabled_reason().is_some());
    }
}
