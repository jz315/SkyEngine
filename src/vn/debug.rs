use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::vn::runtime::VnRuntime;
use crate::vn::script::VnValue;
use crate::vn::{VnStatus, VnVideoPlayback};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VnDebugState {
    pub current_node: String,
    pub instruction_index: usize,
    pub status: VnStatus,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub variables: BTreeMap<String, VnValue>,
    pub visible_actor_count: usize,
    pub backlog_len: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_bgm: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_voice: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_video: Option<VnVideoPlayback>,
    pub pending_asset_intents: usize,
    pub checkpoint_count: usize,
    pub unlocked_cg_count: usize,
    pub wait_remaining: Option<f32>,
}

impl VnDebugState {
    pub fn from_runtime(runtime: &VnRuntime) -> Self {
        Self {
            current_node: runtime.current_node().to_owned(),
            instruction_index: runtime.instruction_index(),
            status: runtime.status().clone(),
            variables: runtime.variables().clone(),
            visible_actor_count: runtime
                .scene()
                .actors
                .values()
                .filter(|actor| actor.visible)
                .count(),
            backlog_len: runtime.dialogue().backlog.len(),
            current_bgm: runtime.audio().bgm.as_ref().map(|bgm| bgm.asset.clone()),
            current_voice: runtime
                .audio()
                .voice
                .as_ref()
                .map(|voice| voice.asset.clone()),
            active_video: runtime.video().active.clone(),
            pending_asset_intents: runtime.assets().intents.len(),
            checkpoint_count: runtime.progress().checkpoints.len(),
            unlocked_cg_count: runtime.progress().unlocked_cg.len(),
            wait_remaining: runtime.wait_remaining(),
        }
    }
}
