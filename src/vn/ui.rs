use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VnUiState {
    pub mode: VnUiMode,
    pub previous_mode: VnUiMode,
    pub focused_slot: usize,
    pub focused_choice: usize,
}

impl Default for VnUiState {
    fn default() -> Self {
        Self {
            mode: VnUiMode::Reading,
            previous_mode: VnUiMode::Reading,
            focused_slot: 0,
            focused_choice: 0,
        }
    }
}

impl VnUiState {
    pub fn enter(&mut self, mode: VnUiMode) {
        if self.mode != mode {
            self.previous_mode = self.mode.clone();
            self.mode = mode;
        }
    }

    pub fn back(&mut self) {
        std::mem::swap(&mut self.mode, &mut self.previous_mode);
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum VnUiMode {
    Title,
    Reading,
    Hidden,
    Menu,
    Backlog,
    Save,
    Load,
    Preferences,
    Gallery,
    Confirm(VnConfirmKind),
    Debug,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum VnConfirmKind {
    OverwriteSave,
    QuickLoad,
    ReturnToTitle,
    Quit,
}
