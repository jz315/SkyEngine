use serde::{Deserialize, Serialize};

use crate::vn::script::YarnCommand;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VnAssetState {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub intents: Vec<VnAssetIntent>,
}

impl VnAssetState {
    pub fn apply_command(&mut self, command: &YarnCommand) -> Option<Vec<VnAssetIntent>> {
        let kind = match command.name.as_str() {
            "preload" => VnAssetIntentKind::Preload,
            "release" => VnAssetIntentKind::Release,
            _ => return None,
        };
        let intents: Vec<_> = command
            .positional_args()
            .map(|arg| VnAssetIntent {
                kind: kind.clone(),
                asset: arg.raw.clone(),
            })
            .collect();
        if intents.is_empty() {
            return None;
        }
        self.intents.extend(intents.iter().cloned());
        Some(intents)
    }

    pub fn drain_intents(&mut self) -> impl Iterator<Item = VnAssetIntent> + '_ {
        self.intents.drain(..)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VnAssetIntent {
    pub kind: VnAssetIntentKind,
    pub asset: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum VnAssetIntentKind {
    Preload,
    Release,
}
