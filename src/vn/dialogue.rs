use serde::{Deserialize, Serialize};

use crate::vn::script::YarnLine;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct VnDialogueState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_line: Option<YarnLine>,
    #[serde(default)]
    pub reveal_chars: usize,
    #[serde(default)]
    pub line_complete: bool,
    #[serde(default)]
    pub selected_choice: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<VnDialogueChoice>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub backlog: Vec<VnBacklogLine>,
}

impl VnDialogueState {
    pub fn present_line(&mut self, line: YarnLine) {
        self.backlog.push(VnBacklogLine::from_line(&line));
        self.current_line = Some(line);
        self.reveal_chars = 0;
        self.line_complete = false;
        self.selected_choice = 0;
        self.choices.clear();
    }

    pub fn complete_line(&mut self) {
        if let Some(line) = &self.current_line {
            self.reveal_chars = line.text.chars().count();
        }
        self.line_complete = true;
    }

    pub fn advance_reveal(&mut self, chars: usize) -> bool {
        let Some(line) = &self.current_line else {
            self.line_complete = true;
            return true;
        };
        let len = line.text.chars().count();
        self.reveal_chars = self.reveal_chars.saturating_add(chars).min(len);
        self.line_complete = self.reveal_chars >= len;
        self.line_complete
    }

    pub fn visible_text(&self) -> String {
        let Some(line) = &self.current_line else {
            return String::new();
        };
        line.text.chars().take(self.reveal_chars).collect()
    }

    pub fn set_choices(&mut self, choices: Vec<VnDialogueChoice>) {
        self.choices = choices;
        self.current_line = None;
        self.reveal_chars = 0;
        self.line_complete = true;
        self.selected_choice = 0;
    }

    pub fn clear_choices(&mut self) {
        self.choices.clear();
        self.selected_choice = 0;
    }

    pub fn select_next_choice(&mut self) {
        if !self.choices.is_empty() {
            self.selected_choice = (self.selected_choice + 1) % self.choices.len();
        }
    }

    pub fn select_previous_choice(&mut self) {
        if !self.choices.is_empty() {
            self.selected_choice = if self.selected_choice == 0 {
                self.choices.len() - 1
            } else {
                self.selected_choice - 1
            };
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VnDialogueChoice {
    pub text: String,
    pub source_index: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VnBacklogLine {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speaker: Option<String>,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice: Option<String>,
}

impl VnBacklogLine {
    pub fn from_line(line: &YarnLine) -> Self {
        Self {
            speaker: line.speaker.clone(),
            text: line.text.clone(),
            line_id: line.line_id.clone(),
            voice: None,
        }
    }
}
