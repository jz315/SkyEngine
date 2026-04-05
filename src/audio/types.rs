use std::fmt::{Display, Formatter};
use std::path::PathBuf;
use std::time::Duration;

use crate::asset::{AssetError, Handle};

use super::assets::{MusicTrack, SoundClip};

#[derive(Clone, Debug)]
pub struct AudioConfig {
    pub enabled: bool,
    pub extra_buses: Vec<String>,
    pub internal_buffer_size: usize,
}

impl AudioConfig {
    #[must_use]
    pub fn with_bus(mut self, name: impl Into<String>) -> Self {
        self.extra_buses.push(name.into());
        self
    }
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            extra_buses: Vec::new(),
            internal_buffer_size: 128,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AudioBusId(pub u32);

impl AudioBusId {
    pub const MASTER: Self = Self(0);
    pub const MUSIC: Self = Self(1);
    pub const SFX: Self = Self(2);
    pub const VOICE: Self = Self(3);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AudioInstanceId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AudioTween {
    pub duration: Duration,
}

impl AudioTween {
    #[must_use]
    pub fn new(duration: Duration) -> Self {
        Self { duration }
    }
}

impl Default for AudioTween {
    fn default() -> Self {
        Self {
            duration: Duration::ZERO,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AudioSpatialSettings {
    pub position: [f32; 2],
    pub min_distance: f32,
    pub max_distance: f32,
    pub attenuation: f32,
    pub spatialization_strength: f32,
}

impl AudioSpatialSettings {
    #[must_use]
    pub fn with_position(mut self, x: f32, y: f32) -> Self {
        self.position = [x, y];
        self
    }
}

impl Default for AudioSpatialSettings {
    fn default() -> Self {
        Self {
            position: [0.0, 0.0],
            min_distance: 1.0,
            max_distance: 100.0,
            attenuation: 1.0,
            spatialization_strength: 0.75,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AudioPlaybackSettings {
    pub bus: AudioBusId,
    pub gain: f32,
    pub pitch: f32,
    pub pan: f32,
    pub looped: bool,
    pub spatial: Option<AudioSpatialSettings>,
}

impl AudioPlaybackSettings {
    #[must_use]
    pub fn on_bus(mut self, bus: AudioBusId) -> Self {
        self.bus = bus;
        self
    }

    #[must_use]
    pub fn gain(mut self, gain: f32) -> Self {
        self.gain = gain;
        self
    }

    #[must_use]
    pub fn pitch(mut self, pitch: f32) -> Self {
        self.pitch = pitch;
        self
    }

    #[must_use]
    pub fn pan(mut self, pan: f32) -> Self {
        self.pan = pan;
        self
    }

    #[must_use]
    pub fn looped(mut self, looped: bool) -> Self {
        self.looped = looped;
        self
    }

    #[must_use]
    pub fn spatial(mut self, spatial: AudioSpatialSettings) -> Self {
        self.spatial = Some(spatial);
        self
    }
}

impl Default for AudioPlaybackSettings {
    fn default() -> Self {
        Self {
            bus: AudioBusId::SFX,
            gain: 1.0,
            pitch: 1.0,
            pan: 0.0,
            looped: false,
            spatial: None,
        }
    }
}

#[derive(Clone, Debug)]
pub enum AudioEmitterAsset {
    Sound(Handle<SoundClip>),
    Music(Handle<MusicTrack>),
}

#[derive(Clone, Debug)]
pub struct AudioEmitter2D {
    pub asset: AudioEmitterAsset,
    pub bus: AudioBusId,
    pub gain: f32,
    pub pitch: f32,
    pub looped: bool,
    pub autoplay: bool,
    pub spatial: AudioSpatialSettings,
    pub enabled: bool,
}

impl AudioEmitter2D {
    #[must_use]
    pub fn sound(asset: Handle<SoundClip>) -> Self {
        Self {
            asset: AudioEmitterAsset::Sound(asset),
            bus: AudioBusId::SFX,
            gain: 1.0,
            pitch: 1.0,
            looped: false,
            autoplay: true,
            spatial: AudioSpatialSettings::default(),
            enabled: true,
        }
    }

    #[must_use]
    pub fn music(asset: Handle<MusicTrack>) -> Self {
        Self {
            asset: AudioEmitterAsset::Music(asset),
            bus: AudioBusId::MUSIC,
            gain: 1.0,
            pitch: 1.0,
            looped: true,
            autoplay: true,
            spatial: AudioSpatialSettings::default(),
            enabled: true,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct AudioListener2D {
    pub enabled: bool,
}

impl Default for AudioListener2D {
    fn default() -> Self {
        Self { enabled: true }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AudioError {
    Asset(AssetError),
    BackendUnavailable {
        message: String,
    },
    DeviceInitializationFailed {
        message: String,
    },
    BusNotFound {
        bus: AudioBusId,
    },
    InstanceNotFound {
        instance: AudioInstanceId,
    },
    PlayFailed {
        message: String,
    },
    InvalidAudioData {
        path: Option<PathBuf>,
        message: String,
    },
}

impl From<AssetError> for AudioError {
    fn from(value: AssetError) -> Self {
        Self::Asset(value)
    }
}

impl Display for AudioError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Asset(error) => Display::fmt(error, f),
            Self::BackendUnavailable { message } => {
                write!(f, "Audio backend unavailable: {message}")
            }
            Self::DeviceInitializationFailed { message } => {
                write!(f, "Audio device initialization failed: {message}")
            }
            Self::BusNotFound { bus } => write!(f, "Audio bus {:?} not found", bus),
            Self::InstanceNotFound { instance } => {
                write!(f, "Audio instance {:?} not found", instance)
            }
            Self::PlayFailed { message } => write!(f, "Audio playback failed: {message}"),
            Self::InvalidAudioData { path, message } => match path {
                Some(path) => write!(f, "Invalid audio data at {:?}: {message}", path),
                None => write!(f, "Invalid audio data: {message}"),
            },
        }
    }
}

impl std::error::Error for AudioError {}
