use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum VnAction {
    Advance,
    Choice(usize),
    Skip,
    Auto,
    HideUi,
    Menu,
    Backlog,
    QuickSave,
    QuickLoad,
    Confirm,
    Cancel,
    Up,
    Down,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VnInputState {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pending: Vec<VnAction>,
}

impl VnInputState {
    pub fn push(&mut self, action: VnAction) {
        self.pending.push(action);
    }

    pub fn pending(&self) -> &[VnAction] {
        &self.pending
    }

    pub fn drain(&mut self) -> impl Iterator<Item = VnAction> + '_ {
        self.pending.drain(..)
    }

    pub fn clear(&mut self) {
        self.pending.clear();
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct VnPlaybackState {
    pub auto_mode: bool,
    pub skip_mode: bool,
    pub ui_hidden: bool,
    pub selected_choice: usize,
}
