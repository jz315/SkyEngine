use std::error::Error;
use std::fmt;

use serde::{Deserialize, Serialize};

use super::ast::VnSpan;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum VnDiagnosticSeverity {
    Info,
    Warning,
    Error,
}

impl fmt::Display for VnDiagnosticSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Info => f.write_str("info"),
            Self::Warning => f.write_str("warning"),
            Self::Error => f.write_str("error"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VnDiagnostic {
    pub severity: VnDiagnosticSeverity,
    pub code: String,
    pub message: String,
    pub span: VnSpan,
}

impl VnDiagnostic {
    pub fn new(
        severity: VnDiagnosticSeverity,
        code: impl Into<String>,
        message: impl Into<String>,
        span: VnSpan,
    ) -> Self {
        Self {
            severity,
            code: code.into(),
            message: message.into(),
            span,
        }
    }

    pub fn error(code: impl Into<String>, message: impl Into<String>, span: VnSpan) -> Self {
        Self::new(VnDiagnosticSeverity::Error, code, message, span)
    }

    pub fn warning(code: impl Into<String>, message: impl Into<String>, span: VnSpan) -> Self {
        Self::new(VnDiagnosticSeverity::Warning, code, message, span)
    }

    pub fn is_error(&self) -> bool {
        self.severity == VnDiagnosticSeverity::Error
    }
}

impl fmt::Display for VnDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(source) = &self.span.source {
            write!(
                f,
                "[vn][{}][{}] {}:{}:{} {}",
                self.severity, self.code, source, self.span.line, self.span.column, self.message
            )
        } else {
            write!(
                f,
                "[vn][{}][{}] {}:{} {}",
                self.severity, self.code, self.span.line, self.span.column, self.message
            )
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VnCompileError {
    pub diagnostics: Vec<VnDiagnostic>,
}

impl VnCompileError {
    pub fn new(diagnostics: Vec<VnDiagnostic>) -> Self {
        debug_assert!(!diagnostics.is_empty());
        Self { diagnostics }
    }

    pub fn single(diagnostic: VnDiagnostic) -> Self {
        Self::new(vec![diagnostic])
    }

    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(VnDiagnostic::is_error)
    }
}

impl fmt::Display for VnCompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.diagnostics.len() == 1 {
            return write!(f, "{}", self.diagnostics[0]);
        }

        write!(f, "{} VN diagnostics", self.diagnostics.len())?;
        if let Some(first) = self.diagnostics.first() {
            write!(f, "; first: {first}")?;
        }
        Ok(())
    }
}

impl Error for VnCompileError {}
