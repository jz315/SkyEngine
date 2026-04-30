use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::vn::script::YarnCommand;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VnProgressState {
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub checkpoints: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub unlocked_cg: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub notifications: BTreeSet<String>,
}

impl VnProgressState {
    pub fn apply_command(&mut self, command: &YarnCommand) -> Option<VnProgressChange> {
        match command.name.as_str() {
            "checkpoint" => {
                let id = command
                    .first_positional_raw()
                    .map(str::to_owned)
                    .unwrap_or_else(|| "checkpoint".to_owned());
                self.checkpoints.insert(id.clone());
                Some(VnProgressChange::Checkpoint(id))
            }
            "unlock_cg" => {
                let id = command.first_positional_raw()?.to_owned();
                self.unlocked_cg.insert(id.clone());
                Some(VnProgressChange::UnlockCg(id))
            }
            "notify" => {
                let message = command
                    .positional_args()
                    .map(|arg| arg.raw.as_str())
                    .collect::<Vec<_>>()
                    .join(" ");
                if message.is_empty() {
                    return None;
                }
                self.notifications.insert(message.clone());
                Some(VnProgressChange::Notify(message))
            }
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum VnProgressChange {
    Checkpoint(String),
    UnlockCg(String),
    Notify(String),
}
