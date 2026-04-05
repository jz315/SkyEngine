use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::asset::Handle;

use super::assets::{MusicTrack, SoundClip};
use super::types::{AudioBusId, AudioInstanceId, AudioPlaybackSettings, AudioTween};

#[derive(Clone, Default)]
pub struct AudioCommands {
    queue: Arc<Mutex<Vec<AudioCommand>>>,
    next_instance: Arc<AtomicU64>,
}

impl AudioCommands {
    pub(crate) fn new(next_instance: Arc<AtomicU64>) -> Self {
        Self {
            queue: Arc::new(Mutex::new(Vec::new())),
            next_instance,
        }
    }

    pub fn play_sound(
        &self,
        asset: Handle<SoundClip>,
        settings: AudioPlaybackSettings,
    ) -> AudioInstanceId {
        let instance = self.reserve_instance();
        self.queue
            .lock()
            .expect("audio commands mutex poisoned")
            .push(AudioCommand::PlaySound {
                instance,
                asset,
                settings,
            });
        instance
    }

    pub fn play_music(
        &self,
        asset: Handle<MusicTrack>,
        settings: AudioPlaybackSettings,
    ) -> AudioInstanceId {
        let instance = self.reserve_instance();
        self.queue
            .lock()
            .expect("audio commands mutex poisoned")
            .push(AudioCommand::PlayMusic {
                instance,
                asset,
                settings,
            });
        instance
    }

    pub fn stop(&self, instance: AudioInstanceId, tween: AudioTween) {
        self.queue
            .lock()
            .expect("audio commands mutex poisoned")
            .push(AudioCommand::Stop { instance, tween });
    }

    pub fn pause(&self, instance: AudioInstanceId, tween: AudioTween) {
        self.queue
            .lock()
            .expect("audio commands mutex poisoned")
            .push(AudioCommand::Pause { instance, tween });
    }

    pub fn resume(&self, instance: AudioInstanceId, tween: AudioTween) {
        self.queue
            .lock()
            .expect("audio commands mutex poisoned")
            .push(AudioCommand::Resume { instance, tween });
    }

    pub fn set_gain(&self, instance: AudioInstanceId, gain: f32, tween: AudioTween) {
        self.queue
            .lock()
            .expect("audio commands mutex poisoned")
            .push(AudioCommand::SetGain {
                instance,
                gain,
                tween,
            });
    }

    pub fn set_pitch(&self, instance: AudioInstanceId, pitch: f32, tween: AudioTween) {
        self.queue
            .lock()
            .expect("audio commands mutex poisoned")
            .push(AudioCommand::SetPitch {
                instance,
                pitch,
                tween,
            });
    }

    pub fn set_pan(&self, instance: AudioInstanceId, pan: f32, tween: AudioTween) {
        self.queue
            .lock()
            .expect("audio commands mutex poisoned")
            .push(AudioCommand::SetPan {
                instance,
                pan,
                tween,
            });
    }

    pub fn set_bus_gain(&self, bus: AudioBusId, gain: f32, tween: AudioTween) {
        self.queue
            .lock()
            .expect("audio commands mutex poisoned")
            .push(AudioCommand::SetBusGain { bus, gain, tween });
    }

    pub(crate) fn drain(&self) -> Vec<AudioCommand> {
        let mut queue = self.queue.lock().expect("audio commands mutex poisoned");
        std::mem::take(&mut *queue)
    }

    fn reserve_instance(&self) -> AudioInstanceId {
        AudioInstanceId(self.next_instance.fetch_add(1, Ordering::Relaxed))
    }
}

#[derive(Clone, Debug)]
pub(crate) enum AudioCommand {
    PlaySound {
        instance: AudioInstanceId,
        asset: Handle<SoundClip>,
        settings: AudioPlaybackSettings,
    },
    PlayMusic {
        instance: AudioInstanceId,
        asset: Handle<MusicTrack>,
        settings: AudioPlaybackSettings,
    },
    Stop {
        instance: AudioInstanceId,
        tween: AudioTween,
    },
    Pause {
        instance: AudioInstanceId,
        tween: AudioTween,
    },
    Resume {
        instance: AudioInstanceId,
        tween: AudioTween,
    },
    SetGain {
        instance: AudioInstanceId,
        gain: f32,
        tween: AudioTween,
    },
    SetPitch {
        instance: AudioInstanceId,
        pitch: f32,
        tween: AudioTween,
    },
    SetPan {
        instance: AudioInstanceId,
        pan: f32,
        tween: AudioTween,
    },
    SetBusGain {
        bus: AudioBusId,
        gain: f32,
        tween: AudioTween,
    },
}
