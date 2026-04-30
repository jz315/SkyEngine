use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use super::ast::{VnProjectManifest, YarnScript};
use super::diagnostics::{VnCompileError, VnDiagnostic};
use super::validate::{root_span, validate_script, VnValidationOptions};

#[derive(Clone, Debug, PartialEq)]
pub struct YarnProject {
    pub manifest: VnProjectManifest,
    pub script: YarnScript,
    pub root: PathBuf,
}

impl YarnProject {
    pub fn new(
        start_node: impl Into<String>,
        script: YarnScript,
    ) -> Result<Self, YarnProjectLoadError> {
        let manifest = VnProjectManifest {
            start_node: start_node.into(),
            ..VnProjectManifest::default()
        };
        Self::from_parts(manifest, script, PathBuf::new())
    }

    pub fn from_parts(
        manifest: VnProjectManifest,
        script: YarnScript,
        root: PathBuf,
    ) -> Result<Self, YarnProjectLoadError> {
        let project = Self {
            manifest,
            script,
            root,
        };
        project.validate()?;
        Ok(project)
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self, YarnProjectLoadError> {
        let path = path.as_ref();
        let text = fs::read_to_string(path).map_err(|source| YarnProjectLoadError::Io {
            path: path.to_path_buf(),
            source: source.to_string(),
        })?;
        let root = path.parent().unwrap_or_else(|| Path::new("")).to_path_buf();
        Self::from_manifest_str(&text, root)
    }

    pub fn from_manifest_str(
        text: &str,
        root: impl Into<PathBuf>,
    ) -> Result<Self, YarnProjectLoadError> {
        let root = root.into();
        let manifest: VnProjectManifest =
            toml::from_str(text).map_err(|source| YarnProjectLoadError::Manifest {
                source: source.to_string(),
            })?;
        if manifest.scripts.is_empty() {
            return Err(YarnProjectLoadError::Compile(VnCompileError::single(
                VnDiagnostic::error(
                    "VN200",
                    "project manifest must list at least one Yarn script",
                    root_span(),
                ),
            )));
        }

        let mut scripts = Vec::new();
        for script_path in &manifest.scripts {
            let full_path = root.join(script_path);
            let source =
                fs::read_to_string(&full_path).map_err(|source| YarnProjectLoadError::Io {
                    path: full_path.clone(),
                    source: source.to_string(),
                })?;
            scripts.push(
                YarnScript::parse_source(script_path, &source)
                    .map_err(YarnProjectLoadError::Compile)?,
            );
        }

        Self::from_parts(manifest, YarnScript::merge(scripts), root)
    }

    pub fn start_node(&self) -> &str {
        &self.manifest.start_node
    }

    pub fn validate(&self) -> Result<(), YarnProjectLoadError> {
        if !self.script.has_node(&self.manifest.start_node) {
            return Err(YarnProjectLoadError::Compile(VnCompileError::single(
                VnDiagnostic::error(
                    "VN201",
                    format!("start node '{}' does not exist", self.manifest.start_node),
                    root_span(),
                ),
            )));
        }
        validate_script(&self.script, VnValidationOptions::default())
            .map_err(YarnProjectLoadError::Compile)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum YarnProjectLoadError {
    Io { path: PathBuf, source: String },
    Manifest { source: String },
    Compile(VnCompileError),
}

impl fmt::Display for YarnProjectLoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => {
                write!(f, "failed to read '{}': {source}", path.display())
            }
            Self::Manifest { source } => write!(f, "invalid VN project manifest: {source}"),
            Self::Compile(source) => write!(f, "{source}"),
        }
    }
}

impl Error for YarnProjectLoadError {}
