use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::vn::script::YarnLine;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct VnLocalizationTable {
    #[serde(default)]
    pub language: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub lines: BTreeMap<String, VnLocalizedLine>,
}

impl VnLocalizationTable {
    pub fn from_toml_str(text: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(text)
    }

    pub fn line(&self, line_id: &str) -> Option<&VnLocalizedLine> {
        self.lines.get(line_id)
    }

    pub fn localize_line(&self, line: &YarnLine) -> YarnLine {
        let Some(line_id) = &line.line_id else {
            return line.clone();
        };
        let Some(localized) = self.line(line_id) else {
            return line.clone();
        };

        YarnLine {
            speaker: localized.speaker.clone().or_else(|| line.speaker.clone()),
            text: localized.text.clone().unwrap_or_else(|| line.text.clone()),
            line_id: line.line_id.clone(),
            span: line.span.clone(),
        }
    }

    pub fn voice_for(&self, line_id: &str) -> Option<&str> {
        self.line(line_id).and_then(|line| line.voice.as_deref())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct VnLocalizedLine {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speaker: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice: Option<String>,
}

#[cfg(test)]
mod tests {
    use crate::vn::script::{VnSpan, YarnLine};

    use super::*;

    #[test]
    fn localization_replaces_text_and_keeps_line_id() {
        let table = VnLocalizationTable::from_toml_str(
            r#"
language = "en-US"

[lines."start.alice.0001"]
speaker = "Alice"
text = "Good morning."
voice = "voice/en/alice/0001.ogg"
"#,
        )
        .unwrap();

        let line = YarnLine {
            speaker: Some("爱丽丝".to_owned()),
            text: "早上好。".to_owned(),
            line_id: Some("start.alice.0001".to_owned()),
            span: VnSpan::new(1, 1),
        };

        let localized = table.localize_line(&line);
        assert_eq!(localized.speaker.as_deref(), Some("Alice"));
        assert_eq!(localized.text, "Good morning.");
        assert_eq!(
            table.voice_for("start.alice.0001"),
            Some("voice/en/alice/0001.ogg")
        );
    }
}
