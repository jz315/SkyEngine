use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VnCommandRegistry {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    commands: BTreeMap<String, VnCommandDescriptor>,
}

impl Default for VnCommandRegistry {
    fn default() -> Self {
        let mut registry = Self {
            commands: BTreeMap::new(),
        };
        for (name, family) in [
            ("scene", VnCommandFamily::Scene),
            ("bg", VnCommandFamily::Scene),
            ("cg", VnCommandFamily::Scene),
            ("show", VnCommandFamily::Scene),
            ("hide", VnCommandFamily::Scene),
            ("move", VnCommandFamily::Scene),
            ("play_bgm", VnCommandFamily::Audio),
            ("stop_bgm", VnCommandFamily::Audio),
            ("play_se", VnCommandFamily::Audio),
            ("play_sfx", VnCommandFamily::Audio),
            ("voice", VnCommandFamily::Audio),
            ("stop_voice", VnCommandFamily::Audio),
            ("play_video", VnCommandFamily::Video),
            ("stop_video", VnCommandFamily::Video),
            ("pause_video", VnCommandFamily::Video),
            ("resume_video", VnCommandFamily::Video),
            ("seek_video", VnCommandFamily::Video),
            ("wait", VnCommandFamily::Runtime),
            ("checkpoint", VnCommandFamily::Runtime),
            ("preload", VnCommandFamily::Asset),
            ("release", VnCommandFamily::Asset),
            ("notify", VnCommandFamily::Ui),
            ("unlock_cg", VnCommandFamily::Runtime),
        ] {
            registry.register(VnCommandDescriptor {
                name: name.to_owned(),
                family,
                passthrough: false,
            });
        }
        registry
    }
}

impl VnCommandRegistry {
    pub fn register(&mut self, descriptor: VnCommandDescriptor) -> Option<VnCommandDescriptor> {
        self.commands.insert(descriptor.name.clone(), descriptor)
    }

    pub fn get(&self, name: &str) -> Option<&VnCommandDescriptor> {
        self.commands.get(name)
    }

    pub fn contains(&self, name: &str) -> bool {
        self.commands.contains_key(name)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VnCommandDescriptor {
    pub name: String,
    pub family: VnCommandFamily,
    pub passthrough: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum VnCommandFamily {
    Flow,
    Scene,
    Audio,
    Video,
    Ui,
    Asset,
    Runtime,
    Custom(String),
}
