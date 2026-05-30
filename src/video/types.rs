use std::fmt::{Display, Formatter};

use crate::asset::{AssetError, Handle};
use crate::video::assets::VideoClip;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VideoInstanceId(pub u64);

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VideoServerStats {
    pub instances: usize,
    pub playing_instances: usize,
    pub paused_instances: usize,
    pub finished_instances: usize,
    pub stopped_instances: usize,
    pub distinct_clips: usize,
    pub current_frame_textures: usize,
    pub current_frame_texture_bytes: usize,
    pub failed_play_requests: u64,
    pub last_play_failure: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VideoPlaybackSettings {
    pub looped: bool,
    pub playback_rate: f32,
    pub start_paused: bool,
}

impl VideoPlaybackSettings {
    #[must_use]
    pub fn looped(mut self, looped: bool) -> Self {
        self.looped = looped;
        self
    }

    #[must_use]
    pub fn playback_rate(mut self, playback_rate: f32) -> Self {
        self.playback_rate = playback_rate;
        self
    }

    #[must_use]
    pub fn start_paused(mut self, start_paused: bool) -> Self {
        self.start_paused = start_paused;
        self
    }
}

impl Default for VideoPlaybackSettings {
    fn default() -> Self {
        Self {
            looped: false,
            playback_rate: 1.0,
            start_paused: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VideoPlaybackState {
    Playing,
    Paused,
    Finished,
    Stopped,
}

#[derive(Clone, Debug)]
pub struct VideoPlayer2D {
    pub clip: Handle<VideoClip>,
    pub settings: VideoPlaybackSettings,
    pub autoplay: bool,
    pub enabled: bool,
    pub instance: Option<VideoInstanceId>,
    pub last_frame_index: Option<usize>,
}

impl VideoPlayer2D {
    #[must_use]
    pub fn new(clip: Handle<VideoClip>) -> Self {
        Self {
            clip,
            settings: VideoPlaybackSettings::default(),
            autoplay: true,
            enabled: true,
            instance: None,
            last_frame_index: None,
        }
    }

    #[must_use]
    pub fn with_settings(mut self, settings: VideoPlaybackSettings) -> Self {
        self.settings = settings;
        self
    }

    #[must_use]
    pub fn autoplay(mut self, autoplay: bool) -> Self {
        self.autoplay = autoplay;
        self
    }

    #[must_use]
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum VideoError {
    Asset(AssetError),
    Texture(crate::render::expert::TextureError),
    InstanceNotFound { instance: VideoInstanceId },
    EmptyClip,
    InvalidFrameDuration,
    InvalidFrameDimensions { width: u32, height: u32 },
    InvalidFrameDataLength { expected: usize, actual: usize },
    InvalidFrameTimestamp { seconds: f64 },
    InvalidPlaybackRate { playback_rate: f32 },
    InvalidSeekTime { seconds: f64 },
    Decode { message: String },
    DecoderThreadPanic,
}

impl From<AssetError> for VideoError {
    fn from(value: AssetError) -> Self {
        Self::Asset(value)
    }
}

impl From<crate::render::expert::TextureError> for VideoError {
    fn from(value: crate::render::expert::TextureError) -> Self {
        Self::Texture(value)
    }
}

impl Display for VideoError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Asset(error) => Display::fmt(error, f),
            Self::Texture(error) => Display::fmt(error, f),
            Self::InstanceNotFound { instance } => {
                write!(f, "Video instance {:?} not found", instance)
            }
            Self::EmptyClip => write!(f, "Video clip has no frames"),
            Self::InvalidFrameDuration => {
                write!(f, "Video frame duration must be greater than zero")
            }
            Self::InvalidFrameDimensions { width, height } => {
                write!(f, "Video frame dimensions are invalid: {width}x{height}")
            }
            Self::InvalidFrameDataLength { expected, actual } => {
                write!(
                    f,
                    "Video RGBA frame length mismatch: expected {expected} bytes, got {actual}"
                )
            }
            Self::InvalidFrameTimestamp { seconds } => {
                write!(f, "Video frame timestamp must be finite (got {seconds})")
            }
            Self::InvalidPlaybackRate { playback_rate } => {
                write!(f, "Video playback rate must be finite and greater than zero (got {playback_rate})")
            }
            Self::InvalidSeekTime { seconds } => {
                write!(
                    f,
                    "Video seek time must be finite and non-negative (got {seconds})"
                )
            }
            Self::Decode { message } => write!(f, "Video decode failed: {message}"),
            Self::DecoderThreadPanic => write!(f, "Video decoder thread panicked"),
        }
    }
}

impl std::error::Error for VideoError {}
