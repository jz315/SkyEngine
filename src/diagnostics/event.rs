use std::fmt;

use crate::ecs::EntityId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DiagnosticSeverity {
    Info,
    Warning,
    Error,
}

impl fmt::Display for DiagnosticSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Info => f.write_str("info"),
            Self::Warning => f.write_str("warning"),
            Self::Error => f.write_str("error"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DiagnosticId(String);

impl DiagnosticId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    #[inline]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for DiagnosticId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for DiagnosticId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl fmt::Display for DiagnosticId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DiagnosticSubsystem(String);

impl DiagnosticSubsystem {
    pub fn new(subsystem: impl Into<String>) -> Self {
        Self(subsystem.into())
    }

    pub fn engine() -> Self {
        Self::new("engine")
    }

    pub fn ecs() -> Self {
        Self::new("ecs")
    }

    pub fn render() -> Self {
        Self::new("render")
    }

    pub fn asset() -> Self {
        Self::new("asset")
    }

    pub fn gpu() -> Self {
        Self::new("gpu")
    }

    pub fn app() -> Self {
        Self::new("app")
    }

    pub fn input() -> Self {
        Self::new("input")
    }

    pub fn audio() -> Self {
        Self::new("audio")
    }

    pub fn live2d() -> Self {
        Self::new("live2d")
    }

    #[inline]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for DiagnosticSubsystem {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for DiagnosticSubsystem {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl Default for DiagnosticSubsystem {
    fn default() -> Self {
        Self::engine()
    }
}

impl fmt::Display for DiagnosticSubsystem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DiagnosticKey(String);

impl DiagnosticKey {
    pub fn new(key: impl Into<String>) -> Self {
        Self(key.into())
    }

    #[inline]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for DiagnosticKey {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for DiagnosticKey {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl fmt::Display for DiagnosticKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiagnosticField {
    pub name: String,
    pub value: String,
}

impl DiagnosticField {
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
        }
    }
}

#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiagnosticEvent {
    pub sequence: u64,
    pub id: DiagnosticId,
    pub subsystem: DiagnosticSubsystem,
    pub severity: DiagnosticSeverity,
    pub title: String,
    pub message: String,
    pub help: Option<String>,
    pub entity: Option<EntityId>,
    pub frame: Option<u64>,
    pub fields: Vec<DiagnosticField>,
    pub once_key: Option<DiagnosticKey>,
}

impl DiagnosticEvent {
    pub fn new(
        id: impl Into<DiagnosticId>,
        subsystem: impl Into<DiagnosticSubsystem>,
        severity: DiagnosticSeverity,
        message: impl Into<String>,
    ) -> Self {
        let id = id.into();
        Self {
            sequence: 0,
            title: id.as_str().to_owned(),
            id,
            subsystem: subsystem.into(),
            severity,
            message: message.into(),
            help: None,
            entity: None,
            frame: None,
            fields: Vec::new(),
            once_key: None,
        }
    }

    pub fn info(
        id: impl Into<DiagnosticId>,
        subsystem: impl Into<DiagnosticSubsystem>,
        message: impl Into<String>,
    ) -> Self {
        Self::new(id, subsystem, DiagnosticSeverity::Info, message)
    }

    pub fn warning(
        id: impl Into<DiagnosticId>,
        subsystem: impl Into<DiagnosticSubsystem>,
        message: impl Into<String>,
    ) -> Self {
        Self::new(id, subsystem, DiagnosticSeverity::Warning, message)
    }

    pub fn error(
        id: impl Into<DiagnosticId>,
        subsystem: impl Into<DiagnosticSubsystem>,
        message: impl Into<String>,
    ) -> Self {
        Self::new(id, subsystem, DiagnosticSeverity::Error, message)
    }

    pub fn with_entity(mut self, entity: EntityId) -> Self {
        self.entity = Some(entity);
        self
    }

    pub fn with_frame(mut self, frame: u64) -> Self {
        self.frame = Some(frame);
        self
    }

    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    pub fn with_field(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.fields.push(DiagnosticField::new(name, value));
        self
    }

    pub fn field(&self, name: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|field| field.name == name)
            .map(|field| field.value.as_str())
    }

    pub fn with_once_key(mut self, key: impl Into<DiagnosticKey>) -> Self {
        self.once_key = Some(key.into());
        self
    }

    pub(super) fn dedup_key(&self) -> DiagnosticKey {
        if let Some(key) = &self.once_key {
            return key.clone();
        }

        if let Some(entity) = self.entity {
            return DiagnosticKey::new(format!(
                "{}:entity:{}:{}",
                self.id.as_str(),
                entity.index(),
                entity.generation()
            ));
        }

        DiagnosticKey::new(self.id.as_str().to_owned())
    }
}

impl fmt::Display for DiagnosticEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[SkyEngine][{}][{}] {}",
            self.subsystem, self.severity, self.title
        )?;
        if !self.message.is_empty() {
            write!(f, ": {}", self.message)?;
        }
        if let Some(help) = &self.help {
            if !help.is_empty() {
                write!(f, " help: {help}")?;
            }
        }
        if let Some(entity) = self.entity {
            write!(f, " entity={entity:?}")?;
        }
        if let Some(frame) = self.frame {
            write!(f, " frame={frame}")?;
        }
        for field in &self.fields {
            write!(f, " {}={}", field.name, field.value)?;
        }
        Ok(())
    }
}
