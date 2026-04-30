use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use super::diagnostics::VnCompileError;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VnSpan {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    pub line: u32,
    pub column: u32,
}

impl VnSpan {
    pub fn new(line: u32, column: u32) -> Self {
        Self {
            source: None,
            line,
            column,
        }
    }

    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum VnValue {
    Bool(bool),
    Number(f64),
    String(String),
}

impl VnValue {
    pub fn truthy(&self) -> bool {
        match self {
            Self::Bool(value) => *value,
            Self::Number(value) => *value != 0.0,
            Self::String(value) => !value.is_empty(),
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value.as_str()),
            _ => None,
        }
    }
}

impl fmt::Display for VnValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bool(value) => write!(f, "{value}"),
            Self::Number(value) => write!(f, "{value}"),
            Self::String(value) => f.write_str(value),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VnCommandArg {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub value: VnValue,
    pub raw: String,
}

impl VnCommandArg {
    pub fn positional(raw: impl Into<String>) -> Self {
        let raw = raw.into();
        Self {
            name: None,
            value: parse_value_literal(&raw),
            raw,
        }
    }

    pub fn named(name: impl Into<String>, raw: impl Into<String>) -> Self {
        let raw = raw.into();
        Self {
            name: Some(name.into()),
            value: parse_value_literal(&raw),
            raw,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct YarnCommand {
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<VnCommandArg>,
    pub raw: String,
    pub span: VnSpan,
}

impl YarnCommand {
    pub fn positional_args(&self) -> impl Iterator<Item = &VnCommandArg> {
        self.args.iter().filter(|arg| arg.name.is_none())
    }

    pub fn positional_values(&self) -> impl Iterator<Item = &VnValue> {
        self.positional_args().map(|arg| &arg.value)
    }

    pub fn named_arg(&self, name: &str) -> Option<&VnCommandArg> {
        self.args
            .iter()
            .find(|arg| arg.name.as_deref() == Some(name))
    }

    pub fn first_positional_raw(&self) -> Option<&str> {
        self.positional_args().next().map(|arg| arg.raw.as_str())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct YarnLine {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speaker: Option<String>,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_id: Option<String>,
    pub span: VnSpan,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct YarnChoice {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub body: Vec<YarnInstruction>,
    pub span: VnSpan,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum YarnInstruction {
    Line(YarnLine),
    Command(YarnCommand),
    Choice(YarnChoice),
}

impl YarnInstruction {
    pub fn span(&self) -> &VnSpan {
        match self {
            Self::Line(line) => &line.span,
            Self::Command(command) => &command.span,
            Self::Choice(choice) => &choice.span,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct YarnNode {
    pub title: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub body: Vec<YarnInstruction>,
    pub span: VnSpan,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct YarnScript {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nodes: Vec<YarnNode>,
}

impl YarnScript {
    pub fn parse_str(source: &str) -> Result<Self, VnCompileError> {
        Self::parse_source("<memory>", source)
    }

    pub fn parse_source(
        source_id: impl Into<String>,
        source: &str,
    ) -> Result<Self, VnCompileError> {
        let script = super::parser::parse_yarn_source(Some(source_id.into()), source)?;
        super::validate::validate_script(&script, super::validate::VnValidationOptions::default())?;
        Ok(script)
    }

    pub fn parse_unvalidated_source(
        source_id: impl Into<String>,
        source: &str,
    ) -> Result<Self, VnCompileError> {
        super::parser::parse_yarn_source(Some(source_id.into()), source)
    }

    pub fn merge(scripts: impl IntoIterator<Item = YarnScript>) -> Self {
        let mut merged = Self::default();
        for mut script in scripts {
            merged.nodes.append(&mut script.nodes);
        }
        merged
    }

    pub fn node(&self, title: &str) -> Option<&YarnNode> {
        self.nodes.iter().find(|node| node.title == title)
    }

    pub fn has_node(&self, title: &str) -> bool {
        self.node(title).is_some()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VnProjectManifest {
    #[serde(default)]
    pub title: String,
    #[serde(default = "default_start_node")]
    pub start_node: String,
    #[serde(default = "default_resolution")]
    pub resolution: [u32; 2],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_language: Option<String>,
    #[serde(default)]
    pub scripts: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub characters: BTreeMap<String, VnCharacterManifest>,
}

impl Default for VnProjectManifest {
    fn default() -> Self {
        Self {
            title: String::new(),
            start_node: default_start_node(),
            resolution: default_resolution(),
            default_language: None,
            scripts: Vec::new(),
            characters: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct VnCharacterManifest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_expression: Option<String>,
}

pub(crate) fn parse_value_literal(raw: &str) -> VnValue {
    let trimmed = raw.trim();
    if trimmed.eq_ignore_ascii_case("true") {
        return VnValue::Bool(true);
    }
    if trimmed.eq_ignore_ascii_case("false") {
        return VnValue::Bool(false);
    }
    if let Ok(value) = trimmed.parse::<f64>() {
        return VnValue::Number(value);
    }
    VnValue::String(trimmed.to_owned())
}

fn default_start_node() -> String {
    "Start".to_owned()
}

fn default_resolution() -> [u32; 2] {
    [1280, 720]
}
