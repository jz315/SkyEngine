use serde::{Deserialize, Serialize};

use crate::vn::script::{VnValue, YarnCommand};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VnAudioState {
    #[serde(default)]
    pub volumes: VnAudioVolumes,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bgm: Option<VnBgmState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice: Option<VnVoiceState>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sfx_events: Vec<VnSfxEvent>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub intents: Vec<VnAudioIntent>,
}

impl Default for VnAudioState {
    fn default() -> Self {
        Self {
            volumes: VnAudioVolumes::default(),
            bgm: None,
            voice: None,
            sfx_events: Vec::new(),
            intents: Vec::new(),
        }
    }
}

impl VnAudioState {
    pub fn apply_command(&mut self, command: &YarnCommand) -> Option<VnAudioIntent> {
        let intent = match command.name.as_str() {
            "play_bgm" => {
                let asset = command.first_positional_raw()?.to_owned();
                let bgm = VnBgmState {
                    asset,
                    looping: named_bool(command, "loop").unwrap_or(true),
                    volume: named_f32(command, "volume").unwrap_or(1.0),
                    fade: named_f32(command, "fade").unwrap_or(0.0),
                };
                self.bgm = Some(bgm.clone());
                VnAudioIntent::PlayBgm(bgm)
            }
            "stop_bgm" => {
                let fade = named_f32(command, "fade").unwrap_or(0.0);
                self.bgm = None;
                VnAudioIntent::StopBgm { fade }
            }
            "play_se" | "play_sfx" => {
                let asset = command.first_positional_raw()?.to_owned();
                let event = VnSfxEvent {
                    asset,
                    volume: named_f32(command, "volume").unwrap_or(1.0),
                    bus: command
                        .named_arg("bus")
                        .map(|arg| arg.raw.clone())
                        .unwrap_or_else(|| "sfx".to_owned()),
                };
                self.sfx_events.push(event.clone());
                VnAudioIntent::PlaySfx(event)
            }
            "voice" => {
                let mut args = command.positional_args();
                let first = args.next()?.raw.clone();
                let second = args.next().map(|arg| arg.raw.clone());
                let (speaker, asset) = match second {
                    Some(asset) => (Some(first), asset),
                    None => (None, first),
                };
                let voice = VnVoiceState {
                    speaker,
                    asset,
                    volume: named_f32(command, "volume").unwrap_or(1.0),
                };
                self.voice = Some(voice.clone());
                VnAudioIntent::PlayVoice(voice)
            }
            "stop_voice" => {
                self.voice = None;
                VnAudioIntent::StopVoice
            }
            _ => return None,
        };
        self.intents.push(intent.clone());
        Some(intent)
    }

    pub fn drain_intents(&mut self) -> impl Iterator<Item = VnAudioIntent> + '_ {
        self.intents.drain(..)
    }

    pub fn drain_sfx_events(&mut self) -> impl Iterator<Item = VnSfxEvent> + '_ {
        self.sfx_events.drain(..)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VnAudioVolumes {
    pub master: f32,
    pub bgm: f32,
    pub sfx: f32,
    pub voice: f32,
    pub ui: f32,
}

impl Default for VnAudioVolumes {
    fn default() -> Self {
        Self {
            master: 1.0,
            bgm: 1.0,
            sfx: 1.0,
            voice: 1.0,
            ui: 1.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum VnAudioIntent {
    PlayBgm(VnBgmState),
    StopBgm { fade: f32 },
    PlaySfx(VnSfxEvent),
    PlayVoice(VnVoiceState),
    StopVoice,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VnBgmState {
    pub asset: String,
    pub looping: bool,
    pub volume: f32,
    pub fade: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VnSfxEvent {
    pub asset: String,
    pub volume: f32,
    pub bus: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VnVoiceState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speaker: Option<String>,
    pub asset: String,
    pub volume: f32,
}

fn named_f32(command: &YarnCommand, name: &str) -> Option<f32> {
    match &command.named_arg(name)?.value {
        VnValue::Number(value) => Some(*value as f32),
        VnValue::String(value) => value.parse().ok(),
        VnValue::Bool(_) => None,
    }
}

fn named_bool(command: &YarnCommand, name: &str) -> Option<bool> {
    match &command.named_arg(name)?.value {
        VnValue::Bool(value) => Some(*value),
        VnValue::String(value) => value.parse().ok(),
        VnValue::Number(value) => Some(*value != 0.0),
    }
}
