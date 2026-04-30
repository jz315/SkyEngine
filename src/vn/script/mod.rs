//! Yarn-style script documents, diagnostics, project manifests, and validation.

mod ast;
mod compiler;
mod diagnostics;
mod parser;
mod project;
mod validate;

pub(crate) use ast::parse_value_literal;
pub use ast::{
    VnCharacterManifest, VnCommandArg, VnProjectManifest, VnSpan, VnValue, YarnChoice, YarnCommand,
    YarnInstruction, YarnLine, YarnNode, YarnScript,
};
pub use compiler::compile_yarn_source;
pub use diagnostics::{VnCompileError, VnDiagnostic, VnDiagnosticSeverity};
pub use project::{YarnProject, YarnProjectLoadError};
pub use validate::{validate_script, VnValidationOptions};

#[cfg(test)]
mod tests;
