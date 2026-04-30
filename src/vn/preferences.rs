use serde::{Deserialize, Serialize};

use crate::vn::audio::VnAudioVolumes;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VnPreferences {
    pub text_chars_per_second: f32,
    pub auto_advance_seconds: f32,
    pub skip_unread: bool,
    pub stop_voice_on_advance: bool,
    pub fullscreen: bool,
    #[serde(default)]
    pub audio: VnAudioVolumes,
}

impl Default for VnPreferences {
    fn default() -> Self {
        Self {
            text_chars_per_second: 42.0,
            auto_advance_seconds: 2.0,
            skip_unread: false,
            stop_voice_on_advance: true,
            fullscreen: false,
            audio: VnAudioVolumes::default(),
        }
    }
}
