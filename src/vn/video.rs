use serde::{Deserialize, Serialize};

use crate::vn::script::{VnValue, YarnCommand};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct VnVideoState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active: Option<VnVideoPlayback>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub intents: Vec<VnVideoIntent>,
}

impl VnVideoState {
    pub fn apply_command(&mut self, command: &YarnCommand) -> Option<VnVideoIntent> {
        let intent = match command.name.as_str() {
            "play_video" => {
                let asset = command.first_positional_raw()?.to_owned();
                let playback = VnVideoPlayback {
                    asset,
                    looping: named_bool(command, "loop").unwrap_or(false),
                    layer: named_i32(command, "layer").unwrap_or(40),
                    time: named_f32(command, "time").unwrap_or(0.0),
                    paused: false,
                };
                self.active = Some(playback.clone());
                VnVideoIntent::Play(playback)
            }
            "stop_video" => {
                self.active = None;
                VnVideoIntent::Stop
            }
            "pause_video" => {
                if let Some(active) = &mut self.active {
                    active.paused = true;
                }
                VnVideoIntent::Pause
            }
            "resume_video" => {
                if let Some(active) = &mut self.active {
                    active.paused = false;
                }
                VnVideoIntent::Resume
            }
            "seek_video" => {
                let time = command
                    .positional_values()
                    .next()
                    .and_then(value_as_f32)
                    .or_else(|| named_f32(command, "time"))
                    .unwrap_or(0.0);
                if let Some(active) = &mut self.active {
                    active.time = time;
                }
                VnVideoIntent::Seek { time }
            }
            _ => return None,
        };
        self.intents.push(intent.clone());
        Some(intent)
    }

    pub fn drain_intents(&mut self) -> impl Iterator<Item = VnVideoIntent> + '_ {
        self.intents.drain(..)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum VnVideoIntent {
    Play(VnVideoPlayback),
    Stop,
    Pause,
    Resume,
    Seek { time: f32 },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VnVideoPlayback {
    pub asset: String,
    pub looping: bool,
    pub layer: i32,
    pub time: f32,
    pub paused: bool,
}

fn named_f32(command: &YarnCommand, name: &str) -> Option<f32> {
    command
        .named_arg(name)
        .and_then(|arg| value_as_f32(&arg.value))
}

fn named_i32(command: &YarnCommand, name: &str) -> Option<i32> {
    command.named_arg(name).and_then(|arg| match &arg.value {
        VnValue::Number(value) => Some(*value as i32),
        VnValue::String(value) => value.parse().ok(),
        VnValue::Bool(_) => None,
    })
}

fn named_bool(command: &YarnCommand, name: &str) -> Option<bool> {
    command.named_arg(name).and_then(|arg| match &arg.value {
        VnValue::Bool(value) => Some(*value),
        VnValue::String(value) => value.parse().ok(),
        VnValue::Number(value) => Some(*value != 0.0),
    })
}

fn value_as_f32(value: &VnValue) -> Option<f32> {
    match value {
        VnValue::Number(value) => Some(*value as f32),
        VnValue::String(value) => value.parse().ok(),
        VnValue::Bool(_) => None,
    }
}
